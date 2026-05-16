// HTTP 探测模块。功能：
//   - 发起请求（GET/HEAD/POST/...）
//   - 测量总耗时、状态码、响应头、body 预览
//   - 对 HTTPS 提取证书信息（subject/issuer/SAN/有效期）
//
// 用 reqwest（rustls-tls）做请求，tls-info feature 暴露对端证书，x509-parser 解析 DER。

use reqwest::redirect::Policy;
use reqwest::tls::TlsInfo;
use reqwest::{Client, Method};
use serde::Serialize;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use x509_parser::prelude::*;

const BODY_PREVIEW_LIMIT: usize = 4096;

#[derive(Debug, Serialize, Clone)]
pub struct HttpProbeResult {
    pub success: bool,
    pub status: u16,
    pub status_text: String,
    pub final_url: String,
    pub redirects: Vec<String>,
    pub headers: Vec<(String, String)>,
    pub body_preview: String,
    pub body_truncated: bool,
    pub body_size_bytes: usize,
    pub total_ms: f64,
    pub error: Option<String>,
    pub tls: Option<TlsCertInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TlsCertInfo {
    pub subject: String,
    pub issuer: String,
    pub serial: String,
    pub sans: Vec<String>,
    pub not_before: String,
    pub not_after: String,
    pub days_until_expiry: i64,
    pub signature_algorithm: String,
}

#[tauri::command]
pub async fn http_request(
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    timeout_ms: u64,
    follow_redirects: bool,
) -> HttpProbeResult {
    let start = Instant::now();

    // 记录重定向链。Policy::custom 的闭包里没法直接写 Vec，得用 Arc<Mutex<>>。
    let redirects: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let redirects_clone = redirects.clone();

    let policy = if follow_redirects {
        Policy::custom(move |attempt| {
            if attempt.previous().len() > 10 {
                return attempt.error("redirect 次数超过 10");
            }
            if let Some(prev) = attempt.previous().last() {
                redirects_clone.lock().unwrap().push(prev.to_string());
            }
            attempt.follow()
        })
    } else {
        Policy::none()
    };

    let client_result = Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .redirect(policy)
        .tls_info(true)
        .user_agent("netools/0.1")
        .build();

    let client = match client_result {
        Ok(c) => c,
        Err(e) => return fail(&url, start, format!("client 构建失败: {}", e)),
    };

    let method = match Method::from_str(&method.to_uppercase()) {
        Ok(m) => m,
        Err(_) => return fail(&url, start, format!("非法 HTTP method: {}", method)),
    };

    let mut req = client.request(method, &url);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    if let Some(b) = body {
        if !b.is_empty() {
            req = req.body(b);
        }
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => return fail(&url, start, format!("请求失败: {}", e)),
    };

    let status = response.status();
    let final_url = response.url().to_string();
    let response_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect();

    let tls = response
        .extensions()
        .get::<TlsInfo>()
        .and_then(|ti| ti.peer_certificate())
        .and_then(parse_cert);

    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => return fail(&final_url, start, format!("读取 body 失败: {}", e)),
    };

    let total_size = bytes.len();
    let truncated = total_size > BODY_PREVIEW_LIMIT;
    let preview_slice = &bytes[..total_size.min(BODY_PREVIEW_LIMIT)];
    let body_preview = String::from_utf8_lossy(preview_slice).to_string();

    let mut chain = redirects.lock().unwrap().clone();
    // 重定向链以"中间跳转"为主，最后再补上 final_url 让前端一眼看到完整路径
    if !chain.is_empty() && chain.last().map(|s| s.as_str()) != Some(final_url.as_str()) {
        chain.push(final_url.clone());
    }

    HttpProbeResult {
        success: status.is_success(),
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        final_url,
        redirects: chain,
        headers: response_headers,
        body_preview,
        body_truncated: truncated,
        body_size_bytes: total_size,
        total_ms: start.elapsed().as_secs_f64() * 1000.0,
        error: None,
        tls,
    }
}

fn fail(url: &str, start: Instant, msg: String) -> HttpProbeResult {
    HttpProbeResult {
        success: false,
        status: 0,
        status_text: String::new(),
        final_url: url.to_string(),
        redirects: vec![],
        headers: vec![],
        body_preview: String::new(),
        body_truncated: false,
        body_size_bytes: 0,
        total_ms: start.elapsed().as_secs_f64() * 1000.0,
        error: Some(msg),
        tls: None,
    }
}

fn parse_cert(der: &[u8]) -> Option<TlsCertInfo> {
    let (_, cert) = X509Certificate::from_der(der).ok()?;

    // SAN 提取
    let mut sans: Vec<String> = Vec::new();
    for ext in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                sans.push(format_general_name(name));
            }
        }
    }

    let not_after_ts = cert.validity().not_after.timestamp();
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days_until_expiry = (not_after_ts - now_ts) / 86_400;

    Some(TlsCertInfo {
        subject: cert.subject().to_string(),
        issuer: cert.issuer().to_string(),
        serial: cert.tbs_certificate.serial.to_str_radix(16),
        sans,
        not_before: cert.validity().not_before.to_string(),
        not_after: cert.validity().not_after.to_string(),
        days_until_expiry,
        signature_algorithm: format!("{:?}", cert.signature_algorithm.algorithm),
    })
}

fn format_general_name(name: &GeneralName) -> String {
    // x509-parser 的 GeneralName 没有统一 Display，针对常见类型挑出可读值
    match name {
        GeneralName::DNSName(s) => format!("DNS:{}", s),
        GeneralName::IPAddress(b) => match b.len() {
            4 => format!("IP:{}.{}.{}.{}", b[0], b[1], b[2], b[3]),
            16 => format!("IP:{:x?}", b),
            _ => format!("IP:{:?}", b),
        },
        GeneralName::URI(s) => format!("URI:{}", s),
        GeneralName::RFC822Name(s) => format!("email:{}", s),
        other => format!("{:?}", other),
    }
}
