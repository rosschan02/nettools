// 子进程版 ping：调用系统 ping 命令、解析输出。
// 优点：无需任何权限；跨平台（macOS/Linux/Windows 各有差异，下面用 cfg 处理）。
// 缺点：受系统 ping 输出格式约束、性能略低（每包一次进程启动）。

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

async fn ping_once(host: &str, seq: u16, timeout_ms: u64) -> PingResult {
    let mut cmd = Command::new("ping");

    // 各平台 ping 参数不同：
    //   macOS: -c <count> 包数, -W <ms> 每包等待毫秒
    //   Linux: -c <count> 包数, -W <s>  每包等待秒
    //   Windows: -n <count>, -w <ms> 每包等待毫秒
    #[cfg(target_os = "windows")]
    cmd.args(["-n", "1", "-w", &timeout_ms.to_string(), host]);

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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
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

// 在输出里找 "time=23.456 ms" / "time=23ms" / "time<1ms" 这种模式
fn parse_rtt_ms(text: &str) -> Option<f64> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let Some(i) = lower.find("time") else {
            continue;
        };
        let rest = &line[i + 4..];
        let mut chars = rest.chars();
        let separator = chars.next()?;
        if separator != '=' && separator != '<' {
            continue;
        }
        let num_str: String = chars
            .skip_while(|c| !c.is_ascii_digit() && *c != '.')
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Ok(ms) = num_str.parse::<f64>() {
            // "time<1ms" 表示小于 1ms，估算成 0.5ms
            return Some(if separator == '<' { ms / 2.0 } else { ms });
        }
    }
    None
}

// 解析响应源地址：
//   Unix:    "64 bytes from 8.8.8.8: ..."  或  "from host.example (1.2.3.4):"
//   Windows: "Reply from 8.8.8.8: ..."
fn parse_source(text: &str) -> Option<String> {
    for line in text.lines() {
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
