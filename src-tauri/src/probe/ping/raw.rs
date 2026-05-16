// 原始套接字版 ping（surge-ping）。需要 sudo / 管理员权限。

use super::PingResult;
use std::net::IpAddr;
use std::time::Duration;
use surge_ping::{Client, Config, IcmpPacket, PingIdentifier, PingSequence, ICMP};
use tauri::Emitter;

pub async fn ping_host(
    app: &tauri::AppHandle,
    host: String,
    count: u16,
    timeout_ms: u64,
) -> Result<Vec<PingResult>, String> {
    let addr = resolve_host(&host).await?;

    let config = match addr {
        IpAddr::V4(_) => Config::default(),
        IpAddr::V6(_) => Config::builder().kind(ICMP::V6).build(),
    };

    let client = Client::new(&config).map_err(|e| {
        format!(
            "无法创建 ICMP client（macOS/Linux 需要 sudo 启动）: {}",
            e
        )
    })?;

    let mut pinger = client.pinger(addr, PingIdentifier(rand::random())).await;
    pinger.timeout(Duration::from_millis(timeout_ms));

    let payload = [0u8; 56];
    let mut results = Vec::with_capacity(count as usize);

    for i in 0..count {
        let r = match pinger.ping(PingSequence(i), &payload).await {
            Ok((packet, rtt)) => {
                let from = match packet {
                    IcmpPacket::V4(p) => p.get_source().to_string(),
                    IcmpPacket::V6(p) => p.get_source().to_string(),
                };
                PingResult {
                    seq: i,
                    rtt_ms: rtt.as_secs_f64() * 1000.0,
                    from,
                    success: true,
                    error: None,
                }
            }
            Err(e) => PingResult {
                seq: i,
                rtt_ms: 0.0,
                from: addr.to_string(),
                success: false,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit("ping-result", &r);
        results.push(r);
        if i + 1 < count {
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }

    Ok(results)
}

async fn resolve_host(host: &str) -> Result<IpAddr, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ip);
    }
    let mut addrs = tokio::net::lookup_host(format!("{}:0", host))
        .await
        .map_err(|e| format!("DNS 解析失败 '{}': {}", host, e))?;
    addrs
        .next()
        .map(|s| s.ip())
        .ok_or_else(|| format!("'{}' 没有 DNS 记录", host))
}
