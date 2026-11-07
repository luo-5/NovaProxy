use novaproxy_lib::core::process::check_core_config;
use novaproxy_lib::generator::generate_core_config;
use novaproxy_lib::model::node::{
    CoreType, ProtocolConfig, ProtocolType, ProxyNode, ShadowsocksConfig,
};
use novaproxy_lib::model::routing::{DnsSettings, RoutingSettings};
use novaproxy_lib::model::settings::AppSettings;
use novaproxy_lib::system::autostart::{is_autostart_enabled, set_autostart};
use novaproxy_lib::system::deeplink::parse_deep_link;
use novaproxy_lib::system::elevation::is_elevated;
use novaproxy_lib::system::sysproxy::SystemProxyManager;
use novaproxy_lib::system::tun::TunSettings;
use std::path::PathBuf;

fn locate_core(core: CoreType) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("cores")
        .join(core.binary_name())
}

#[test]
fn test_deep_link_parsing() {
    let raw =
        "novaproxy://install-config?url=https%3A%2F%2Fsub.example.com%2Fapi%2Fv1&name=PremiumVIP";
    let action = parse_deep_link(raw).expect("parse deep link");
    assert_eq!(action.subscription_url, "https://sub.example.com/api/v1");
    assert_eq!(action.name, Some("PremiumVIP".to_string()));

    let clash_raw = "clash://install-config?url=https%3A%2F%2Fsub.example.com%2Fclash";
    let clash_action = parse_deep_link(clash_raw).expect("parse clash deep link");
    assert_eq!(
        clash_action.subscription_url,
        "https://sub.example.com/clash"
    );
    assert_eq!(clash_action.name, None);
}

#[test]
fn test_sysproxy_activation_and_cleanup() {
    let initial = SystemProxyManager::query_current().expect("read initial proxy");
    let manager = SystemProxyManager::new();

    let activated = manager
        .set_proxy("127.0.0.1:28999", Some("<local>;localhost;127.*"))
        .expect("activate proxy");
    assert!(activated.enabled);
    assert_eq!(activated.server, Some("127.0.0.1:28999".to_string()));

    let cleared = manager.clear_proxy().expect("clear proxy");
    assert!(!cleared.enabled);

    manager.restore_original().expect("restore original proxy");
    let restored = SystemProxyManager::query_current().expect("read restored proxy");
    assert_eq!(restored.enabled, initial.enabled);
}

#[test]
fn test_autostart_and_elevation_checks() {
    let _ = is_elevated(); // Should not panic

    let initial = is_autostart_enabled().unwrap();
    set_autostart(true).unwrap();
    assert!(is_autostart_enabled().unwrap());

    set_autostart(false).unwrap();
    assert!(!is_autostart_enabled().unwrap());

    set_autostart(initial).unwrap();
}

#[test]
fn test_tun_settings_model() {
    let tun = TunSettings::default();
    assert_eq!(tun.interface_name, "NovaProxy-TUN");
    assert_eq!(tun.address, "172.19.0.1/30");
    assert!(tun.auto_route);
    assert!(tun.strict_route);
    assert_eq!(tun.stack, "system");
}

#[tokio::test]
async fn test_tun_mode_config_generation_and_binary_check() {
    let sb_bin = locate_core(CoreType::SingBox);
    let mihomo_bin = locate_core(CoreType::Mihomo);

    let temp_dir = std::env::temp_dir().join("novaproxy_test_tun_check");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let dummy_node = ProxyNode {
        id: "ss-tun".to_string(),
        name: "SS TUN".to_string(),
        server: "example.invalid".to_string(),
        port: 8388,
        protocol: ProtocolType::Shadowsocks,
        tls: None,
        transport: None,
        settings: ProtocolConfig::Shadowsocks(ShadowsocksConfig {
            method: "2022-blake3-aes-128-gcm".to_string(),
            password: "YWJjZGVmZ2hpamtsbW5vcA==".to_string(),
            plugin: None,
            plugin_opts: None,
            uot: Some(true),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    };

    let mut settings = AppSettings::default();
    settings.mixed_port = 27891;
    settings.tun_mode = true; // Enable TUN mode

    let dns = DnsSettings::default();
    let routing = RoutingSettings::default();

    // 1. Check sing-box with TUN mode enabled
    if sb_bin.exists() {
        let sb_cfg = generate_core_config(
            CoreType::SingBox,
            &[dummy_node.clone()],
            &[],
            &settings,
            &dns,
            &routing,
            Some("ss-tun"),
        )
        .expect("generate singbox with tun");

        assert!(sb_cfg.contains("\"type\": \"tun\""));
        assert!(sb_cfg.contains("\"interface_name\": \"NovaProxy-TUN\""));

        let sb_path = temp_dir.join("sb_tun.json");
        std::fs::write(&sb_path, sb_cfg.as_bytes()).unwrap();

        check_core_config(CoreType::SingBox, &sb_bin, &sb_path, &temp_dir)
            .await
            .expect("sing-box TUN config must pass validation");
    }

    // 2. Check mihomo with TUN mode enabled
    if mihomo_bin.exists() {
        let mihomo_cfg = generate_core_config(
            CoreType::Mihomo,
            &[dummy_node.clone()],
            &[],
            &settings,
            &dns,
            &routing,
            Some("ss-tun"),
        )
        .expect("generate mihomo with tun");

        assert!(mihomo_cfg.contains("tun:"));
        assert!(mihomo_cfg.contains("device: NovaProxy-TUN"));

        let mihomo_path = temp_dir.join("mihomo_tun.yaml");
        std::fs::write(&mihomo_path, mihomo_cfg.as_bytes()).unwrap();

        check_core_config(CoreType::Mihomo, &mihomo_bin, &mihomo_path, &temp_dir)
            .await
            .expect("mihomo TUN config must pass validation");
    }
}
