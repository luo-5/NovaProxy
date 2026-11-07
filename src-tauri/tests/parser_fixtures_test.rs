use novaproxy_lib::model::{
    ProtocolConfig, ProtocolType, ProxyNode, ShadowsocksRConfig, TlsConfig, TransportConfig,
    VmessConfig,
};
use novaproxy_lib::parser::base64::encode_base64_standard;
use novaproxy_lib::parser::qrcode::{
    decode_qr_from_image_bytes, encode_node_qr_svg, encode_qr_png_bytes,
};
use novaproxy_lib::parser::{
    node_to_share_link, parse_share_link, parse_subscription_content, parse_subscription_userinfo,
    SubscriptionFormat,
};
use novaproxy_lib::storage::Database;
use std::collections::BTreeMap;

const SHARE_LINKS_FIXTURE: &str = include_str!("fixtures/share_links.txt");
const CLASH_YAML_FIXTURE: &str = include_str!("fixtures/clash_subscription.yaml");
const SINGBOX_JSON_FIXTURE: &str = include_str!("fixtures/singbox_subscription.json");
const SIP008_JSON_FIXTURE: &str = include_str!("fixtures/sip008_subscription.json");
const WIREGUARD_CONF_FIXTURE: &str = include_str!("fixtures/wireguard_sample.conf");

fn assert_node_roundtrip(node: &ProxyNode) {
    let uri = node_to_share_link(node).expect("serialize node to URI");
    let reparsed = parse_share_link(&uri)
        .unwrap_or_else(|e| panic!("failed to reparse generated URI '{uri}': {e}"));

    assert_eq!(
        reparsed.protocol, node.protocol,
        "protocol mismatch for {uri}"
    );
    assert_eq!(reparsed.server, node.server, "server mismatch for {uri}");
    assert_eq!(reparsed.port, node.port, "port mismatch for {uri}");
    assert_eq!(reparsed.name, node.name, "name mismatch for {uri}");
    assert_eq!(
        reparsed.transport_kind(),
        node.transport_kind(),
        "transport kind mismatch for {uri}"
    );
    assert_eq!(
        reparsed.compute_fingerprint(),
        node.compute_fingerprint(),
        "fingerprint mismatch for {uri}"
    );
}

#[test]
fn test_all_share_links_fixture_roundtrip() {
    let report = parse_subscription_content(SHARE_LINKS_FIXTURE).expect("parse share_links.txt");
    assert_eq!(report.format, SubscriptionFormat::PlainLinks);
    assert!(
        report.warnings.is_empty(),
        "unexpected warnings in share_links.txt: {:?}",
        report.warnings
    );
    // 6 VLESS + 5 Trojan + 5 SS + 5 Hy2 + 5 Hy1 + 5 TUIC + 5 WG = 36 links
    assert_eq!(report.nodes.len(), 36);

    for node in &report.nodes {
        assert_node_roundtrip(node);
    }
}

