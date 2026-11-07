//! DNS configuration generators for sing-box, mihomo, and Xray-core

use crate::model::routing::DnsSettings;
use serde_json::{json, Value};

/// Generates the `dns` section for sing-box 1.14 configuration
pub fn generate_singbox_dns(dns: &DnsSettings) -> Value {
    let mut servers = Vec::new();

    // 1. Remote encrypted DNS (detoured through proxy)
    let remote_server = dns
        .remote_dns
        .first()
        .cloned()
        .unwrap_or_else(|| "https://8.8.8.8/dns-query".to_string());
    servers.push(json!({
        "type": "https",
        "tag": "dns-remote",
        "server": extract_server_host(&remote_server),
        "detour": "proxy"
    }));

    // 2. Domestic direct DNS
    let domestic_server = dns
        .domestic_dns
        .first()
        .cloned()
        .unwrap_or_else(|| "https://223.5.5.5/dns-query".to_string());
    servers.push(json!({
        "type": "https",
        "tag": "dns-direct",
        "server": extract_server_host(&domestic_server),
        "detour": "direct"
    }));

    // 3. Fake-IP DNS server if enabled
    if dns.fake_ip {
        servers.push(json!({
            "type": "fakeip",
            "tag": "dns-fakeip",
            "inet4_range": dns.fake_ip_range
        }));
    }

    let mut rules = Vec::new();
    // Inbound dns hijacking & fakeip query matching
    if dns.fake_ip {
        rules.push(json!({
            "query_type": ["A", "AAAA"],
            "server": "dns-fakeip"
        }));
    }

    // Direct CN domains to domestic DNS
    rules.push(json!({
        "rule_set": ["geosite-cn"],
        "server": "dns-direct"
    }));

    json!({
        "servers": servers,
        "rules": rules,
        "final": "dns-remote",
        "strategy": dns.strategy.as_singbox_str()
    })
}

/// Generates the `dns` section for Mihomo (Clash.Meta) YAML
pub fn generate_mihomo_dns_yaml(dns: &DnsSettings) -> String {
    let mut out = String::new();
    out.push_str("dns:\n");
    out.push_str(&format!("  enable: {}\n", dns.enabled));
    out.push_str(&format!("  ipv6: {}\n", !dns.disable_ipv6));
    if dns.fake_ip {
        out.push_str("  enhanced-mode: fake-ip\n");
        out.push_str(&format!("  fake-ip-range: {}\n", dns.fake_ip_range));
    } else {
        out.push_str("  enhanced-mode: redir-host\n");
    }

    out.push_str("  default-nameserver:\n");
    for ns in &dns.fallback_dns {
        out.push_str(&format!("    - {}\n", ns));
    }

    out.push_str("  nameserver:\n");
    for ns in &dns.remote_dns {
        out.push_str(&format!("    - {}\n", ns));
    }

    out.push_str("  nameserver-policy:\n");
    let domestic = dns
        .domestic_dns
        .first()
        .map(|s| s.as_str())
        .unwrap_or("https://223.5.5.5/dns-query");
    out.push_str(&format!("    '+.cn': {}\n", domestic));
    out.push_str(&format!("    'rule-set:cn-domain': {}\n", domestic));

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
    if let Some(stripped) = uri_or_host.strip_prefix("https://") {
        if let Some(host) = stripped.split('/').next() {
            return host.to_string();
        }
    }
    uri_or_host.to_string()
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
