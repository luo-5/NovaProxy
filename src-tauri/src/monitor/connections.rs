//! Clash API `/connections` inspection and closing helpers.

use std::time::Duration;

fn empty_connections_json() -> serde_json::Value {
    serde_json::json!({
        "downloadTotal": 0,
        "uploadTotal": 0,
        "connections": []
    })
}

/// Fetches active connection snapshots from Clash API `/connections`.
pub async fn fetch_clash_connections(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
) -> serde_json::Value {
    let url = format!("http://127.0.0.1:{clash_api_port}/connections");
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(2000))
        .build()
    else {
        return empty_connections_json();
    };

    let mut req = client.get(&url);
    if let Some(secret) = clash_api_secret.filter(|s| !s.is_empty()) {
        req = req.bearer_auth(secret);
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => resp
            .json::<serde_json::Value>()
            .await
            .unwrap_or_else(|_| empty_connections_json()),
        _ => empty_connections_json(),
    }
}

/// Closes a single connection (`id = Some(...)`) or all connections (`id = None`).
pub async fn close_clash_connection(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
    id: Option<&str>,
) -> Result<(), String> {
    let url = match id {
        Some(cid) => {
            let encoded = urlencoding::encode(cid);
            format!("http://127.0.0.1:{clash_api_port}/connections/{encoded}")
        }
        None => format!("http://127.0.0.1:{clash_api_port}/connections"),
    };

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(2000))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.delete(&url);
    if let Some(secret) = clash_api_secret.filter(|s| !s.is_empty()) {
        req = req.bearer_auth(secret);
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if resp.status().is_success() || resp.status() == reqwest::StatusCode::NO_CONTENT {
        Ok(())
    } else {
        Err(format!("Clash API returned {}", resp.status()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_fetch_and_close_clash_connections_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            for _ in 0..2 {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if req.starts_with("GET /connections") {
                        let body =
                            r#"{"downloadTotal":100,"uploadTotal":50,"connections":[{"id":"c1"}]}"#;
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                    } else if req.starts_with("DELETE /connections/c1") {
                        let _ = stream
                            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                            .await;
                    }
                }
            }
        });

        let conns = fetch_clash_connections(port, Some("test-secret")).await;
        assert_eq!(conns["downloadTotal"], 100);
        assert_eq!(conns["connections"].as_array().unwrap().len(), 1);

        close_clash_connection(port, Some("test-secret"), Some("c1"))
            .await
            .expect("delete connection succeeds");

        server.abort();
    }
}
