use novaproxy_lib::core::hot_reload::{
    try_hot_reload_config, try_hot_switch_proxy, try_smart_hot_switch,
};
use novaproxy_lib::core::CoreManager;
use novaproxy_lib::model::node::{ProtocolConfig, ProtocolType, ProxyNode, VlessConfig};
use novaproxy_lib::monitor::health::HealthMonitor;
use novaproxy_lib::storage::atomic::atomic_write_file;
use novaproxy_lib::storage::Database;
use novaproxy_lib::system::network::NetworkChangeListener;
use novaproxy_lib::system::sysproxy::SystemProxyManager;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_atomic_file_write_operations() {
    let temp_dir =
        std::env::temp_dir().join(format!("novaproxy_test_atomic_{}", uuid::Uuid::new_v4()));
    let target_file = temp_dir.join("config").join("runtime.json");

    // 1. Initial write to deeply nested non-existent directory
    let content1 = b"{\"mode\": \"initial\", \"port\": 1080}";
    atomic_write_file(&target_file, content1).expect("Initial atomic write should succeed");

    assert!(target_file.exists());
    let read1 = std::fs::read(&target_file).expect("Read written content");
    assert_eq!(read1, content1);

    // 2. Overwrite existing file atomically
    let content2 = b"{\"mode\": \"updated\", \"port\": 7890}";
    atomic_write_file(&target_file, content2).expect("Atomic overwrite should succeed");

    let read2 = std::fs::read(&target_file).expect("Read overwritten content");
    assert_eq!(read2, content2);

    // 3. Ensure no temporary files were leaked in the parent directory
    let parent = target_file.parent().unwrap();
    let entries: Vec<_> = std::fs::read_dir(parent)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], "runtime.json");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_database_integrity_and_backup_operations() {
    let temp_dir = std::env::temp_dir().join(format!("novaproxy_test_db_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let db_path = temp_dir.join("source.db");
    let backup_path = temp_dir.join("backup.db");

    let db = Database::open(&db_path).expect("Create test database");

    // Verify integrity of fresh database
    let ok = db.check_integrity().expect("Integrity check");
    assert!(ok, "Fresh database must pass integrity check");

    // Insert sample data
    let node = ProxyNode {
        id: "backup-test-node".into(),
        name: "Backup Test Node".into(),
        server: "1.1.1.1".into(),
        port: 443,
        protocol: ProtocolType::Vless,
        tls: None,
        transport: None,
        subscription_id: None,
        group_id: None,
        settings: ProtocolConfig::Vless(VlessConfig {
            uuid: "00000000-0000-0000-0000-000000000099".into(),
            flow: None,
            encryption: "none".into(),
            packet_encoding: None,
        }),
        latency: Some(15),
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    };
    db.upsert_node(&node).expect("Insert node");

    // Create online backup
    db.backup_database(&backup_path).expect("Create backup");
    assert!(backup_path.exists());

    // Verify backup database can be opened and contains data
    let backup_db = Database::open(&backup_path).expect("Open backup database");
    let backup_ok = backup_db.check_integrity().expect("Backup integrity check");
    assert!(backup_ok, "Backup database must pass integrity check");

    let retrieved = backup_db
        .get_node("backup-test-node")
        .expect("Query from backup")
        .expect("Node must exist in backup");
    assert_eq!(retrieved.name, "Backup Test Node");
    assert_eq!(retrieved.port, 443);

    drop(db);
    drop(backup_db);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_hot_reload_offline_graceful_error() {
    let unused_port = 48291;
    let switch_res = try_hot_switch_proxy(unused_port, None, "proxy", "node-x").await;
    assert!(switch_res.is_err());

    let smart_res = try_smart_hot_switch(unused_port, None, "node-id-1", "node-name-1").await;
    assert!(smart_res.is_err());

    let reload_res = try_hot_reload_config(unused_port, None, "C:\\nonexistent\\config.json").await;
    assert!(reload_res.is_err());
}

#[tokio::test]
async fn test_health_monitor_failover_integration() {
    let db = Database::open_in_memory().unwrap();
    let core_manager = CoreManager::new(std::env::temp_dir(), std::env::temp_dir());

    let node1 = ProxyNode {
        id: "fail-1".into(),
        name: "Unhealthy Node 1".into(),
        server: "127.0.0.1".into(),
        port: 48991,
        protocol: ProtocolType::Vless,
        tls: None,
        transport: None,
        subscription_id: None,
        group_id: None,
        settings: ProtocolConfig::Vless(VlessConfig {
            uuid: "00000000-0000-0000-0000-000000000001".into(),
            flow: None,
            encryption: "none".into(),
            packet_encoding: None,
        }),
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    };

    let node2 = ProxyNode {
        id: "healthy-2".into(),
        name: "Healthy Fallback Node 2".into(),
        server: "127.0.0.1".into(),
        port: 48992,
        protocol: ProtocolType::Vless,
        tls: None,
        transport: None,
        subscription_id: None,
        group_id: None,
        settings: ProtocolConfig::Vless(VlessConfig {
            uuid: "00000000-0000-0000-0000-000000000002".into(),
            flow: None,
            encryption: "none".into(),
            packet_encoding: None,
        }),
        latency: Some(25),
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 1,
    };

    db.upsert_node(&node1).unwrap();
    db.upsert_node(&node2).unwrap();

    let mut settings = db.get_app_settings().unwrap();
    settings.active_node_id = Some("fail-1".into());
    db.save_app_settings(&settings).unwrap();

    let monitor = HealthMonitor::new(db.clone(), core_manager).with_config(
        Duration::from_millis(10),
        Duration::from_millis(50),
        2,
    );

    // Initial state: 0 failures, no failover
    let initial_failover = monitor.evaluate_failover().await.unwrap();
    assert!(initial_failover.is_none());

    // Single failure: no failover
    let _ = monitor.check_once().await;
    let single_failover = monitor.evaluate_failover().await.unwrap();
    assert!(single_failover.is_none());

    // Second failure: triggers failover
    let _ = monitor.check_once().await;
    let triggered_failover = monitor.evaluate_failover().await.unwrap();
    assert!(triggered_failover.is_some());

    let event = triggered_failover.unwrap();
    assert_eq!(event.previous_node_id, "fail-1");
    assert_eq!(event.new_node_id, "healthy-2");

    let updated_settings = db.get_app_settings().unwrap();
    assert_eq!(updated_settings.active_node_id, Some("healthy-2".into()));
}

#[test]
fn test_network_listener_thread_lifecycle() {
    let sysproxy = SystemProxyManager::new();
    let listener = Arc::new(NetworkChangeListener::new(sysproxy));

    let handle = listener.clone().start().expect("spawn network listener");
    assert!(listener.is_running());

    std::thread::sleep(Duration::from_millis(30));
    listener.stop();
    let _ = handle.join();
    assert!(!listener.is_running());
}
