//! Routing rules configuration generators for sing-box, mihomo, and Xray-core

use crate::model::routing::{RoutingSettings, RuleAction, RuleItem, RuleType};
use crate::model::settings::RoutingMode;
use serde_json::{json, Value};
use std::collections::BTreeSet;

const TUN_BYPASS_PROCESSES: &[&str] = &[
    "NovaProxy.exe",
    "novaproxy.exe",
    "sing-box.exe",
    "mihomo.exe",
    "xray.exe",
];

fn normalize_geosite_tag(pattern: &str) -> String {
    let cleaned = pattern
        .trim()
        .trim_start_matches("geosite:")
        .trim_start_matches("geosite-")
        .to_ascii_lowercase();
    format!("geosite-{cleaned}")
}

fn normalize_geoip_tag(pattern: &str) -> String {
    let cleaned = pattern
        .trim()
        .trim_start_matches("geoip:")
        .trim_start_matches("geoip-")
        .to_ascii_lowercase();
    format!("geoip-{cleaned}")
}

/// Generates sing-box 1.14 route configuration block without TUN process exclusions.
pub fn generate_singbox_route(routing: &RoutingSettings, proxy_tag: &str) -> Value {
    generate_singbox_route_with_options(routing, proxy_tag, false)
}

/// Generates sing-box 1.14 route configuration block with optional TUN process exclusions.
pub fn generate_singbox_route_with_options(
    routing: &RoutingSettings,
    proxy_tag: &str,
    tun_mode: bool,
) -> Value {
    let mut rules = Vec::new();

    // 1. Sniff and hijack DNS
    rules.push(json!({ "action": "sniff" }));
    rules.push(json!({ "protocol": "dns", "action": "hijack-dns" }));

    if tun_mode && routing.mode != RoutingMode::Direct {
        rules.push(json!({
            "process_name": TUN_BYPASS_PROCESSES,
            "outbound": "direct"
        }));
    }

    let mut rule_sets = Vec::new();
    let mut registered_tags = BTreeSet::new();
    let mut extra_geosite_tags = BTreeSet::new();
    let mut extra_geoip_tags = BTreeSet::new();

    match routing.mode {
        RoutingMode::Direct => {
            rules.push(json!({ "outbound": "direct" }));
        }
        RoutingMode::Global => {
            rules.push(json!({ "outbound": proxy_tag }));
        }
        RoutingMode::Rule => {
            // Private network direct
            if routing.private_direct {
                rules.push(json!({
                    "ip_is_private": true,
                    "outbound": "direct"
                }));
            }

            // Custom user-defined rules
            for item in &routing.custom_rules {
                if !item.enabled || item.pattern.trim().is_empty() {
                    continue;
                }
                let clean_pat = item.pattern.trim();
                match item.rule_type {
                    RuleType::GeoSite => {
                        extra_geosite_tags.insert(normalize_geosite_tag(clean_pat));
                    }
                    RuleType::GeoIp => {
                        extra_geoip_tags.insert(normalize_geoip_tag(clean_pat));
                    }
                    _ => {}
                }
                if let Some(rule_val) = compile_singbox_rule_item(item, proxy_tag) {
                    rules.push(rule_val);
                }
            }

            // Preset: Ads block
            if routing.ads_block {
                rules.push(json!({
                    "rule_set": ["geosite-ads"],
                    "action": "reject"
                }));
                registered_tags.insert("geosite-ads".to_string());
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": "geosite-ads",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs",
                    "download_detour": proxy_tag,
                    "update_interval": "24h"
                }));
            }

            // Preset: CN Direct (Domains + IPs)
            if routing.cn_direct {
                rules.push(json!({
                    "rule_set": ["geosite-cn", "geoip-cn"],
                    "outbound": "direct"
                }));
                registered_tags.insert("geosite-cn".to_string());
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": "geosite-cn",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-cn.srs",
                    "download_detour": proxy_tag,
                    "update_interval": "24h"
                }));
                registered_tags.insert("geoip-cn".to_string());
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": "geoip-cn",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-cn.srs",
                    "download_detour": proxy_tag,
                    "update_interval": "24h"
                }));
            }

            for tag in extra_geosite_tags {
                if !registered_tags.insert(tag.clone()) {
                    continue;
                }
                let url = if tag == "geosite-ads" {
                    "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs".to_string()
                } else {
                    format!("https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/{tag}.srs")
                };
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": tag,
                    "format": "binary",
                    "url": url,
                    "download_detour": proxy_tag,
                    "update_interval": "24h"
                }));
            }
            for tag in extra_geoip_tags {
                if !registered_tags.insert(tag.clone()) {
                    continue;
                }
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": tag,
                    "format": "binary",
                    "url": format!("https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/{tag}.srs"),
                    "download_detour": proxy_tag,
                    "update_interval": "24h"
                }));
            }
        }
    }

    let final_outbound = match routing.mode {
        RoutingMode::Direct => "direct",
        RoutingMode::Global | RoutingMode::Rule => proxy_tag,
    };

    let mut route_obj = json!({
        "default_domain_resolver": "dns-direct",
        "rules": rules,
        "final": final_outbound,
        "auto_detect_interface": true
    });

    if !rule_sets.is_empty() {
        route_obj["rule_set"] = Value::Array(rule_sets);
    }

    route_obj
}

