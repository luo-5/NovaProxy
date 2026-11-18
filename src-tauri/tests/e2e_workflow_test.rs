use novaproxy_lib::core::process::check_core_config;
use novaproxy_lib::core::{CoreLifecycleState, CoreManager};
use novaproxy_lib::model::{
    AppSettings, CoreType, DnsSettings, ProtocolConfig, RoutingSettings, Subscription,
};
use novaproxy_lib::parser::subscription::parse_subscription_content;
use novaproxy_lib::storage::atomic::atomic_write_file;
use novaproxy_lib::storage::Database;
use novaproxy_lib::system::deeplink::parse_deep_link;
use novaproxy_lib::system::sysproxy::SystemProxyManager;
use std::path::PathBuf;
use std::time::Duration;

fn cores_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("cores")
}

#[tokio::test]
async fn test_e2e_deeplink_subscription_hy2_vless_reality_core_switch_and_clean_exit() {
    // 1. Parse one-click deep-link URL (novaproxy:// and clash://)
    let np_link =
        "novaproxy://install-config?url=https%3A%2F%2Fexample.com%2Fsub.yaml&name=EdgeTest";
    let clash_link =
        "clash://install-config?url=https%3A%2F%2Fexample.com%2Fclash.yaml&name=ClashTest";
    let np_parsed = parse_deep_link(np_link).expect("novaproxy deeplink should parse");
    assert_eq!(np_parsed.subscription_url, "https://example.com/sub.yaml");
    assert_eq!(np_parsed.name.as_deref(), Some("EdgeTest"));

    let clash_parsed = parse_deep_link(clash_link).expect("clash deeplink should parse");
    assert_eq!(
        clash_parsed.subscription_url,
        "https://example.com/clash.yaml"
    );
    assert_eq!(clash_parsed.name.as_deref(), Some("ClashTest"));

    // 2. Parse a subscription containing Hysteria2 port-hopping + VLESS Reality Vision + Clash url-test/fallback groups
    let clash_yaml = r#"
proxies:
  - name: "Hy2 Hopping Test"
    type: hysteria2
    server: 198.51.100.10
    port: 443
    ports: 20000-30000
    hop-interval: 30
    password: "test-hy2-password"
    sni: hy2.example.com
    up: "50 Mbps"
    down: "200 Mbps"
    obfs: salamander
    obfs-password: "test-obfs-password"
  - name: "VLESS Reality Test"
    type: vless
    server: 198.51.100.20
    port: 443
    uuid: 00000000-0000-0000-0000-000000000001
    network: tcp
    tls: true
    servername: learn.microsoft.com
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    reality-opts:
      public-key: AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE
      short-id: 0123456789abcdef
proxy-groups:
  - name: "Auto URLTest"
    type: url-test
    url: http://www.gstatic.com/generate_204
    interval: 180
    proxies:
      - Hy2 Hopping Test
      - VLESS Reality Test
  - name: "Auto Fallback"
    type: fallback
    url: http://www.gstatic.com/generate_204
    interval: 300
    proxies:
      - VLESS Reality Test
      - Hy2 Hopping Test
"#;

    let report = parse_subscription_content(clash_yaml).expect("subscription YAML should parse");
    assert_eq!(report.nodes.len(), 2);
    assert_eq!(report.groups.len(), 2);

    // 3. Persist into SQLite DB & verify sync_groups remaps real saved node IDs
    let db = Database::open_in_memory().expect("in-memory DB");
    let sub = Subscription {
        id: "sub-e2e-1".to_string(),
        name: "EdgeTest".to_string(),
        url: "https://example.com/sub.yaml".to_string(),
        enabled: true,
        user_agent: None,
        update_interval_mins: None,
        use_proxy: false,
        updated_at: Some(1_700_000_000),
        upload: None,
        download: None,
        total: None,
        expire: None,
        home_url: None,
        sort_order: 0,
    };
    db.upsert_subscription(&sub).expect("save sub");

    let saved_nodes = db
        .sync_subscription_nodes(&sub.id, report.nodes.clone())
        .expect("sync nodes");
    let saved_groups = db
        .sync_groups(Some(&sub.id), &report.nodes, &saved_nodes, report.groups)
        .expect("sync groups");
    assert_eq!(saved_nodes.len(), 2);
    assert_eq!(saved_groups.len(), 2);

    let hy2_node = saved_nodes
        .iter()
        .find(|n| matches!(n.settings, ProtocolConfig::Hysteria2(_)))
        .expect("Hy2 node persisted");
    if let ProtocolConfig::Hysteria2(h) = &hy2_node.settings {
        assert_eq!(h.ports.as_deref(), Some("20000-30000"));
        assert_eq!(h.hop_interval, Some(30));
    }
    let vless_node = saved_nodes
        .iter()
        .find(|n| matches!(n.settings, ProtocolConfig::Vless(_)))
        .expect("VLESS node persisted");

    // 4. Plan & validate Hysteria2 hopping runtime config with real sing-box & mihomo binaries,
    //    then start CoreManager cleanly, switch to VLESS Reality, re-validate, and stop cleanly.
    let work_dir = std::env::temp_dir().join("novaproxy_e2e_workflow");
    let _ = std::fs::remove_dir_all(&work_dir);
    let mgr = CoreManager::new(cores_dir(), work_dir.clone());
    let dns = DnsSettings::default();
    let routing = RoutingSettings::default();
    let settings = AppSettings {
        mixed_port: 39180,
        clash_api_port: 39181,
        auto_port_fallback: true,
        system_proxy: false,
        tun_mode: false,
        ..Default::default()
    };

    // Plan + core binary check for Hy2 (sing-box + mihomo)
    std::fs::create_dir_all(&work_dir).expect("create work_dir");
    let hy2_req = mgr
        .plan_start_request(
            Some(CoreType::SingBox),
            None,
            None,
            None,
            &saved_nodes,
            &saved_groups,
            &settings,
            &dns,
            &routing,
            Some(&hy2_node.id),
        )
        .expect("plan hy2 sing-box request");
    let hy2_cfg = hy2_req.config_json.as_ref().expect("hy2 config");
    assert!(hy2_cfg.contains("\"server_ports\""));
    assert!(hy2_cfg.contains("20000:30000"));
    assert!(hy2_cfg.contains("\"hop_interval\""));
    let hy2_sb_path = work_dir.join("hy2_singbox.json");
    atomic_write_file(&hy2_sb_path, hy2_cfg.as_bytes()).expect("write hy2 sing-box config");
    check_core_config(
        CoreType::SingBox,
        &cores_dir().join("sing-box.exe"),
        &hy2_sb_path,
        &work_dir,
    )
    .await
    .expect("sing-box binary check for hy2");

    let hy2_mihomo_req = mgr
        .plan_start_request(
            Some(CoreType::Mihomo),
            None,
            None,
            None,
            &saved_nodes,
            &saved_groups,
            &settings,
            &dns,
            &routing,
            Some(&hy2_node.id),
        )
        .expect("plan hy2 mihomo request");
    let hy2_yaml = hy2_mihomo_req.config_json.as_ref().expect("mihomo yaml");
    assert!(hy2_yaml.contains("ports: \"20000-30000\""));
    assert!(hy2_yaml.contains("hop-interval: 30"));
    let hy2_mihomo_path = work_dir.join("hy2_mihomo.yaml");
    atomic_write_file(&hy2_mihomo_path, hy2_yaml.as_bytes()).expect("write hy2 mihomo config");
    check_core_config(
        CoreType::Mihomo,
        &cores_dir().join("mihomo.exe"),
        &hy2_mihomo_path,
        &work_dir,
    )
    .await
    .expect("mihomo check for hy2");

    // Start Hy2 live core
    let st_hy2 = mgr.start(hy2_req).await.expect("start Hy2 live core");
    assert_eq!(st_hy2.lifecycle, CoreLifecycleState::Running);
    assert!(st_hy2.pid.is_some());

    // Switch to VLESS Reality & validate + restart cleanly
    let vless_req = mgr
        .plan_start_request(
            Some(CoreType::SingBox),
            None,
            None,
            None,
            &saved_nodes,
            &saved_groups,
            &settings,
            &dns,
            &routing,
            Some(&vless_node.id),
        )
        .expect("plan vless reality request");
    let vless_cfg = vless_req.config_json.as_ref().expect("vless config");
    assert!(vless_cfg.contains("xtls-rprx-vision"));
    assert!(vless_cfg.contains("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"));
    let vless_sb_path = work_dir.join("vless_singbox.json");
    atomic_write_file(&vless_sb_path, vless_cfg.as_bytes()).expect("write vless sing-box config");
    check_core_config(
        CoreType::SingBox,
        &cores_dir().join("sing-box.exe"),
        &vless_sb_path,
        &work_dir,
    )
    .await
    .expect("sing-box check for vless reality");

    let st_vless = mgr
        .restart(vless_req)
        .await
        .expect("restart onto VLESS Reality");
    assert_eq!(st_vless.lifecycle, CoreLifecycleState::Running);
    assert!(st_vless.pid.is_some());

    tokio::time::sleep(Duration::from_millis(250)).await;
    let stopped = mgr.stop().await.expect("graceful stop");
    assert_eq!(stopped.lifecycle, CoreLifecycleState::Stopped);
    assert!(stopped.pid.is_none());
    let _ = std::fs::remove_dir_all(&work_dir);

    // 5. Capture WinINet baseline, run one non-destructive toggle+restore cycle,
    //    and ensure original state is identically restored.
    let baseline = SystemProxyManager::query_current().expect("capture baseline sysproxy");
    let proxy_mgr = SystemProxyManager::new();
    let mid = proxy_mgr
        .set_proxy("127.0.0.1:39180", Some("<local>;localhost;127.*"))
        .expect("enable temporary test proxy");
    assert!(mid.enabled);
    assert_eq!(mid.server.as_deref(), Some("127.0.0.1:39180"));
    proxy_mgr
        .restore_original()
        .expect("restore original sysproxy baseline");
    let restored = SystemProxyManager::query_current().expect("capture restored sysproxy");
    assert_eq!(restored.enabled, baseline.enabled);
    assert_eq!(restored.server, baseline.server);
    assert_eq!(restored.bypass, baseline.bypass);
}
