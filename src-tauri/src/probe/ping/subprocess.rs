// 子进程版 ping：调用系统 ping 命令、解析输出。
// 优点：无需任何权限；跨平台（macOS/Linux/Windows 各有差异，下面用 cfg 处理）。
// 缺点：受系统 ping 输出格式约束、性能略低（每包一次进程启动）。
//
// Windows 上还有两个细节要处理：
//   1. 默认 spawn 会弹一个黑色 cmd 窗口，加 CREATE_NO_WINDOW(0x08000000) flag 关掉。
//   2. 中文 Windows 的 ping 输出是 GBK 编码、关键词是中文（"时间"、"来自"），
//      所以下面用 encoding_rs 做 UTF-8→GBK 的回退，解析器同时认中英文。

use super::PingResult;
use std::time::Duration;
use tauri::Emitter;
use tokio::process::Command;

pub async fn ping_host(
    app: &tauri::AppHandle,
    host: String,
    count: u16,
    timeout_ms: u64,
) -> Result<Vec<PingResult>, String> {
    let mut results = Vec::with_capacity(count as usize);
    for i in 0..count {
        let r = ping_once(&host, i, timeout_ms).await;
        let _ = app.emit("ping-result", &r);
        results.push(r);
        if i + 1 < count {
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }
    Ok(results)
}

pub async fn ping_host_no_emit(
    host: String,
    count: u16,
    timeout_ms: u64,
) -> Result<Vec<PingResult>, String> {
    let mut results = Vec::with_capacity(count as usize);
    for i in 0..count {
        let r = ping_once(&host, i, timeout_ms).await;
        results.push(r);
        if i + 1 < count {
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }
    Ok(results)
}

async fn ping_once(host: &str, seq: u16, timeout_ms: u64) -> PingResult {
    let mut cmd = Command::new("ping");

    // 各平台 ping 参数不同：
    //   macOS: -c <count> 包数, -W <ms> 每包等待毫秒
    //   Linux: -c <count> 包数, -W <s>  每包等待秒
    //   Windows: -n <count>, -w <ms> 每包等待毫秒
    #[cfg(target_os = "windows")]
    {
        cmd.args(["-n", "1", "-w", &timeout_ms.to_string(), host]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    #[cfg(target_os = "macos")]
    cmd.args(["-c", "1", "-W", &timeout_ms.to_string(), host]);

    #[cfg(target_os = "linux")]
    {
        let timeout_s = timeout_ms.div_ceil(1000).max(1).to_string();
        cmd.args(["-c", "1", "-W", &timeout_s, host]);
    }

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => {
            return PingResult {
                seq,
                rtt_ms: 0.0,
                from: host.to_string(),
                success: false,
                error: Some(format!("启动 ping 失败: {}", e)),
            };
        }
    };

    let stdout = decode_bytes(&output.stdout);
    let stderr = decode_bytes(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    let rtt = parse_rtt_ms(&combined);
    let from = parse_source(&combined).unwrap_or_else(|| host.to_string());

    match rtt {
        Some(ms) => PingResult {
            seq,
            rtt_ms: ms,
            from,
            success: true,
            error: None,
        },
        None => PingResult {
            seq,
            rtt_ms: 0.0,
            from,
            success: false,
            error: Some(extract_error(&combined)),
        },
    }
}

/// 把子进程输出字节解码成字符串。
/// 多数系统输出 UTF-8；中文 Windows 输出 GBK，遇到非 UTF-8 时回退 GBK。
fn decode_bytes(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
    decoded.into_owned()
}

/// 在 reply 行里找 RTT。识别两套关键词：
///   英文：bytes from / reply from
///   中文：来自 / 字节=
/// 然后在该行扫描 "[=<]<数字>[空白]?ms" 模式。这样
///   英文：time=23.4 ms / time=203ms / time<1ms
///   中文：时间=203ms
/// 都能命中。
fn parse_rtt_ms(text: &str) -> Option<f64> {
    for line in text.lines() {
        if !is_reply_line(line) {
            continue;
        }
        if let Some(rtt) = find_ms_value(line) {
            return Some(rtt);
        }
    }
    None
}

fn is_reply_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("bytes from")
        || lower.contains("reply from")
        || line.contains("来自")
        || line.contains("字节=")
}

fn find_ms_value(line: &str) -> Option<f64> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'=' || b == b'<' {
            let is_less = b == b'<';
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            let num_start = j;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            if j > num_start {
                let num_end = j;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b'm' && bytes[j + 1] == b's' {
                    if let Ok(s) = std::str::from_utf8(&bytes[num_start..num_end]) {
                        if let Ok(ms) = s.parse::<f64>() {
                            return Some(if is_less { ms / 2.0 } else { ms });
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
}

// 解析响应源地址：
//   Unix:       "64 bytes from 8.8.8.8: ..."  或  "from host.example (1.2.3.4):"
//   Windows EN: "Reply from 8.8.8.8: ..."
//   Windows CN: "来自 8.8.8.8 的回复: ..."
fn parse_source(text: &str) -> Option<String> {
    for line in text.lines() {
        // 中文 Windows 优先识别
        if let Some(rest) = line.split("来自").nth(1) {
            let token = rest.trim_start().split_whitespace().next()?;
            return Some(token.trim_end_matches([':', ',', '：', '，']).to_string());
        }
        let lower = line.to_ascii_lowercase();
        if !(lower.contains("bytes from") || lower.contains("reply from")) {
            continue;
        }
        let token = line
            .split_whitespace()
            .skip_while(|w| !w.eq_ignore_ascii_case("from"))
            .nth(1)?;
        return Some(token.trim_end_matches([':', ',']).to_string());
    }
    None
}

fn extract_error(text: &str) -> String {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("timeout")
            || lower.contains("timed out")
            || lower.contains("unknown host")
            || lower.contains("unreachable")
            || lower.contains("cannot resolve")
            || lower.contains("not be resolved")
            || line.contains("请求超时")
            || line.contains("找不到主机")
            || line.contains("无法访问")
            || line.contains("传输失败")
        {
            return line.trim().to_string();
        }
    }
    text.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("ping 失败")
        .trim()
        .to_string()
}