fn compile_singbox_rule_item(item: &RuleItem, default_proxy_tag: &str) -> Option<Value> {
    let clean_pat = item.pattern.trim();
    if clean_pat.is_empty() {
        return None;
    }

    let mut map = serde_json::Map::new();

    let target = item.action.target_tag(default_proxy_tag);
    if item.action == RuleAction::Block {
        map.insert("action".to_string(), json!("reject"));
    } else {
        map.insert("outbound".to_string(), json!(target));
    }

    match item.rule_type {
        RuleType::Domain => {
            map.insert("domain".to_string(), json!([clean_pat]));
        }
        RuleType::DomainSuffix => {
            let pat = clean_pat.trim_start_matches('.');
            map.insert("domain_suffix".to_string(), json!([pat]));
        }
        RuleType::DomainKeyword => {
            map.insert("domain_keyword".to_string(), json!([clean_pat]));
        }
        RuleType::DomainRegex => {
            map.insert("domain_regex".to_string(), json!([clean_pat]));
        }
        RuleType::IpCidr => {
            map.insert("ip_cidr".to_string(), json!([clean_pat]));
        }
        RuleType::ProcessName => {
            map.insert("process_name".to_string(), json!([clean_pat]));
        }
        RuleType::Port => {
            let mut single_ports = Vec::new();
            let mut port_ranges = Vec::new();
            for part in clean_pat
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if let Ok(p) = part.parse::<u16>() {
                    single_ports.push(p);
                } else {
                    let normalized = part.replace('-', ":");
                    if normalized.contains(':') {
                        port_ranges.push(normalized);
                    }
                }
            }
            if !single_ports.is_empty() {
                map.insert("port".to_string(), json!(single_ports));
            }
            if !port_ranges.is_empty() {
                map.insert("port_range".to_string(), json!(port_ranges));
            }
            if single_ports.is_empty() && port_ranges.is_empty() {
                return None;
            }
        }
        RuleType::GeoSite => {
            map.insert(
                "rule_set".to_string(),
                json!([normalize_geosite_tag(clean_pat)]),
            );
        }
        RuleType::GeoIp => {
            map.insert(
                "rule_set".to_string(),
                json!([normalize_geoip_tag(clean_pat)]),
            );
        }
    }

    Some(Value::Object(map))
}

/// Generates Mihomo (Clash.Meta) YAML rule-providers and rules without TUN process exclusions.
pub fn generate_mihomo_rules_yaml(routing: &RoutingSettings, proxy_tag: &str) -> String {
    generate_mihomo_rules_yaml_with_options(routing, proxy_tag, false)
}

