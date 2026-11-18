//! DNS configuration generators for sing-box, mihomo, and Xray-core

use crate::model::routing::{DnsSettings, RoutingSettings};
use crate::model::settings::RoutingMode;
use serde_json::{json, Value};

/// Generates the `dns` section for sing-box 1.14 configuration using default Rule routing assumptions.
pub fn generate_singbox_dns(dns: &DnsSettings) -> Value {
    generate_singbox_dns_for_routing(dns, &RoutingSettings::default())
}

/// Generates the `dns` section for sing-box 1.14 aligned with active routing settings.
pub fn generate_singbox_dns_for_routing(dns: &DnsSettings, routing: &RoutingSettings) -> Value {
    let mut servers = Vec::new();
    let is_direct_mode = routing.mode == RoutingMode::Direct;
    let has_cn_ruleset = routing.mode == RoutingMode::Rule && routing.cn_direct;

    // 1. Domestic direct DNS (also resolves non-IP remote DoH domains without circular dependency)
    let domestic_server = dns
        .domestic_dns
        .first()
        .cloned()
        .unwrap_or_else(|| "https://223.5.5.5/dns-query".to_string());
    let mut direct_dns_obj = compile_singbox_dns_server("dns-direct", &domestic_server, None);
    if let Some(srv) = direct_dns_obj.get("server").and_then(Value::as_str) {
        if srv.parse::<std::net::IpAddr>().is_err() {
            direct_dns_obj["domain_resolver"] = json!("dns-bootstrap");
            let bootstrap_ip = dns
                .fallback_dns
                .first()
                .map(|s| extract_server_host(s))
                .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
                .unwrap_or_else(|| "223.5.5.5".to_string());
            servers.push(json!({
                "type": "udp",
                "tag": "dns-bootstrap",
                "server": bootstrap_ip
            }));
        }
    }
    servers.push(direct_dns_obj);

    // 2. Remote encrypted DNS (detoured through proxy unless Direct mode)
    let remote_server = dns
        .remote_dns
        .first()
        .cloned()
        .unwrap_or_else(|| "https://8.8.8.8/dns-query".to_string());
    let mut remote_dns_obj = compile_singbox_dns_server(
        "dns-remote",
        &remote_server,
        if is_direct_mode { None } else { Some("proxy") },
    );
    if let Some(srv) = remote_dns_obj.get("server").and_then(Value::as_str) {
        if srv.parse::<std::net::IpAddr>().is_err() {
            remote_dns_obj["domain_resolver"] = json!("dns-direct");
        }
    }
    servers.push(remote_dns_obj);

    // 3. Fake-IP DNS server if enabled (disabled in Direct mode)
    let use_fake_ip = dns.fake_ip && !is_direct_mode;
    if use_fake_ip {
        servers.push(json!({
            "type": "fakeip",
            "tag": "dns-fakeip",
            "inet4_range": dns.fake_ip_range
        }));
    }

    let mut rules = Vec::new();
    // Direct CN domains to domestic DNS BEFORE fake-ip rule so domestic domains get real direct IPs
    if has_cn_ruleset {
        rules.push(json!({
            "rule_set": ["geosite-cn"],
            "server": "dns-direct"
        }));
    }

    // Inbound dns hijacking & fakeip query matching
    if use_fake_ip {
        rules.push(json!({
            "query_type": ["A", "AAAA"],
            "server": "dns-fakeip"
        }));
    }

    let final_dns = if is_direct_mode {
        "dns-direct"
    } else {
        "dns-remote"
    };

    json!({
        "servers": servers,
        "rules": rules,
        "final": final_dns,
        "strategy": dns.strategy.as_singbox_str()
    })
}

fn compile_singbox_dns_server(tag: &str, raw: &str, detour: Option<&str>) -> Value {
    let trimmed = raw.trim();
    let host = extract_server_host(trimmed);
    let dns_type = if trimmed.starts_with("https://") {
        "https"
    } else if trimmed.starts_with("h3://") {
        "h3"
    } else if trimmed.starts_with("tls://") {
        "tls"
    } else if trimmed.starts_with("quic://") {
        "quic"
    } else {
        "udp"
    };
    let mut obj = json!({
        "type": dns_type,
        "tag": tag,
        "server": host
    });
    // sing-box 1.12+ forbids `"detour": "direct"` on DNS servers (empty direct outbound)
    if let Some(d) = detour.filter(|d| !d.is_empty() && *d != "direct") {
        obj["detour"] = json!(d);
    }
    obj
}

