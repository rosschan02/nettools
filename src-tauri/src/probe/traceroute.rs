// Traceroute 模块。子进程方式调系统 traceroute（macOS/Linux）/ tracert（Windows）。
//
// 设计：
//   - traceroute 通常要 30-60s，全跑完再返回体验太差
//   - 所以每解析出一跳就通过 Tauri event "trace-hop" 推给前端
//   - 命令本身是 async fn，完成时再 emit "trace-done"
//
// 跨平台差异：
//   - macOS: traceroute -n -q 3 -w <sec> -m <hops> <host>
//   - Linux: traceroute -n -q 3 -w <sec> -m <hops> <host>
//   - Windows: tracert -d -h <hops> -w <ms> <host>，spawn 加 CREATE_NO_WINDOW
//
// 输出编码：Unix 通常 UTF-8；中文 Windows tracert 是 GBK，需要逐行 raw bytes
// 读出来再用 encoding_rs 解码，所以这里用 read_until(b'\n') 而不是 lines()。

use serde::Serialize;
use std::process::Stdio;
use std::time::Instant;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Serialize, Clone)]
pub struct TraceHop {
    pub ttl: u8,
    pub addr: Option<String>,
    pub rtts: Vec<Option<f64>>, // 通常 3 个，None 表示这次探测超时
    pub raw: String,            // 原始行，前端可显示原文做兜底
}

#[derive(Debug, Serialize, Clone)]
pub struct TraceDone {
    pub total_ms: f64,
    pub hop_count: usize,
}

#[tauri::command]
pub async fn traceroute_run(
    app: tauri::AppHandle,
    host: String,
    max_hops: u8,
    timeout_ms: u64,
) -> Result<(), String> {
    let start = Instant::now();
    let hops = traceroute_collect(host, max_hops, timeout_ms).await?;
    for hop in &hops {
        let _ = app.emit("trace-hop", hop);
    }
    let _ = app.emit(
        "trace-done",
        TraceDone {
            total_ms: start.elapsed().as_secs_f64() * 1000.0,
            hop_count: hops.len(),
        },
    );
    Ok(())
}

pub async fn traceroute_collect(
    host: String,
    max_hops: u8,
    timeout_ms: u64,
) -> Result<Vec<TraceHop>, String> {
    let mut cmd = build_command(&host, max_hops, timeout_ms);

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 traceroute 失败: {}", e))?;

    let stdout = child.stdout.take().ok_or("无 stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    let mut hops = Vec::new();

    loop {
        buf.clear();
        let n = reader
            .read_until(b'\n', &mut buf)
            .await
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let line = decode_bytes(&buf);
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some(hop) = parse_line(trimmed) {
            hops.push(hop);
        }
    }

    // 等子进程退出 + 收集 stderr 帮助诊断
    let stderr = child.stderr.take();
    let status = child.wait().await.map_err(|e| e.to_string())?;
    let mut stderr_text = String::new();
    if !status.success() {
        if let Some(mut err) = stderr {
            let mut bytes = Vec::new();
            let _ = err.read_to_end(&mut bytes).await;
            stderr_text = decode_bytes(&bytes);
        }
    }

    if !status.success() && hops.is_empty() {
        return Err(format!(
            "traceroute 失败 (exit {:?}): {}",
            status.code(),
            stderr_text.trim()
        ));
    }
    Ok(hops)
}

fn build_command(host: &str, max_hops: u8, timeout_ms: u64) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("tracert");
        cmd.args([
            "-d",
            "-h",
            &max_hops.to_string(),
            "-w",
            &timeout_ms.to_string(),
            host,
        ]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        cmd
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let wait_s = timeout_ms.div_ceil(1000).max(1).to_string();
        let mut cmd = Command::new("traceroute");
        cmd.args([
            "-n",
            "-q",
            "3",
            "-w",
            &wait_s,
            "-m",
            &max_hops.to_string(),
            host,
        ]);
        cmd
    }
}

