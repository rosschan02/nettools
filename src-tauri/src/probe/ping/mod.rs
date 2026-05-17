// ping 模块入口。两个后端实现：
//   - subprocess：调用系统 ping 解析输出（默认，无需 sudo）
//   - raw：surge-ping 原始套接字（更精确，但 macOS/Linux 需要 sudo）
//
// 切换方式：环境变量 NETOOLS_PING_BACKEND=raw 选 raw，缺省走 subprocess。
//
// 每发出一个 ping 探测就 emit "ping-result" 事件给前端（实时图表用），
// 同时整个 command 仍返回完整 Vec 方便不监听事件的调用方。

mod raw;
mod subprocess;

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct PingResult {
    pub seq: u16,
    pub rtt_ms: f64,
    pub from: String,
    pub success: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ping_host(
    app: tauri::AppHandle,
    host: String,
    count: u16,
    timeout_ms: u64,
) -> Result<Vec<PingResult>, String> {
    let backend =
        std::env::var("NETOOLS_PING_BACKEND").unwrap_or_else(|_| "subprocess".to_string());
    match backend.as_str() {
        "raw" => raw::ping_host(&app, host, count, timeout_ms).await,
        _ => subprocess::ping_host(&app, host, count, timeout_ms).await,
    }
}

/// 纯 CLI 使用的 ping：不依赖 Tauri AppHandle，也不 emit 前端事件。
/// Linux 版本默认走 CLI，因此这里固定复用无需权限的 subprocess 后端。
pub async fn ping_host_cli(
    host: String,
    count: u16,
    timeout_ms: u64,
) -> Result<Vec<PingResult>, String> {
    subprocess::ping_host_no_emit(host, count, timeout_ms).await
}

/// 返回当前生效的 ping 后端，给前端显示用
#[tauri::command]
pub fn ping_backend() -> String {
    std::env::var("NETOOLS_PING_BACKEND").unwrap_or_else(|_| "subprocess".to_string())
}
