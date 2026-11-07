//! Configuration generators for sing-box, mihomo, and Xray-core
//! Orchestrates full core configurations, routing rules, DNS, and multi-core relays.

pub mod dns;
pub mod mihomo;
pub mod routing;
pub mod singbox;
pub mod xray;

use crate::model::node::{CoreType, ProxyNode};
use crate::model::routing::{DnsSettings, RoutingSettings};
use crate::model::settings::AppSettings;
use crate::model::subscription::NodeGroup;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("sing-box generation error: {0}")]
    SingBox(#[from] singbox::SingBoxGeneratorError),
    #[error("mihomo generation error: {0}")]
    Mihomo(#[from] mihomo::MihomoGeneratorError),
    #[error("Xray generation error: {0}")]
    Xray(#[from] xray::XrayGeneratorError),
    #[error("Relay configuration error: {0}")]
    Relay(String),
}

/// Unified entry point to generate the complete configuration for the specified core engine
pub fn generate_core_config(
    core: CoreType,
    nodes: &[ProxyNode],
    groups: &[NodeGroup],
    settings: &AppSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
    active_node_id: Option<&str>,
) -> Result<String, GeneratorError> {
    match core {
        CoreType::SingBox => {
            singbox::generate_singbox_config(nodes, groups, settings, dns, routing, active_node_id)
                .map_err(GeneratorError::SingBox)
        }

        CoreType::Mihomo => {
            mihomo::generate_mihomo_config(nodes, groups, settings, dns, routing, active_node_id)
                .map_err(GeneratorError::Mihomo)
        }

        CoreType::Xray => xray::generate_xray_config(nodes, settings, routing, active_node_id)
            .map_err(GeneratorError::Xray),
    }
}

/// Configuration bundle for multi-core cooperative relay
/// (e.g. Xray upstream SOCKS relay + sing-box primary engine for TUN & routing)
#[derive(Debug, Clone)]
pub struct MultiCoreRelayConfig {
    pub primary_core: CoreType,
    pub primary_config: String,
    pub upstream_core: CoreType,
    pub upstream_config: String,
    pub relay_port: u16,
}

/// Generates a two-tier multi-core relay configuration when a selected node requires
/// an upstream engine (such as Xray for XHTTP/mKCP) while sing-box or mihomo manages routing.
#[allow(clippy::too_many_arguments)]
pub fn generate_multi_core_relay(
    primary_core: CoreType,
    upstream_core: CoreType,
    upstream_node: &ProxyNode,
    all_nodes: &[ProxyNode],
    groups: &[NodeGroup],
    settings: &AppSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
    relay_port: u16,
) -> Result<MultiCoreRelayConfig, GeneratorError> {
    // 1. Build upstream config (e.g. Xray with local SOCKS5 inbound on relay_port)
    let mut upstream_settings = settings.clone();
    upstream_settings.mixed_port = relay_port;
    upstream_settings.allow_lan = false;

    let upstream_cfg = match upstream_core {
        CoreType::Xray => xray::generate_xray_config(
            std::slice::from_ref(upstream_node),
            &upstream_settings,
            routing,
            Some(&upstream_node.id),
        )?,
        CoreType::Mihomo => mihomo::generate_mihomo_config(
            std::slice::from_ref(upstream_node),
            &[],
            &upstream_settings,
            dns,
            routing,
            Some(&upstream_node.id),
        )?,
        CoreType::SingBox => singbox::generate_singbox_config(
            std::slice::from_ref(upstream_node),
            &[],
            &upstream_settings,
            dns,
            routing,
            Some(&upstream_node.id),
        )?,
    };

    // 2. Synthesize a local SOCKS5 node representing the relay in the primary core
    let relay_node = ProxyNode {
        id: format!("relay-{}", upstream_node.id),
        name: format!("{} (Relay via {})", upstream_node.name, upstream_core),
        server: "127.0.0.1".to_string(),
        port: relay_port,
        protocol: crate::model::node::ProtocolType::Socks5,
        tls: None,
        transport: None,
        settings: crate::model::node::ProtocolConfig::Socks5(crate::model::node::Socks5Config {
            username: None,
            password: None,
            udp: true,
        }),
        subscription_id: upstream_node.subscription_id.clone(),
        group_id: upstream_node.group_id.clone(),
        latency: upstream_node.latency,
        dialer_proxy: upstream_node.dialer_proxy.clone(),
        preferred_core: Some(primary_core),
        sort_order: upstream_node.sort_order,
    };

    let mut merged_nodes = all_nodes.to_vec();
    // Replace or insert relay node
    if let Some(pos) = merged_nodes.iter().position(|n| n.id == upstream_node.id) {
        merged_nodes[pos] = relay_node.clone();
    } else {
        merged_nodes.push(relay_node.clone());
    }

    let primary_cfg = generate_core_config(
        primary_core,
        &merged_nodes,
        groups,
        settings,
        dns,
        routing,
        Some(&relay_node.id),
    )?;

    Ok(MultiCoreRelayConfig {
        primary_core,
        primary_config: primary_cfg,
        upstream_core,
        upstream_config: upstream_cfg,
        relay_port,
    })
}