/// 把子进程输出字节解码成字符串。UTF-8 优先，遇到非 UTF-8 字节回退 GBK
/// （主要给中文 Windows tracert 用）。
fn decode_bytes(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
    decoded.into_owned()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_line(line: &str) -> Option<TraceHop> {
    parse_unix_line(line)
}

#[cfg(target_os = "windows")]
fn parse_line(line: &str) -> Option<TraceHop> {
    parse_windows_line(line)
}

/// 解析 macOS/Linux 的 traceroute 输出。
///
/// 例：
///   " 1  10.0.0.1  1.234 ms  1.567 ms  1.345 ms"
///   " 2  * * *"
///   " 3  router.foo (192.168.1.1)  10.123 ms  9.876 ms  10.234 ms"
///   " 4  192.168.1.2  10.123 ms 192.168.1.3  9.876 ms 192.168.1.2  10.234 ms"  (多探测命中不同路由)
#[allow(dead_code)]
fn parse_unix_line(line: &str) -> Option<TraceHop> {
    let trimmed = line.trim_start();
    let mut tokens = trimmed.split_whitespace().peekable();

    // 第一个 token 必须是 hop 序号
    let ttl: u8 = tokens.next()?.parse().ok()?;

    let mut rtts: Vec<Option<f64>> = Vec::new();
    let mut addr: Option<String> = None;

    while let Some(tok) = tokens.next() {
        if tok == "*" {
            rtts.push(None);
            continue;
        }
        // 在括号里：可能是 (ip) 形式，提取 IP
        if tok.starts_with('(') && tok.ends_with(')') {
            let inner = &tok[1..tok.len() - 1];
            if addr.is_none() && is_ip_like(inner) {
                addr = Some(inner.to_string());
            }
            continue;
        }
        // 数字 + 紧跟 "ms" → 是 RTT
        if let Ok(rtt) = tok.parse::<f64>() {
            if tokens.peek().copied() == Some("ms") {
                tokens.next();
                rtts.push(Some(rtt));
                continue;
            }
        }
        // 看起来是 IP
        if is_ip_like(tok) {
            if addr.is_none() {
                addr = Some(tok.to_string());
            }
            continue;
        }
        // 其余视为 hostname，忽略（我们用 -n，正常不会出现）
    }

    Some(TraceHop {
        ttl,
        addr,
        rtts,
        raw: line.to_string(),
    })
}

/// 解析 Windows tracert 输出。
///
/// 例：
///   "  1     1 ms     1 ms     1 ms  10.0.0.1"
///   "  2     *        *        *     Request timed out."
#[allow(dead_code)]
fn parse_windows_line(line: &str) -> Option<TraceHop> {
    let trimmed = line.trim_start();
    let mut tokens = trimmed.split_whitespace().peekable();

    let ttl: u8 = tokens.next()?.parse().ok()?;

    let mut rtts: Vec<Option<f64>> = Vec::new();
    let mut addr: Option<String> = None;

    // Windows: 接下来通常是 3 组 "<n> ms" 或 "*"
    for _ in 0..3 {
        match tokens.next() {
            Some("*") => rtts.push(None),
            Some(t) => {
                if let Some(stripped) = t.strip_suffix("ms") {
                    rtts.push(stripped.parse::<f64>().ok());
                } else if let Ok(v) = t.parse::<f64>() {
                    // 如果数字和 "ms" 分开（"1 ms"），下一个 token 是 "ms"
                    if tokens.peek().copied() == Some("ms") {
                        tokens.next();
                    }
                    rtts.push(Some(v));
                } else {
                    rtts.push(None);
                }
            }
            None => break,
        }
    }

    // 剩下的 token 拼起来找 IP
    let rest: String = tokens.collect::<Vec<_>>().join(" ");
    for word in rest.split_whitespace() {
        let candidate = word.trim_matches(|c: char| c == '[' || c == ']');
        if is_ip_like(candidate) {
            addr = Some(candidate.to_string());
            break;
        }
    }

    Some(TraceHop {
        ttl,
        addr,
        rtts,
        raw: line.to_string(),
    })
}

fn is_ip_like(s: &str) -> bool {
    use std::net::IpAddr;
    s.parse::<IpAddr>().is_ok()
}
