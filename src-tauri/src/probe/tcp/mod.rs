// TCP 探测模块。三个对外命令：
//   - tcp_probe   单端口连接 + 可选 fingerprint
//   - tcp_scan    多端口并发扫描 + 可选 fingerprint
//   - tcp_ping    对同一端口连续探测（看握手延迟稳定性）

mod connect;
mod fingerprint;

use serde::Serialize;
use std::time::Duration;
use tauri::Emitter;

#[derive(Debug, Serialize, Clone)]
pub struct TcpProbeResult {
    pub host: String,
    pub port: u16,
    pub success: bool,
    pub rtt_ms: f64,
    pub error: Option<String>,
    pub service: Option<String>, // 识别出的服务名（HTTP/SSH/...）
    pub banner: Option<String>,  // 抓到的 banner 前 256 字节（截断为 UTF-8 文本）
}

#[tauri::command]
pub async fn tcp_probe(
    host: String,
    port: u16,
    timeout_ms: u64,
    fingerprint: bool,
) -> TcpProbeResult {
    connect::probe_one(host, port, Duration::from_millis(timeout_ms), fingerprint).await
}

#[tauri::command]
pub async fn tcp_scan(
    host: String,
    ports: Vec<u16>,
    timeout_ms: u64,
    fingerprint: bool,
    concurrency: usize,
) -> Vec<TcpProbeResult> {
    use futures::stream::{self, StreamExt};

    let timeout = Duration::from_millis(timeout_ms);
    let conc = concurrency.clamp(1, 256); // 限制最大并发，避免炸文件描述符

    let mut results: Vec<TcpProbeResult> = stream::iter(ports.into_iter())
        .map(|port| {
            let host = host.clone();
            async move { connect::probe_one(host, port, timeout, fingerprint).await }
        })
        .buffer_unordered(conc)
        .collect()
        .await;

    // buffer_unordered 顺序乱，按 port 升序整理一下方便展示
    results.sort_by_key(|r| r.port);
    results
}

#[tauri::command]
pub async fn tcp_ping(
    app: tauri::AppHandle,
    host: String,
    port: u16,
    count: u16,
    interval_ms: u64,
    timeout_ms: u64,
) -> Vec<TcpProbeResult> {
    let timeout = Duration::from_millis(timeout_ms);
    let mut results = Vec::with_capacity(count as usize);
    for i in 0..count {
        let r = connect::probe_one(host.clone(), port, timeout, false).await;
        // 用 TcpPingEvent 包一层把 seq 也传给前端，图表用 seq 做 X 轴
        let _ = app.emit(
            "tcp-ping-result",
            &TcpPingEvent { seq: i, result: r.clone() },
        );
        results.push(r);
        if (i + 1) < count {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    }
    results
}

#[derive(Debug, Serialize, Clone)]
struct TcpPingEvent {
    seq: u16,
    result: TcpProbeResult,
}
