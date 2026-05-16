// 简单的服务指纹识别。两步策略：
//   1. 被动监听：很多协议（SSH/SMTP/FTP/POP3/IMAP）服务端主动发 banner，读 200ms 看看
//   2. 主动 HTTP 探测：80/8080 这类端口服务端不主动发，发个 HEAD 请求看是否回 "HTTP/"
//   3. 兜底：按端口号做"概率猜测"（"HTTPS (probable)"）

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const READ_LIMIT: usize = 256;
const PASSIVE_TIMEOUT: Duration = Duration::from_millis(300);
const ACTIVE_TIMEOUT: Duration = Duration::from_millis(500);

pub async fn detect(stream: &mut TcpStream, port: u16) -> (Option<String>, Option<String>) {
    // 1. 被动读 banner
    if let Some(banner) = try_read(stream, PASSIVE_TIMEOUT).await {
        let service = classify(&banner, port).unwrap_or_else(|| port_guess(port));
        return (Some(service), Some(banner));
    }

    // 2. 对常见 HTTP 端口发 HEAD 探测
    if is_likely_http(port) {
        let probe = b"HEAD / HTTP/1.0\r\n\r\n";
        if stream.write_all(probe).await.is_ok() {
            if let Some(banner) = try_read(stream, ACTIVE_TIMEOUT).await {
                if banner.contains("HTTP/") {
                    return (Some("HTTP".to_string()), Some(banner));
                }
                // 返回了但不是 HTTP——记录原始 banner 供前端展示
                return (
                    Some(port_guess(port)),
                    Some(banner),
                );
            }
        }
    }

    // 3. 端口号猜测
    (Some(port_guess(port)), None)
}

async fn try_read(stream: &mut TcpStream, timeout: Duration) -> Option<String> {
    let mut buf = vec![0u8; READ_LIMIT];
    match tokio::time::timeout(timeout, stream.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => {
            buf.truncate(n);
            // banner 通常是 ASCII，用 lossy 处理含非 UTF-8 字节的情况
            Some(String::from_utf8_lossy(&buf).into_owned())
        }
        _ => None,
    }
}

fn classify(banner: &str, _port: u16) -> Option<String> {
    let trimmed = banner.trim_start();

    if trimmed.starts_with("SSH-") {
        return Some("SSH".to_string());
    }
    if trimmed.starts_with("+OK") {
        return Some("POP3".to_string());
    }
    if trimmed.starts_with("* OK") {
        return Some("IMAP".to_string());
    }
    if trimmed.contains("HTTP/1") || trimmed.contains("HTTP/2") {
        return Some("HTTP".to_string());
    }
    if trimmed.starts_with("220 ") || trimmed.starts_with("220-") {
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("ftp") {
            return Some("FTP".to_string());
        }
        if lower.contains("smtp") || lower.contains("esmtp") {
            return Some("SMTP".to_string());
        }
        return Some("SMTP/FTP (220 banner)".to_string());
    }
    // Redis 不主动发 banner，但发 "PING\r\n" 会回 "+PONG\r\n"。这里被动模式抓不到，留给主动模式扩展。
    None
}

fn is_likely_http(port: u16) -> bool {
    matches!(
        port,
        80 | 81 | 591 | 2480 | 4567 | 5000 | 5001 | 8000 | 8008 | 8080 | 8081 | 8088 | 8888 | 9000 | 3000
    )
}

fn port_guess(port: u16) -> String {
    match port {
        20 | 21 => "FTP (probable)",
        22 => "SSH (probable)",
        23 => "Telnet (probable)",
        25 | 587 | 465 => "SMTP (probable)",
        53 => "DNS (probable)",
        80 | 8080 | 8000 | 8008 | 8088 | 3000 | 5000 => "HTTP (probable)",
        110 => "POP3 (probable)",
        143 => "IMAP (probable)",
        443 | 8443 => "HTTPS/TLS (probable)",
        993 => "IMAPS (probable)",
        995 => "POP3S (probable)",
        3306 => "MySQL (probable)",
        5432 => "PostgreSQL (probable)",
        6379 => "Redis (probable)",
        27017 => "MongoDB (probable)",
        5900..=5910 => "VNC (probable)",
        3389 => "RDP (probable)",
        _ => "open",
    }
    .to_string()
}
