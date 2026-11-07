use novaproxy_lib::core::process::check_core_config;
use novaproxy_lib::generator::{generate_core_config, generate_multi_core_relay};
use novaproxy_lib::model::node::{
    AnyTlsConfig, CoreType, Hysteria2Config, HysteriaConfig, MieruConfig, ProtocolConfig,
    ProtocolType, ProxyNode, RealityConfig, ShadowsocksConfig, ShadowsocksRConfig, SnellConfig,
    Socks5Config, SshConfig, TlsConfig, TransportConfig, TrojanConfig, TuicConfig, VlessConfig,
    VmessConfig, WireGuardConfig,
};
use novaproxy_lib::model::routing::{DnsSettings, RoutingSettings, RuleAction, RuleItem, RuleType};
use novaproxy_lib::model::settings::AppSettings;
use novaproxy_lib::model::subscription::{GroupType, NodeGroup};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn locate_core(core: CoreType) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("cores")
        .join(core.binary_name())
}

fn sample_test_nodes() -> Vec<ProxyNode> {
    vec![
        // 1. Shadowsocks 2022
        ProxyNode {
            id: "node-ss".to_string(),
            name: "SS-2022-Node".to_string(),
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
        },
        // 2. ShadowsocksR (for mihomo)
        ProxyNode {
            id: "node-ssr".to_string(),
            name: "SSR-Node".to_string(),
            server: "example.invalid".to_string(),
            port: 8389,
            protocol: ProtocolType::ShadowsocksR,
            tls: None,
            transport: None,
            settings: ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
                method: "aes-256-cfb".to_string(),
                password: "password123".to_string(),
                protocol: "auth_aes128_md5".to_string(),
                protocol_param: None,
                obfs: "tls1.2_ticket_auth".to_string(),
                obfs_param: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 1,
        },
        // 3. VMess + WS + TLS
        ProxyNode {
            id: "node-vmess".to_string(),
            name: "VMess-WS-TLS".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Vmess,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec!["h2".to_string(), "http/1.1".to_string()],
                fingerprint: Some("chrome".to_string()),
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: Some(TransportConfig::Ws {
                path: "/vmess-ws".to_string(),
                host: Some("example.invalid".to_string()),
                headers: BTreeMap::new(),
                max_early_data: None,
                early_data_header_name: None,
            }),
            settings: ProtocolConfig::Vmess(VmessConfig {
                uuid: "00000000-0000-0000-0000-000000000001".to_string(),
                alter_id: 0,
                security: "auto".to_string(),
                global_padding: None,
                authenticated_length: None,
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 2,
        },
        // 4. VLESS + REALITY + Vision
        ProxyNode {
            id: "node-vless".to_string(),
            name: "VLESS-Reality".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Vless,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec![],
                fingerprint: Some("chrome".to_string()),
                reality: Some(RealityConfig {
                    public_key: "dGVzdC1wdWJsaWMta2V5LTAwMDAwMDAwMDAwMDAwMDA".to_string(),
                    short_id: "01234567".to_string(),
                    spider_x: None,
                }),
                ech_config: None,
                fragment: None,
            }),
            transport: None,
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "00000000-0000-0000-0000-000000000002".to_string(),
                flow: Some("xtls-rprx-vision".to_string()),
                encryption: "none".to_string(),
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 3,
        },
        // 5. Trojan + gRPC
        ProxyNode {
            id: "node-trojan".to_string(),
            name: "Trojan-gRPC".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Trojan,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec!["h2".to_string()],
                fingerprint: Some("chrome".to_string()),
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: Some(TransportConfig::Grpc {
                service_name: "grpc-trojan".to_string(),
                multi_mode: true,
                authority: None,
            }),
            settings: ProtocolConfig::Trojan(TrojanConfig {
                password: "trojan-password".to_string(),
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 4,
        },
        // 6. Hysteria v1
        ProxyNode {
            id: "node-hy1".to_string(),
            name: "Hy1-Node".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Hysteria,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec![],
                fingerprint: None,
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: None,
            settings: ProtocolConfig::Hysteria(HysteriaConfig {
                auth_str: Some("auth123".to_string()),
                protocol: None,
                up_mbps: 50,
                down_mbps: 100,
                obfs: None,
                recv_window_conn: None,
                recv_window: None,
                disable_mtu_discovery: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 5,
        },
        // 7. Hysteria 2
        ProxyNode {
            id: "node-hy2".to_string(),
            name: "Hy2-Node".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Hysteria2,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec![],
                fingerprint: None,
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: None,
            settings: ProtocolConfig::Hysteria2(Hysteria2Config {
                password: "hy2-password".to_string(),
                up_mbps: Some(50),
                down_mbps: Some(100),
                obfs_type: Some("salamander".to_string()),
                obfs_password: Some("salamander-key".to_string()),
                ports: None,
                hop_interval: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 6,
        },
        // 8. TUIC
        ProxyNode {
            id: "node-tuic".to_string(),
            name: "TUIC-Node".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Tuic,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec![],
                fingerprint: None,
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: None,
            settings: ProtocolConfig::Tuic(TuicConfig {
                uuid: "00000000-0000-0000-0000-000000000003".to_string(),
                password: "tuic-pass".to_string(),
                congestion_control: Some("bbr".to_string()),
                udp_relay_mode: Some("native".to_string()),
                zero_rtt_handshake: Some(true),
                heartbeat: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 7,
        },
        // 9. WireGuard
        ProxyNode {
            id: "node-wg".to_string(),
            name: "WireGuard-Node".to_string(),
            server: "198.51.100.1".to_string(),
            port: 51820,
            protocol: ProtocolType::WireGuard,
            tls: None,
            transport: None,
            settings: ProtocolConfig::WireGuard(WireGuardConfig {
                private_key: "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=".to_string(),
                public_key: "YmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmI=".to_string(),
                preshared_key: None,
                local_address: vec!["10.0.0.2/32".to_string()],
                reserved: None,
                mtu: Some(1408),
                workers: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 8,
        },
        // 10. AnyTLS
        ProxyNode {
            id: "node-anytls".to_string(),
            name: "AnyTLS-Node".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::AnyTls,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec![],
                fingerprint: Some("chrome".to_string()),
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: None,
            settings: ProtocolConfig::AnyTls(AnyTlsConfig {
                password: "anytls-pass".to_string(),
                idle_session_check_interval: None,
                idle_session_timeout: None,
                min_idle_session: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 9,
        },
        // 11. Snell (for mihomo)
        ProxyNode {
            id: "node-snell".to_string(),
            name: "Snell-Node".to_string(),
            server: "example.invalid".to_string(),
            port: 8443,
            protocol: ProtocolType::Snell,
            tls: None,
            transport: None,
            settings: ProtocolConfig::Snell(SnellConfig {
                psk: "psk-secret".to_string(),
                version: 4,
                obfs_mode: Some("tls".to_string()),
                obfs_host: Some("example.invalid".to_string()),
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 10,
        },
        // 12. Mieru (for mihomo)
        ProxyNode {
            id: "node-mieru".to_string(),
            name: "Mieru-Node".to_string(),
            server: "example.invalid".to_string(),
            port: 8443,
            protocol: ProtocolType::Mieru,
            tls: None,
            transport: None,
            settings: ProtocolConfig::Mieru(MieruConfig {
                username: "mieru-user".to_string(),
                password: "mieru-pass".to_string(),
                transport: "TCP".to_string(),
                port_range: None,
                multiplexing: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 11,
        },
        // 13. SSH
        ProxyNode {
            id: "node-ssh".to_string(),
            name: "SSH-Tunnel".to_string(),
            server: "example.invalid".to_string(),
            port: 22,
            protocol: ProtocolType::Ssh,
            tls: None,
            transport: None,
            settings: ProtocolConfig::Ssh(SshConfig {
                user: "root".to_string(),
                password: Some("ssh-pass".to_string()),
                private_key: None,
                private_key_passphrase: None,
                host_key: vec![],
                host_key_algorithms: vec![],
                client_version: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 12,
        },
        // 14. SOCKS5
        ProxyNode {
            id: "node-socks5".to_string(),
            name: "SOCKS5-Node".to_string(),
            server: "127.0.0.1".to_string(),
            port: 1080,
            protocol: ProtocolType::Socks5,
            tls: None,
            transport: None,
            settings: ProtocolConfig::Socks5(Socks5Config {
                username: None,
                password: None,
                udp: true,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 13,
        },
    ]
}

#[tokio::test]
async fn test_singbox_config_generation_and_binary_check() {
    let bin = locate_core(CoreType::SingBox);
    if !bin.exists() {
        eprintln!(
            "sing-box binary not found at {}, skipping test",
            bin.display()
        );
        return;
    }

    let temp_dir = std::env::temp_dir().join("novaproxy_test_sb_check");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let nodes = sample_test_nodes();
    let groups = vec![NodeGroup {
        id: "group-auto".to_string(),
        name: "Auto Group".to_string(),
        group_type: GroupType::UrlTest,
        node_ids: vec!["node-ss".to_string(), "node-vless".to_string()],
        test_url: Some("https://www.gstatic.com/generate_204".to_string()),
        interval_secs: Some(300),
        sort_order: 0,
    }];

    let mut settings = AppSettings::default();
    settings.mixed_port = 17890;
    settings.clash_api_port = 19090;

    let mut routing = RoutingSettings::default();
    routing.custom_rules.push(RuleItem {
        id: "r1".to_string(),
        enabled: true,
        rule_type: RuleType::DomainSuffix,
        pattern: "google.com".to_string(),
        action: RuleAction::Proxy,
        description: None,
        sort_order: 0,
    });
    routing.custom_rules.push(RuleItem {
        id: "r2".to_string(),
        enabled: true,
        rule_type: RuleType::IpCidr,
        pattern: "192.168.0.0/16".to_string(),
        action: RuleAction::Direct,
        description: None,
        sort_order: 1,
    });

    let dns = DnsSettings::default();

    let cfg_str = generate_core_config(
        CoreType::SingBox,
        &nodes,
        &groups,
        &settings,
        &dns,
        &routing,
        Some("node-vless"),
    )
    .expect("generate singbox config");

    let cfg_path = temp_dir.join("singbox_gen_test.json");
    std::fs::write(&cfg_path, cfg_str.as_bytes()).unwrap();

    check_core_config(CoreType::SingBox, &bin, &cfg_path, &temp_dir)
        .await
        .expect("sing-box configuration check must pass!");
}

#[tokio::test]
async fn test_mihomo_config_generation_and_binary_check() {
    let bin = locate_core(CoreType::Mihomo);
    if !bin.exists() {
        eprintln!(
            "mihomo binary not found at {}, skipping test",
            bin.display()
        );
        return;
    }

    let temp_dir = std::env::temp_dir().join("novaproxy_test_mihomo_check");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let nodes = sample_test_nodes();
    let groups = vec![NodeGroup {
        id: "group-auto".to_string(),
        name: "Auto Group".to_string(),
        group_type: GroupType::UrlTest,
        node_ids: vec![
            "node-ss".to_string(),
            "node-ssr".to_string(),
            "node-vless".to_string(),
        ],
        test_url: Some("https://www.gstatic.com/generate_204".to_string()),
        interval_secs: Some(300),
        sort_order: 0,
    }];

    let mut settings = AppSettings::default();
    settings.mixed_port = 27890;
    settings.clash_api_port = 29090;

    let routing = RoutingSettings::default();
    let dns = DnsSettings::default();

    let cfg_str = generate_core_config(
        CoreType::Mihomo,
        &nodes,
        &groups,
        &settings,
        &dns,
        &routing,
        Some("node-ss"),
    )
    .expect("generate mihomo config");

    let cfg_path = temp_dir.join("mihomo_gen_test.yaml");
    std::fs::write(&cfg_path, cfg_str.as_bytes()).unwrap();

    check_core_config(CoreType::Mihomo, &bin, &cfg_path, &temp_dir)
        .await
        .expect("mihomo configuration check must pass!");
}

#[tokio::test]
async fn test_xray_config_generation_and_binary_check() {
    let bin = locate_core(CoreType::Xray);
    if !bin.exists() {
        eprintln!("xray binary not found at {}, skipping test", bin.display());
        return;
    }

    let temp_dir = std::env::temp_dir().join("novaproxy_test_xray_check");
    std::fs::create_dir_all(&temp_dir).unwrap();

    // VLESS with XHTTP
    let node_xhttp = ProxyNode {
        id: "node-xhttp".to_string(),
        name: "VLESS-XHTTP-Reality".to_string(),
        server: "example.invalid".to_string(),
        port: 443,
        protocol: ProtocolType::Vless,
        tls: Some(TlsConfig {
            enabled: true,
            server_name: Some("example.invalid".to_string()),
            insecure: false,
            alpn: vec![],
            fingerprint: Some("chrome".to_string()),
            reality: Some(RealityConfig {
                public_key: "dGVzdC1wdWJsaWMta2V5LTAwMDAwMDAwMDAwMDAwMDA".to_string(),
                short_id: "01234567".to_string(),
                spider_x: None,
            }),
            ech_config: None,
            fragment: None,
        }),
        transport: Some(TransportConfig::Xhttp {
            host: Some("example.invalid".to_string()),
            path: "/xhttp".to_string(),
            mode: Some("auto".to_string()),
            extra: None,
        }),
        settings: ProtocolConfig::Vless(VlessConfig {
            uuid: "00000000-0000-0000-0000-000000000001".to_string(),
            flow: Some("xtls-rprx-vision".to_string()),
            encryption: "none".to_string(),
            packet_encoding: None,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    };

    let mut settings = AppSettings::default();
    settings.mixed_port = 37890;

    let routing = RoutingSettings::default();
    let dns = DnsSettings::default();

    let cfg_str = generate_core_config(
        CoreType::Xray,
        &[node_xhttp.clone()],
        &[],
        &settings,
        &dns,
        &routing,
        Some("node-xhttp"),
    )
    .expect("generate xray config");

    let cfg_path = temp_dir.join("xray_gen_test.json");
    std::fs::write(&cfg_path, cfg_str.as_bytes()).unwrap();

    check_core_config(CoreType::Xray, &bin, &cfg_path, &temp_dir)
        .await
        .expect("xray configuration check must pass!");
}

#[tokio::test]
async fn test_multi_core_relay_cooperative_generation() {
    let sb_bin = locate_core(CoreType::SingBox);
    let xray_bin = locate_core(CoreType::Xray);
    if !sb_bin.exists() || !xray_bin.exists() {
        return;
    }

    let temp_dir = std::env::temp_dir().join("novaproxy_test_relay_check");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let upstream_xhttp = ProxyNode {
        id: "xray-upstream-1".to_string(),
        name: "Upstream Xray XHTTP".to_string(),
        server: "example.invalid".to_string(),
        port: 443,
        protocol: ProtocolType::Vless,
        tls: Some(TlsConfig {
            enabled: true,
            server_name: Some("example.invalid".to_string()),
            insecure: false,
            alpn: vec![],
            fingerprint: Some("chrome".to_string()),
            reality: Some(RealityConfig {
                public_key: "dGVzdC1wdWJsaWMta2V5LTAwMDAwMDAwMDAwMDAwMDA".to_string(),
                short_id: "01234567".to_string(),
                spider_x: None,
            }),
            ech_config: None,
            fragment: None,
        }),
        transport: Some(TransportConfig::Xhttp {
            host: Some("example.invalid".to_string()),
            path: "/xhttp".to_string(),
            mode: Some("auto".to_string()),
            extra: None,
        }),
        settings: ProtocolConfig::Vless(VlessConfig {
            uuid: "00000000-0000-0000-0000-000000000001".to_string(),
            flow: Some("xtls-rprx-vision".to_string()),
            encryption: "none".to_string(),
            packet_encoding: None,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: Some(CoreType::Xray),
        sort_order: 0,
    };

    let settings = AppSettings::default();
    let dns = DnsSettings::default();
    let routing = RoutingSettings::default();

    let relay_bundle = generate_multi_core_relay(
        CoreType::SingBox,
        CoreType::Xray,
        &upstream_xhttp,
        &[upstream_xhttp.clone()],
        &[],
        &settings,
        &dns,
        &routing,
        18889,
    )
    .expect("generate multi-core relay");

    assert_eq!(relay_bundle.relay_port, 18889);
    assert_eq!(relay_bundle.primary_core, CoreType::SingBox);
    assert_eq!(relay_bundle.upstream_core, CoreType::Xray);

    // Validate upstream Xray config with Xray binary
    let xray_cfg_path = temp_dir.join("upstream_xray.json");
    std::fs::write(&xray_cfg_path, relay_bundle.upstream_config.as_bytes()).unwrap();
    check_core_config(CoreType::Xray, &xray_bin, &xray_cfg_path, &temp_dir)
        .await
        .expect("relay upstream Xray check must pass");

    // Validate primary sing-box config with sing-box binary
    let sb_cfg_path = temp_dir.join("primary_singbox.json");
    std::fs::write(&sb_cfg_path, relay_bundle.primary_config.as_bytes()).unwrap();
    check_core_config(CoreType::SingBox, &sb_bin, &sb_cfg_path, &temp_dir)
        .await
        .expect("relay primary sing-box check must pass");
}