/// Generates Mihomo (Clash.Meta) YAML rule-providers and rules with optional TUN process exclusions.
pub fn generate_mihomo_rules_yaml_with_options(
    routing: &RoutingSettings,
    proxy_tag: &str,
    tun_mode: bool,
) -> String {
    let mut out = String::new();

    // 1. rule-providers (when in Rule mode)
    if routing.mode == RoutingMode::Rule && (routing.ads_block || routing.cn_direct) {
        out.push_str("rule-providers:\n");
        if routing.ads_block {
            out.push_str("  ads-domain:\n");
            out.push_str("    type: http\n");
            out.push_str("    behavior: domain\n");
            out.push_str("    format: yaml\n");
            out.push_str("    url: https://raw.githubusercontent.com/Loyalsoldier/clash-rules/release/reject.txt\n");
            out.push_str("    path: ./ruleset/ads-domain.yaml\n");
            out.push_str("    interval: 86400\n");
        }
        if routing.cn_direct {
            out.push_str("  cn-domain:\n");
            out.push_str("    type: http\n");
            out.push_str("    behavior: domain\n");
            out.push_str("    format: yaml\n");
            out.push_str("    url: https://raw.githubusercontent.com/Loyalsoldier/clash-rules/release/direct.txt\n");
            out.push_str("    path: ./ruleset/cn-domain.yaml\n");
            out.push_str("    interval: 86400\n");
            out.push_str("  cn-ip:\n");
            out.push_str("    type: http\n");
            out.push_str("    behavior: ipcidr\n");
            out.push_str("    format: yaml\n");
            out.push_str("    url: https://raw.githubusercontent.com/Loyalsoldier/clash-rules/release/cncidr.txt\n");
            out.push_str("    path: ./ruleset/cn-ip.yaml\n");
            out.push_str("    interval: 86400\n");
        }
    }

    // 2. rules
    out.push_str("rules:\n");
    if tun_mode && routing.mode != RoutingMode::Direct {
        for proc_name in TUN_BYPASS_PROCESSES {
            out.push_str(&format!("  - PROCESS-NAME,{proc_name},DIRECT\n"));
        }
    }
    match routing.mode {
        RoutingMode::Direct => {
            out.push_str("  - MATCH,DIRECT\n");
        }
        RoutingMode::Global => {
            out.push_str(&format!("  - MATCH,{}\n", proxy_tag));
        }
        RoutingMode::Rule => {
            if routing.private_direct {
                out.push_str("  - IP-CIDR,127.0.0.0/8,DIRECT,no-resolve\n");
                out.push_str("  - IP-CIDR,10.0.0.0/8,DIRECT,no-resolve\n");
                out.push_str("  - IP-CIDR,172.16.0.0/12,DIRECT,no-resolve\n");
                out.push_str("  - IP-CIDR,192.168.0.0/16,DIRECT,no-resolve\n");
                out.push_str("  - IP-CIDR,::1/128,DIRECT,no-resolve\n");
                out.push_str("  - IP-CIDR,fc00::/7,DIRECT,no-resolve\n");
                out.push_str("  - IP-CIDR,fe80::/10,DIRECT,no-resolve\n");
            }

            for item in &routing.custom_rules {
                let clean_pat = item.pattern.trim();
                if !item.enabled || clean_pat.is_empty() {
                    continue;
                }
                let target = match item.action {
                    RuleAction::Proxy => proxy_tag,
                    RuleAction::Direct => "DIRECT",
                    RuleAction::Block => "REJECT",
                    RuleAction::Tag(ref t) => t.as_str(),
                };

                let line = match item.rule_type {
                    RuleType::Domain => format!("  - DOMAIN,{},{}\n", clean_pat, target),
                    RuleType::DomainSuffix => format!(
                        "  - DOMAIN-SUFFIX,{},{}\n",
                        clean_pat.trim_start_matches('.'),
                        target
                    ),
                    RuleType::DomainKeyword => {
                        format!("  - DOMAIN-KEYWORD,{},{}\n", clean_pat, target)
                    }
                    RuleType::DomainRegex => {
                        format!("  - DOMAIN-REGEX,{},{}\n", clean_pat, target)
                    }
                    RuleType::IpCidr => {
                        format!("  - IP-CIDR,{},{},no-resolve\n", clean_pat, target)
                    }
                    RuleType::ProcessName => {
                        format!("  - PROCESS-NAME,{},{}\n", clean_pat, target)
                    }
                    RuleType::Port => {
                        let mut lines = String::new();
                        for part in clean_pat
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                        {
                            let normalized = part.replace(':', "-");
                            lines.push_str(&format!("  - DST-PORT,{},{}\n", normalized, target));
                        }
                        lines
                    }
                    RuleType::GeoSite => format!("  - GEOSITE,{},{}\n", clean_pat, target),
                    RuleType::GeoIp => {
                        format!("  - GEOIP,{},{},no-resolve\n", clean_pat, target)
                    }
                };
                out.push_str(&line);
            }

            if routing.ads_block {
                out.push_str("  - RULE-SET,ads-domain,REJECT\n");
            }
            if routing.cn_direct {
                out.push_str("  - RULE-SET,cn-domain,DIRECT\n");
                out.push_str("  - RULE-SET,cn-ip,DIRECT,no-resolve\n");
            }

            out.push_str(&format!("  - MATCH,{}\n", proxy_tag));
        }
    }

    out
}