/// Generates the `dns` section for Mihomo (Clash.Meta) YAML using default Rule routing assumptions.
pub fn generate_mihomo_dns_yaml(dns: &DnsSettings) -> String {
    generate_mihomo_dns_yaml_for_routing(dns, &RoutingSettings::default())
}

/// Generates the `dns` section for Mihomo (Clash.Meta) YAML aligned with active routing settings.
pub fn generate_mihomo_dns_yaml_for_routing(
    dns: &DnsSettings,
    routing: &RoutingSettings,
) -> String {
    let mut out = String::new();
    out.push_str("dns:\n");
    out.push_str(&format!("  enable: {}\n", dns.enabled));
    out.push_str(&format!("  ipv6: {}\n", !dns.disable_ipv6));
    if dns.fake_ip && routing.mode != RoutingMode::Direct {
        out.push_str("  enhanced-mode: fake-ip\n");
        out.push_str(&format!("  fake-ip-range: {}\n", dns.fake_ip_range));
        out.push_str("  fake-ip-filter:\n");
        out.push_str("    - '*.lan'\n");
        out.push_str("    - '*.local'\n");
        out.push_str("    - 'localhost.ptlogin2.qq.com'\n");
        out.push_str("    - 'time.*.com'\n");
        out.push_str("    - 'ntp.*.com'\n");
        out.push_str("    - 'stun.*.*'\n");
    } else {
        out.push_str("  enhanced-mode: redir-host\n");
    }

    out.push_str("  default-nameserver:\n");
    for ns in &dns.fallback_dns {
        out.push_str(&format!("    - {}\n", ns));
    }

    out.push_str("  nameserver:\n");
    if routing.mode == RoutingMode::Direct {
        for ns in &dns.domestic_dns {
            out.push_str(&format!("    - {}\n", ns));
        }
    } else {
        for ns in &dns.remote_dns {
            out.push_str(&format!("    - {}\n", ns));
        }
    }

    out.push_str("  nameserver-policy:\n");
    let domestic = dns
        .domestic_dns
        .first()
        .map(|s| s.as_str())
        .unwrap_or("https://223.5.5.5/dns-query");
    out.push_str(&format!("    '+.cn': {}\n", domestic));
    if routing.mode == RoutingMode::Rule && routing.cn_direct {
        out.push_str(&format!("    'rule-set:cn-domain': {}\n", domestic));
    }

    out
}

/// Generates the `dns` section for Xray-core configuration
pub fn generate_xray_dns(dns: &DnsSettings) -> Value {
    let mut servers: Vec<Value> = Vec::new();

    for remote in &dns.remote_dns {
        servers.push(Value::String(remote.clone()));
    }

    let domestic = dns
        .domestic_dns
        .first()
        .cloned()
        .unwrap_or_else(|| "223.5.5.5".to_string());

    servers.push(json!({
        "address": domestic,
        "domains": ["geosite:cn", "domain:cn"]
    }));

    json!({
        "servers": servers
    })
}

fn extract_server_host(uri_or_host: &str) -> String {
    let trimmed = uri_or_host.trim();
    for scheme in [
        "https://", "http://", "tls://", "quic://", "udp://", "h3://",
    ] {
        if let Some(stripped) = trimmed.strip_prefix(scheme) {
            if let Some(authority) = stripped.split('/').next() {
                if let Some(host_only) = authority.split(':').next() {
                    if !authority.starts_with('[') {
                        return host_only.to_string();
                    }
                }
                return authority.to_string();
            }
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_generators() {
        let dns = DnsSettings::default();

        let sb_dns = generate_singbox_dns(&dns);
        assert!(sb_dns.get("servers").is_some());
        assert_eq!(sb_dns["final"], "dns-remote");

        let mihomo_dns = generate_mihomo_dns_yaml(&dns);
        assert!(mihomo_dns.contains("enhanced-mode: fake-ip"));
        assert!(mihomo_dns.contains("nameserver:"));

        let xray_dns = generate_xray_dns(&dns);
        assert!(xray_dns.get("servers").is_some());
    }
}