#[test]
fn test_vmess_five_samples_and_roundtrip() {
    let samples = vec![
        // 1. Standard TCP + TLS + Chinese/Emoji
        ProxyNode {
            id: "vm-1".into(),
            name: "🇭🇰 香港 VMess-TCP-TLS 01".into(),
            server: "vm1.example.com".into(),
            port: 443,
            protocol: ProtocolType::Vmess,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("vm1.example.com".into()),
                insecure: false,
                alpn: vec!["h2".into(), "http/1.1".into()],
                fingerprint: Some("chrome".into()),
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: None,
            settings: ProtocolConfig::Vmess(VmessConfig {
                uuid: "11111111-2222-3333-4444-555555550001".into(),
                alter_id: 0,
                security: "auto".into(),
                global_padding: None,
                authenticated_length: None,
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 0,
        },
        // 2. IPv6 + WebSocket + TLS
        ProxyNode {
            id: "vm-2".into(),
            name: "🇯🇵 东京 VMess-IPv6-WS 02".into(),
            server: "2001:db8::8802".into(),
            port: 8443,
            protocol: ProtocolType::Vmess,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("ws.example.com".into()),
                insecure: true,
                alpn: Vec::new(),
                fingerprint: Some("firefox".into()),
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: Some(TransportConfig::Ws {
                path: "/vmess-ws?ed=2048".into(),
                host: Some("ws.example.com".into()),
                headers: {
                    let mut h = BTreeMap::new();
                    h.insert("Host".into(), "ws.example.com".into());
                    h
                },
                max_early_data: None,
                early_data_header_name: None,
            }),
            settings: ProtocolConfig::Vmess(VmessConfig {
                uuid: "11111111-2222-3333-4444-555555550002".into(),
                alter_id: 0,
                security: "aes-128-gcm".into(),
                global_padding: None,
                authenticated_length: None,
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 1,
        },
        // 3. gRPC multi-mode + TLS
        ProxyNode {
            id: "vm-3".into(),
            name: "🇺🇸 纽约 VMess-gRPC 03".into(),
            server: "vm3.example.com".into(),
            port: 443,
            protocol: ProtocolType::Vmess,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("vm3.example.com".into()),
                insecure: false,
                alpn: vec!["h2".into()],
                fingerprint: None,
                reality: None,
                ech_config: None,
                fragment: None,
            }),
            transport: Some(TransportConfig::Grpc {
                service_name: "vmess-grpc-service".into(),
                multi_mode: true,
                authority: Some("auth.example.com".into()),
            }),
            settings: ProtocolConfig::Vmess(VmessConfig {
                uuid: "11111111-2222-3333-4444-555555550003".into(),
                alter_id: 0,
                security: "chacha20-poly1305".into(),
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
        // 4. mKCP + seed + no TLS
        ProxyNode {
            id: "vm-4".into(),
            name: "VMess-mKCP-04".into(),
            server: "vm4.example.com".into(),
            port: 29999,
            protocol: ProtocolType::Vmess,
            tls: None,
            transport: Some(TransportConfig::Mkcp {
                seed: Some("kcp-seed-secret".into()),
                header_type: Some("utp".into()),
                mtu: None,
                tti: None,
                uplink_capacity: None,
                downlink_capacity: None,
                congestion: false,
            }),
            settings: ProtocolConfig::Vmess(VmessConfig {
                uuid: "11111111-2222-3333-4444-555555550004".into(),
                alter_id: 16,
                security: "auto".into(),
                global_padding: None,
                authenticated_length: None,
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 3,
        },
        // 5. HTTPUpgrade + minimal fields
        ProxyNode {
            id: "vm-5".into(),
            name: "vm5.example.com:8080".into(),
            server: "vm5.example.com".into(),
            port: 8080,
            protocol: ProtocolType::Vmess,
            tls: None,
            transport: Some(TransportConfig::HttpUpgrade {
                host: Some("cdn.example.com".into()),
                path: "/up".into(),
                headers: BTreeMap::new(),
            }),
            settings: ProtocolConfig::Vmess(VmessConfig {
                uuid: "11111111-2222-3333-4444-555555550005".into(),
                alter_id: 0,
                security: "auto".into(),
                global_padding: None,
                authenticated_length: None,
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 4,
        },
    ];

    for node in &samples {
        assert_node_roundtrip(node);
    }

    // Also test URL-style vmess://uuid@host:port?type=ws#name
    let url_vmess = "vmess://11111111-2222-3333-4444-555555550099@vm-url.example.com:443?type=ws&security=tls&sni=vm-url.example.com&path=%2Fws#URL-Style-VMess";
    let parsed_url_vmess = parse_share_link(url_vmess).expect("parse URL-style vmess");
    assert_eq!(parsed_url_vmess.protocol, ProtocolType::Vmess);
    assert_eq!(parsed_url_vmess.name, "URL-Style-VMess");
    assert_node_roundtrip(&parsed_url_vmess);
}

#[test]
fn test_ssr_five_samples_and_roundtrip() {
    let cases = [
        (
            "ssr1.example.com",
            8388,
            "origin",
            "aes-256-cfb",
            "plain",
            "pass-01",
            None,
            None,
            "🇭🇰 香港 SSR 01",
        ),
        (
            "2001:db8::9002",
            8389,
            "auth_aes128_md5",
            "chacha20-ietf",
            "tls1.2_ticket_auth",
            "pass:02!@#",
            Some("cdn.example.com"),
            Some("1001:key"),
            "🇯🇵 东京 SSR IPv6 02",
        ),
        (
            "ssr3.example.com",
            9003,
            "auth_sha1_v4",
            "rc4-md5",
            "http_simple",
            "pass-03",
            Some("download.example.com"),
            None,
            "🇸🇬 新加坡 SSR 03",
        ),
        (
            "ssr4.example.com",
            9004,
            "auth_chain_a",
            "none",
            "plain",
            "pass-04",
            None,
            Some("proto-param-04"),
            "🇺🇸 美国 SSR 04",
        ),
        (
            "ssr5.example.com",
            9005,
            "origin",
            "aes-128-ctr",
            "tls1.2_ticket_auth",
            "pass-05",
            None,
            None,
            "ssr5.example.com:9005",
        ),
    ];

    for (idx, (server, port, proto, method, obfs, pass, obfs_p, proto_p, name)) in
        cases.iter().enumerate()
    {
        let node = ProxyNode {
            id: format!("ssr-{idx}"),
            name: (*name).to_string(),
            server: (*server).to_string(),
            port: *port,
            protocol: ProtocolType::ShadowsocksR,
            tls: None,
            transport: None,
            settings: ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
                method: (*method).to_string(),
                password: (*pass).to_string(),
                protocol: (*proto).to_string(),
                protocol_param: proto_p.map(ToString::to_string),
                obfs: (*obfs).to_string(),
                obfs_param: obfs_p.map(ToString::to_string),
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: idx as i32,
        };
        assert_node_roundtrip(&node);
    }
}

#[test]
fn test_extra_protocols_roundtrip() {
    let extra_links = [
        "anytls://any-pass-01@any1.example.com:443?sni=any1.example.com&fp=chrome#AnyTLS-01",
        "shadowtls://stls-pass-01@stls1.example.com:443?version=3&sni=stls1.example.com#ShadowTLS-01",
        "naive+https://naiveuser:naivepass@naive1.example.com:443?sni=naive1.example.com#Naive-HTTPS-01",
        "naive+quic://naiveuser:naivepass@[2001:db8::99]:443?sni=naive2.example.com#Naive-QUIC-IPv6-02",
        "snell://snell-psk-01@snell1.example.com:9443?version=4&obfs=tls&obfs-host=snell1.example.com#Snell-01",
        "mieru://muser:mpass@mieru1.example.com:10443?transport=TCP&multiplexing=MULTIPLEXING_HIGH#Mieru-01",
        "ssh://sshuser:sshpass@ssh1.example.com:22#SSH-01",
        "socks5://suser:spass@socks1.example.com:1080#SOCKS5-01",
        "https://huser:hpass@http1.example.com:8443#HTTPS-Proxy-01",
    ];

    for link in extra_links {
        let node = parse_share_link(link).unwrap_or_else(|e| panic!("failed to parse {link}: {e}"));
        assert_node_roundtrip(&node);
    }
}

#[test]
fn test_clash_yaml_and_base64_and_json_subscriptions() {
    // 1. Clash YAML fixture (16 valid nodes + 1 broken node skipped with warning)
    let clash_report = parse_subscription_content(CLASH_YAML_FIXTURE).expect("parse clash yaml");
    assert_eq!(clash_report.format, SubscriptionFormat::ClashYaml);
    assert_eq!(clash_report.nodes.len(), 16);
    assert_eq!(clash_report.warnings.len(), 1);
    assert!(clash_report.warnings[0].contains("Broken Node Should Be Skipped"));

    // 2. Base64-encoded subscription (with UTF-8 BOM and CRLF)
    let b64_payload = format!(
        "\u{feff}{}\r\n",
        encode_base64_standard(SHARE_LINKS_FIXTURE.as_bytes())
    );
    let b64_report = parse_subscription_content(&b64_payload).expect("parse base64 subscription");
    assert_eq!(b64_report.format, SubscriptionFormat::Base64Links);
    assert_eq!(b64_report.nodes.len(), 36);

    // 3. sing-box JSON subscription
    let sb_report = parse_subscription_content(SINGBOX_JSON_FIXTURE).expect("parse sing-box json");
    assert_eq!(sb_report.format, SubscriptionFormat::SingBoxOrSip008Json);
    assert_eq!(sb_report.nodes.len(), 4);

    // 4. SIP008 JSON subscription
    let sip_report = parse_subscription_content(SIP008_JSON_FIXTURE).expect("parse SIP008 json");
    assert_eq!(sip_report.format, SubscriptionFormat::SingBoxOrSip008Json);
    assert_eq!(sip_report.nodes.len(), 2);

    // 5. WireGuard .conf INI subscription
    let wg_report = parse_subscription_content(WIREGUARD_CONF_FIXTURE).expect("parse wg conf");
    assert_eq!(wg_report.format, SubscriptionFormat::WireGuardConf);
    assert_eq!(wg_report.nodes.len(), 1);
    assert_eq!(wg_report.nodes[0].name, "🇨🇦 WireGuard 配置文件示例 01");
    assert_node_roundtrip(&wg_report.nodes[0]);
}

#[test]
fn test_unsupported_scheme_and_malformed_inputs_never_panic() {
    assert!(parse_share_link("").is_err());
    assert!(parse_share_link("   ").is_err());
    assert!(parse_share_link("ftp://example.com:21").is_err());
    assert!(parse_share_link("unknown-proto://abc@example.com:443").is_err());
    assert!(parse_share_link("vless://@example.com:443").is_err());
    assert!(parse_share_link("vmess://not-valid-base64-or-url").is_err());
    assert!(parse_subscription_content("not a valid subscription").is_err());
}

#[test]
fn test_subscription_userinfo_and_incremental_sync_preserves_custom_name_and_order() {
    let header = "upload=1073741824; download=5368709120; total=107374182400; expire=1893456000";
    let info = parse_subscription_userinfo(header);
    assert_eq!(info.upload, Some(1_073_741_824));
    assert_eq!(info.download, Some(5_368_709_120));
    assert_eq!(info.total, Some(107_374_182_400));
    assert_eq!(info.expire, Some(1_893_456_000));

    let db = Database::open_in_memory().expect("open in-memory db");
    let sub = novaproxy_lib::model::Subscription {
        id: "sub-sync-1".into(),
        name: "My Sub".into(),
        url: "https://example.com/sub".into(),
        enabled: true,
        user_agent: None,
        update_interval_mins: Some(60),
        use_proxy: false,
        updated_at: None,
        upload: None,
        download: None,
        total: None,
        expire: None,
        home_url: None,
        sort_order: 0,
    };
    db.upsert_subscription(&sub).unwrap();

    let n1 = parse_share_link("trojan://pass1@n1.example.com:443#Original-Name-1").unwrap();
    let n2 = parse_share_link("trojan://pass2@n2.example.com:443#Original-Name-2").unwrap();
    let initial = db
        .sync_subscription_nodes("sub-sync-1", vec![n1, n2])
        .unwrap();
    assert_eq!(initial.len(), 2);

    // Simulate user customizing node 1's name, sort_order, and recording latency
    let mut customized_n1 = initial[0].clone();
    customized_n1.name = "⭐ 用户自定义置顶名称".to_string();
    customized_n1.sort_order = 10;
    db.upsert_node(&customized_n1).unwrap();
    db.record_latency(&customized_n1.id, Some(28), "tcp", None)
        .unwrap();

    // Now subscription refreshes: n1 is still present (with old server remark), n2 is removed, n3 is added
    let refresh_n1 =
        parse_share_link("trojan://pass1@n1.example.com:443#Server-Renamed-1").unwrap();
    let refresh_n3 = parse_share_link("trojan://pass3@n3.example.com:443#New-Node-3").unwrap();

    let synced = db
        .sync_subscription_nodes("sub-sync-1", vec![refresh_n1, refresh_n3])
        .unwrap();
    assert_eq!(synced.len(), 2);

    let kept_n1 = synced
        .iter()
        .find(|n| n.server == "n1.example.com")
        .expect("n1 should be preserved");
    assert_eq!(kept_n1.id, customized_n1.id);
    assert_eq!(kept_n1.name, "⭐ 用户自定义置顶名称");
    assert_eq!(kept_n1.sort_order, 10);
    assert_eq!(kept_n1.latency, Some(28));
}

#[test]
fn test_qrcode_encode_and_decode_roundtrip() {
    let uri = "vless://11111111-1111-1111-1111-111111111101@vless1.example.com:443?encryption=none&type=tcp&security=reality&sni=www.example.com&pbk=DummyRealityPubKey01#QR-Test-Node";
    let node = parse_share_link(uri).unwrap();

    let svg = encode_node_qr_svg(&node).expect("encode SVG QR");
    assert!(svg.contains("<svg"));

    let png_bytes = encode_qr_png_bytes(uri).expect("encode PNG QR");
    let decoded_list = decode_qr_from_image_bytes(&png_bytes).expect("decode PNG QR");
    assert_eq!(decoded_list, vec![uri.to_string()]);
}

#[tokio::test]
async fn test_fetch_and_parse_subscription_http_headers_and_retry() {
    use novaproxy_lib::model::Subscription;
    use novaproxy_lib::parser::{apply_userinfo_to_subscription, fetch_and_parse_subscription};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().unwrap();
    let hit_count = Arc::new(AtomicUsize::new(0));
    let hit_count_srv = Arc::clone(&hit_count);

    let server_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let count = hit_count_srv.fetch_add(1, Ordering::SeqCst) + 1;
            let mut req_buf = vec![0u8; 2048];
            let n = socket.read(&mut req_buf).await.unwrap_or(0);
            let req_str = String::from_utf8_lossy(&req_buf[..n]);

            // Verify custom User-Agent is sent
            assert!(
                req_str.contains("User-Agent: clash.meta/custom-test")
                    || req_str.contains("user-agent: clash.meta/custom-test"),
                "Expected custom User-Agent in request: {req_str}"
            );

            if count == 1 {
                // First attempt returns 502 to test retry logic
                let resp =
                    "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(resp.as_bytes()).await;
            } else {
                let body = encode_base64_standard(SHARE_LINKS_FIXTURE.as_bytes());
                let resp = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Type: text/plain; charset=utf-8\r\n\
                     Subscription-Userinfo: upload=1024; download=2048; total=10485760; expire=1900000000\r\n\
                     Profile-Update-Interval: 12\r\n\
                     Profile-Web-Page-Url: https://panel.example.com\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                break;
            }
        }
    });

    let mut sub = Subscription {
        id: "sub-http-1".into(),
        name: "Mock Remote Sub".into(),
        url: format!("http://127.0.0.1:{}/sub", addr.port()),
        enabled: true,
        user_agent: Some("clash.meta/custom-test".into()),
        update_interval_mins: None,
        use_proxy: false,
        updated_at: None,
        upload: None,
        download: None,
        total: None,
        expire: None,
        home_url: None,
        sort_order: 0,
    };

    let (report, user_info) = fetch_and_parse_subscription(&sub, None, 5, 3)
        .await
        .expect("fetch_and_parse_subscription should succeed on retry");

    assert_eq!(hit_count.load(Ordering::SeqCst), 2);
    assert_eq!(report.format, SubscriptionFormat::Base64Links);
    assert_eq!(report.nodes.len(), 36);

    apply_userinfo_to_subscription(&mut sub, &user_info);
    assert_eq!(sub.upload, Some(1024));
    assert_eq!(sub.download, Some(2048));
    assert_eq!(sub.total, Some(10_485_760));
    assert_eq!(sub.expire, Some(1_900_000_000));
    assert_eq!(sub.update_interval_mins, Some(720));
    assert_eq!(sub.home_url.as_deref(), Some("https://panel.example.com"));
    assert!(sub.updated_at.is_some());

    let _ = server_task.await;
}
