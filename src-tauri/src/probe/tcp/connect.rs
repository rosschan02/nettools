// 单次 TCP 连接 + 计时 + 可选 fingerprint。
//
// 设计要点：
//   - 用 tokio::time::timeout 包裹 connect，避免无限阻塞
//   - 连接成功后才计时为"成功 RTT"；超时/拒绝走错误分支但 rtt_ms 仍记录用时
//   - fingerprint 调用见 fingerprint.rs，会消耗这个 TcpStream

use super::fingerprint;
use super::TcpProbeResult;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

pub async fn probe_one(
    host: String,
    port: u16,
    timeout: Duration,
    do_fingerprint: bool,
) -> TcpProbeResult {
    let start = Instant::now();
    let connect_result = tokio::time::timeout(timeout, TcpStream::connect((host.as_str(), port))).await;
    let rtt_ms = start.elapsed().as_secs_f64() * 1000.0;

    match connect_result {
        Ok(Ok(mut stream)) => {
            let (service, banner) = if do_fingerprint {
                fingerprint::detect(&mut stream, port).await
            } else {
                (None, None)
            };
            TcpProbeResult {
                host,
                port,
                success: true,
                rtt_ms,
                error: None,
                service,
                banner,
            }
        }
        Ok(Err(e)) => TcpProbeResult {
            host,
            port,
            success: false,
            rtt_ms,
            error: Some(format!("连接失败: {}", e)),
            service: None,
            banner: None,
        },
        Err(_elapsed) => TcpProbeResult {
            host,
            port,
            success: false,
            rtt_ms,
            error: Some(format!("超时 ({:.0} ms)", timeout.as_millis())),
            service: None,
            banner: None,
        },
    }
}
