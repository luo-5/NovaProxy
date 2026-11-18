//! Clash API dynamic switching and hot-reload client
//! Provides zero-downtime proxy node switching without restarting core processes.

use reqwest::Client;
use std::path::Path;
use std::time::Duration;

/// Attempts to switch the active proxy node dynamically via Clash API `PUT /proxies/{group}`.
/// Returns Ok(()) if the group was switched without restarting the core process.
pub async fn try_hot_switch_proxy(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
    group_name: &str,
    node_name: &str,
) -> Result<(), String> {
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(1500))
        .build()
        .map_err(|e| e.to_string())?;

    let encoded_group = urlencoding::encode(group_name);
    let url = format!(
        "http://127.0.0.1:{}/proxies/{}",
        clash_api_port, encoded_group
    );

    let mut req = client.put(&url).json(&serde_json::json!({
        "name": node_name
    }));

    if let Some(secret) = clash_api_secret {
        if !secret.is_empty() {
            req = req.bearer_auth(secret);
        }
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "Clash API returned status {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ))
    }
}

/// Attempts to reload the configuration dynamically via Clash API `PUT /configs`.
pub async fn try_hot_reload_config(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
    config_path: impl AsRef<Path>,
) -> Result<(), String> {
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(2000))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("http://127.0.0.1:{}/configs", clash_api_port);
    let path_str = config_path.as_ref().to_string_lossy().to_string();

    let mut req = client.put(&url).json(&serde_json::json!({
        "path": path_str
    }));

    if let Some(secret) = clash_api_secret {
        if !secret.is_empty() {
            req = req.bearer_auth(secret);
        }
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "Clash API config reload returned status {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ))
    }
}

/// Smart proxy switcher that attempts common group names (`proxy`, `PROXY`, `GLOBAL`)
/// against an ordered list of candidate proxy tags/names (ignoring empty or duplicate entries).
pub async fn try_smart_hot_switch_candidates(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
    candidates: &[&str],
) -> Result<(), String> {
    let groups = ["proxy", "PROXY", "GLOBAL"];
    let mut unique_candidates: Vec<&str> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() && !unique_candidates.contains(&trimmed) {
            unique_candidates.push(trimmed);
        }
    }

    if unique_candidates.is_empty() {
        return Err("No valid proxy candidates supplied".to_string());
    }

    let mut last_err = String::from("No candidate groups responded");

    for group in groups {
        for candidate in &unique_candidates {
            match try_hot_switch_proxy(clash_api_port, clash_api_secret, group, candidate).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = e;
                }
            }
        }
    }

    Err(last_err)
}

/// Smart proxy switcher that attempts common group names (`proxy`, `PROXY`, `GLOBAL`)
/// with both node ID and node display name.
pub async fn try_smart_hot_switch(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
    node_id: &str,
    node_name: &str,
) -> Result<(), String> {
    try_smart_hot_switch_candidates(clash_api_port, clash_api_secret, &[node_id, node_name]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hot_switch_proxy_handles_offline_gracefully() {
        // When core is offline, it returns an error instead of panicking
        let res = try_hot_switch_proxy(39191, None, "GLOBAL", "NodeA").await;
        assert!(res.is_err());
    }
}
