//! Routing rules configuration generators for sing-box, mihomo, and Xray-core

use crate::model::routing::{RoutingSettings, RuleAction, RuleItem, RuleType};
use crate::model::settings::RoutingMode;
use serde_json::{json, Value};

/// Generates sing-box 1.14 route configuration block
pub fn generate_singbox_route(routing: &RoutingSettings, proxy_tag: &str) -> Value {
    let mut rules = Vec::new();

    // 1. Sniff and hijack DNS
    rules.push(json!({ "action": "sniff" }));
    rules.push(json!({ "protocol": "dns", "action": "hijack-dns" }));

    let mut rule_sets = Vec::new();

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
                if !item.enabled {
                    continue;
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
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": "geosite-ads",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs",
                    "download_detour": "direct",
                    "update_interval": "24h"
                }));
            }

            // Preset: CN Direct (Domains + IPs)
            if routing.cn_direct {
                rules.push(json!({
                    "rule_set": ["geosite-cn", "geoip-cn"],
                    "outbound": "direct"
                }));
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": "geosite-cn",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-cn.srs",
                    "download_detour": "direct",
                    "update_interval": "24h"
                }));
                rule_sets.push(json!({
                    "type": "remote",
                    "tag": "geoip-cn",
                    "format": "binary",
                    "url": "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-cn.srs",
                    "download_detour": "direct",
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
    let mut map = serde_json::Map::new();

    let target = item.action.target_tag(default_proxy_tag);
    if item.action == RuleAction::Block {
        map.insert("action".to_string(), json!("reject"));
    } else {
        map.insert("outbound".to_string(), json!(target));
    }

    match item.rule_type {
        RuleType::Domain => {
            map.insert("domain".to_string(), json!([item.pattern]));
        }
        RuleType::DomainSuffix => {
            let pat = item.pattern.trim_start_matches('.');
            map.insert("domain_suffix".to_string(), json!([pat]));
        }
        RuleType::DomainKeyword => {
            map.insert("domain_keyword".to_string(), json!([item.pattern]));
        }
        RuleType::DomainRegex => {
            map.insert("domain_regex".to_string(), json!([item.pattern]));
        }
        RuleType::IpCidr => {
            map.insert("ip_cidr".to_string(), json!([item.pattern]));
        }
        RuleType::ProcessName => {
            map.insert("process_name".to_string(), json!([item.pattern]));
        }
        RuleType::Port => {
            if let Ok(p) = item.pattern.trim().parse::<u16>() {
                map.insert("port".to_string(), json!([p]));
            } else {
                map.insert("port_range".to_string(), json!([item.pattern]));
            }
        }
        RuleType::GeoSite => {
            map.insert(
                "rule_set".to_string(),
                json!([format!("geosite-{}", item.pattern)]),
            );
        }
        RuleType::GeoIp => {
            map.insert(
                "rule_set".to_string(),
                json!([format!("geoip-{}", item.pattern)]),
            );
        }
    }

    Some(Value::Object(map))
}

/// Generates Mihomo (Clash.Meta) YAML rule-providers and rules
pub fn generate_mihomo_rules_yaml(routing: &RoutingSettings, proxy_tag: &str) -> String {
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
                if !item.enabled {
                    continue;
                }
                let target = match item.action {
                    RuleAction::Proxy => proxy_tag,
                    RuleAction::Direct => "DIRECT",
                    RuleAction::Block => "REJECT",
                    RuleAction::Tag(ref t) => t.as_str(),
                };

                let line = match item.rule_type {
                    RuleType::Domain => format!("  - DOMAIN,{},{}\n", item.pattern, target),
                    RuleType::DomainSuffix => format!(
                        "  - DOMAIN-SUFFIX,{},{}\n",
                        item.pattern.trim_start_matches('.'),
                        target
                    ),
                    RuleType::DomainKeyword => {
                        format!("  - DOMAIN-KEYWORD,{},{}\n", item.pattern, target)
                    }
                    RuleType::DomainRegex => {
                        format!("  - DOMAIN-REGEX,{},{}\n", item.pattern, target)
                    }
                    RuleType::IpCidr => {
                        format!("  - IP-CIDR,{},{},no-resolve\n", item.pattern, target)
                    }
                    RuleType::ProcessName => {
                        format!("  - PROCESS-NAME,{},{}\n", item.pattern, target)
                    }
                    RuleType::Port => format!("  - DST-PORT,{},{}\n", item.pattern, target),
                    RuleType::GeoSite => format!("  - GEOSITE,{},{}\n", item.pattern, target),
                    RuleType::GeoIp => {
                        format!("  - GEOIP,{},{},no-resolve\n", item.pattern, target)
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
                if !item.enabled {
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
                            "domain": [format!("full:{}", item.pattern)],
                            "outboundTag": target
                        }));
                    }
                    RuleType::DomainSuffix => {
                        rules.push(json!({
                            "type": "field",
                            "domain": [format!("domain:{}", item.pattern.trim_start_matches('.'))],
                            "outboundTag": target
                        }));
                    }
                    RuleType::DomainKeyword => {
                        rules.push(json!({
                            "type": "field",
                            "domain": [format!("keyword:{}", item.pattern)],
                            "outboundTag": target
                        }));
                    }
                    RuleType::DomainRegex => {
                        rules.push(json!({
                            "type": "field",
                            "domain": [format!("regexp:{}", item.pattern)],
                            "outboundTag": target
                        }));
                    }
                    RuleType::IpCidr => {
                        rules.push(json!({
                            "type": "field",
                            "ip": [item.pattern],
                            "outboundTag": target
                        }));
                    }
                    RuleType::Port => {
                        rules.push(json!({
                            "type": "field",
                            "port": item.pattern,
                            "outboundTag": target
                        }));
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
