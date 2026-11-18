//! Live Clash API `/traffic` monitor.
//! Reads a single NDJSON line from the chunked streaming endpoint with `.no_proxy()`
//! so active system proxy settings never loop back into traffic queries.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrafficSnapshot {
    #[ts(type = "number")]
    pub up: u64,
    #[ts(type = "number")]
    pub down: u64,
}

/// Fetches the current upload/download byte rate snapshot from Clash API `/traffic`.
pub async fn fetch_clash_traffic(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
) -> TrafficSnapshot {
    let url = format!("http://127.0.0.1:{clash_api_port}/traffic");
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_millis(600))
        .timeout(Duration::from_millis(1600))
        .build()
    else {
        return TrafficSnapshot::default();
    };

    let mut req = client.get(&url);
    if let Some(secret) = clash_api_secret.filter(|s| !s.is_empty()) {
        req = req.bearer_auth(secret);
    }

    let Ok(mut resp) = req.send().await else {
        return TrafficSnapshot::default();
    };
    if !resp.status().is_success() {
        return TrafficSnapshot::default();
    }

    let mut buffer = Vec::with_capacity(256);
    while let Ok(Some(chunk)) = resp.chunk().await {
        buffer.extend_from_slice(&chunk);
        if let Some(nl_pos) = buffer.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&buffer[..nl_pos]);
            if let Ok(snapshot) = serde_json::from_str::<TrafficSnapshot>(line.trim()) {
                return snapshot;
            }
            buffer.drain(..=nl_pos);
        }
        if buffer.len() > 8192 {
            break;
        }
    }

    if !buffer.is_empty() {
        let text = String::from_utf8_lossy(&buffer);
        if let Ok(snapshot) = serde_json::from_str::<TrafficSnapshot>(text.trim()) {
            return snapshot;
        }
    }

    TrafficSnapshot::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_fetch_clash_traffic_reads_first_ndjson_chunk_without_waiting_eof() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut req_buf = [0u8; 512];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut req_buf).await;
                let header = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n";
                let payload = "{\"up\":1234,\"down\":5678}\n";
                let chunk = format!("{:x}\r\n{}\r\n", payload.len(), payload);
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(chunk.as_bytes()).await;
                // Keep socket open without EOF to prove we parse immediately on first line
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });

        let started = std::time::Instant::now();
        let snap = fetch_clash_traffic(port, None).await;
        assert_eq!(snap.up, 1234);
        assert_eq!(snap.down, 5678);
        assert!(started.elapsed() < Duration::from_secs(1));

        server.abort();
    }
}