/// Generates Xray-core routing configuration block
pub fn generate_xray_routing(routing: &RoutingSettings, proxy_tag: &str) -> Value {
    let mut rules = Vec::new();

    match routing.mode {
        RoutingMode::Direct => {
            rules.push(json!({
                "type": "field",
                "network": "tcp,udp",
                "outboundTag": "direct"
            }));
        }
        RoutingMode::Global => {
            rules.push(json!({
                "type": "field",
                "network": "tcp,udp",
                "outboundTag": proxy_tag
            }));
        }
        RoutingMode::Rule => {
            // Private network direct using explicit CIDRs (does not depend on geoip.dat)
            if routing.private_direct {
                rules.push(json!({
                    "type": "field",
                    "ip": [
                        "127.0.0.0/8",
                        "10.0.0.0/8",
                        "172.16.0.0/12",
                        "192.168.0.0/16",
                        "::1/128",
                        "fc00::/7",
                        "fe80::/10"
                    ],
                    "outboundTag": "direct"
                }));
            }

            for item in &routing.custom_rules {
                let clean_pat = item.pattern.trim();
                if !item.enabled || clean_pat.is_empty() {
                    continue;
                }
                let target = match item.action {
                    RuleAction::Proxy => proxy_tag,
                    RuleAction::Direct => "direct",
                    RuleAction::Block => "block",
                    RuleAction::Tag(ref t) => t.as_str(),
                };

                match item.rule_type {
                    RuleType::Domain => {
                        rules.push(json!({
                            "type": "field",
                            "domain": [format!("full:{}", clean_pat)],
                            "outboundTag": target
                        }));
                    }
                    RuleType::DomainSuffix => {
                        rules.push(json!({
                            "type": "field",
                            "domain": [format!("domain:{}", clean_pat.trim_start_matches('.'))],
                            "outboundTag": target
                        }));
                    }
                    RuleType::DomainKeyword => {
                        rules.push(json!({
                            "type": "field",
                            "domain": [format!("keyword:{}", clean_pat)],
                            "outboundTag": target
                        }));
                    }
                    RuleType::DomainRegex => {
                        rules.push(json!({
                            "type": "field",
                            "domain": [format!("regexp:{}", clean_pat)],
                            "outboundTag": target
                        }));
                    }
                    RuleType::IpCidr => {
                        rules.push(json!({
                            "type": "field",
                            "ip": [clean_pat],
                            "outboundTag": target
                        }));
                    }
                    RuleType::Port => {
                        let cleaned: Vec<String> = clean_pat
                            .split(',')
                            .map(|s| s.trim().replace(':', "-"))
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !cleaned.is_empty() {
                            rules.push(json!({
                                "type": "field",
                                "port": cleaned.join(","),
                                "outboundTag": target
                            }));
                        }
                    }
                    _ => {}
                }
            }

            // Default catch-all
            rules.push(json!({
                "type": "field",
                "network": "tcp,udp",
                "outboundTag": proxy_tag
            }));
        }
    }

    json!({
        "domainStrategy": "IPIfNonMatch",
        "rules": rules
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_generators() {
        let mut routing = RoutingSettings::default();
        routing.custom_rules.push(RuleItem {
            id: "1".to_string(),
            enabled: true,
            rule_type: RuleType::DomainSuffix,
            pattern: "google.com".to_string(),
            action: RuleAction::Proxy,
            description: None,
            sort_order: 0,
        });

        let sb = generate_singbox_route(&routing, "proxy");
        assert!(sb.get("rules").is_some());
        assert_eq!(sb["final"], "proxy");

        let mihomo = generate_mihomo_rules_yaml(&routing, "PROXY");
        assert!(mihomo.contains("DOMAIN-SUFFIX,google.com,PROXY"));
        assert!(mihomo.contains("MATCH,PROXY"));

        let xray = generate_xray_routing(&routing, "proxy-out");
        assert!(xray.get("rules").is_some());
    }
}
