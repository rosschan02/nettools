// 一键诊断编排。复用已有 probe，并行执行后汇总成 GUI/CLI 共用的结构化报告。

use super::{dns, http, ping, tcp, traceroute};
use serde::Serialize;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use url::Url;

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Serialize, Clone)]
pub struct DiagnosticStep<T> {
    pub status: DiagnosticStatus,
    pub duration_ms: f64,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DiagnosticSummary {
    pub status: DiagnosticStatus,
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct DiagnosticReport {
    pub target: String,
    pub host: String,
    pub url: String,
    pub port: u16,
    pub generated_at_unix_ms: u64,
    pub total_ms: f64,
    pub summary: DiagnosticSummary,
    pub ping: DiagnosticStep<Vec<ping::PingResult>>,
    pub dns: DiagnosticStep<Vec<dns::DnsServerResult>>,
    pub tcp: DiagnosticStep<tcp::TcpProbeResult>,
    pub http: DiagnosticStep<http::HttpProbeResult>,
    pub traceroute: DiagnosticStep<Vec<traceroute::TraceHop>>,
}

#[tauri::command]
pub async fn diagnose(
    target: String,
    include_trace: bool,
    timeout_ms: u64,
    max_hops: u8,
    ping_count: u16,
) -> Result<DiagnosticReport, String> {
    let normalized = normalize_target(&target)?;
    let timeout_ms = timeout_ms.clamp(100, 30_000);
    let max_hops = max_hops.clamp(1, 64);
    let ping_count = ping_count.clamp(1, 20);
    let total_start = Instant::now();

    let ping_host = normalized.host.clone();
    let ping_future = async move {
        let start = Instant::now();
        match ping::ping_host_cli(ping_host, ping_count, timeout_ms).await {
            Ok(rows) => {
                let success_count = rows.iter().filter(|row| row.success).count();
                let status = if success_count == rows.len() {
                    DiagnosticStatus::Passed
                } else if success_count > 0 {
                    DiagnosticStatus::Warning
                } else {
                    DiagnosticStatus::Failed
                };
                DiagnosticStep {
                    status,
                    duration_ms: elapsed_ms(start),
                    error: (success_count == 0).then(|| "所有 Ping 探测均失败".to_string()),
                    data: Some(rows),
                }
            }
            Err(error) => failed_step(start, error),
        }
    };

    let dns_host = normalized.host.clone();
    let dns_future = async move {
        let start = Instant::now();
        let rows = dns::dns_query(
            dns_host,
            "A".to_string(),
            vec![
                "system".to_string(),
                "1.1.1.1".to_string(),
                "8.8.8.8".to_string(),
            ],
            timeout_ms,
        )
        .await;
        let success_count = rows.iter().filter(|row| row.success).count();
        let status = if success_count == rows.len() {
            DiagnosticStatus::Passed
        } else if success_count > 0 {
            DiagnosticStatus::Warning
        } else {
            DiagnosticStatus::Failed
        };
        DiagnosticStep {
            status,
            duration_ms: elapsed_ms(start),
            error: (success_count == 0).then(|| "所有 DNS 查询均失败".to_string()),
            data: Some(rows),
        }
    };

    let tcp_host = normalized.host.clone();
    let tcp_port = normalized.port;
    let tcp_future = async move {
        let start = Instant::now();
        let result = tcp::tcp_probe(tcp_host, tcp_port, timeout_ms, false).await;
        DiagnosticStep {
            status: if result.success {
                DiagnosticStatus::Passed
            } else {
                DiagnosticStatus::Failed
            },
            duration_ms: elapsed_ms(start),
            error: result.error.clone(),
            data: Some(result),
        }
    };

    let http_url = normalized.url.clone();
    let uses_tls = normalized.scheme == "https";
    let http_future = async move {
        let start = Instant::now();
        let result =
            http::http_request(http_url, "GET".to_string(), vec![], None, timeout_ms, true).await;
        let status = http_status(&result, uses_tls);
        DiagnosticStep {
            status,
            duration_ms: elapsed_ms(start),
            error: result.error.clone(),
            data: Some(result),
        }
    };

    let trace_host = normalized.host.clone();
    let trace_future = async move {
        let start = Instant::now();
        if !include_trace {
            return DiagnosticStep {
                status: DiagnosticStatus::Skipped,
                duration_ms: 0.0,
                data: None,
                error: None,
            };
        }
        match traceroute::traceroute_collect(trace_host, max_hops, timeout_ms).await {
            Ok(rows) if !rows.is_empty() => DiagnosticStep {
                status: DiagnosticStatus::Passed,
                duration_ms: elapsed_ms(start),
                data: Some(rows),
                error: None,
            },
            Ok(rows) => DiagnosticStep {
                status: DiagnosticStatus::Failed,
                duration_ms: elapsed_ms(start),
                data: Some(rows),
                error: Some("未解析到任何路由跳点".to_string()),
            },
            Err(error) => failed_step(start, error),
        }
    };

    let (ping, dns, tcp, http, traceroute) = tokio::join!(
        ping_future,
        dns_future,
        tcp_future,
        http_future,
        trace_future
    );
    let summary = summarize([
        ping.status,
        dns.status,
        tcp.status,
        http.status,
        traceroute.status,
    ]);

    Ok(DiagnosticReport {
        target: target.trim().to_string(),
        host: normalized.host,
        url: normalized.url,
        port: normalized.port,
        generated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        total_ms: elapsed_ms(total_start),
        summary,
        ping,
        dns,
        tcp,
        http,
        traceroute,
    })
}

struct NormalizedTarget {
    host: String,
    url: String,
    scheme: String,
    port: u16,
}

fn normalize_target(target: &str) -> Result<NormalizedTarget, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("诊断目标不能为空".to_string());
    }

    let candidate = if target.contains("://") {
        target.to_string()
    } else {
        format!("https://{target}")
    };
    let url = Url::parse(&candidate).map_err(|error| format!("无法解析诊断目标: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("诊断报告目前只支持 HTTP/HTTPS URL 或主机名".to_string());
    }
    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| "诊断目标缺少主机名".to_string())?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "无法确定目标端口".to_string())?;

    Ok(NormalizedTarget {
        host,
        url: url.to_string(),
        scheme: url.scheme().to_string(),
        port,
    })
}

fn http_status(result: &http::HttpProbeResult, uses_tls: bool) -> DiagnosticStatus {
    if result.error.is_some() || result.status == 0 || result.status >= 500 {
        return DiagnosticStatus::Failed;
    }
    if result.status >= 400 {
        return DiagnosticStatus::Warning;
    }
    if uses_tls {
        match &result.tls {
            Some(tls) if tls.days_until_expiry < 0 => return DiagnosticStatus::Failed,
            Some(tls) if tls.days_until_expiry < 30 => return DiagnosticStatus::Warning,
            None => return DiagnosticStatus::Warning,
            _ => {}
        }
    }
    DiagnosticStatus::Passed
}

fn failed_step<T>(start: Instant, error: String) -> DiagnosticStep<T> {
    DiagnosticStep {
        status: DiagnosticStatus::Failed,
        duration_ms: elapsed_ms(start),
        data: None,
        error: Some(error),
    }
}

fn summarize<const N: usize>(statuses: [DiagnosticStatus; N]) -> DiagnosticSummary {
    let mut summary = DiagnosticSummary {
        status: DiagnosticStatus::Passed,
        passed: 0,
        warnings: 0,
        failed: 0,
        skipped: 0,
    };
    for status in statuses {
        match status {
            DiagnosticStatus::Passed => summary.passed += 1,
            DiagnosticStatus::Warning => summary.warnings += 1,
            DiagnosticStatus::Failed => summary.failed += 1,
            DiagnosticStatus::Skipped => summary.skipped += 1,
        }
    }
    summary.status = if summary.failed == N {
        DiagnosticStatus::Failed
    } else if summary.failed > 0 || summary.warnings > 0 {
        DiagnosticStatus::Warning
    } else {
        DiagnosticStatus::Passed
    };
    summary
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_host_to_https() {
        let target = normalize_target("example.com").unwrap();
        assert_eq!(target.host, "example.com");
        assert_eq!(target.url, "https://example.com/");
        assert_eq!(target.port, 443);
    }

    #[test]
    fn preserves_explicit_url_and_port() {
        let target = normalize_target("http://localhost:8080/health").unwrap();
        assert_eq!(target.host, "localhost");
        assert_eq!(target.url, "http://localhost:8080/health");
        assert_eq!(target.port, 8080);
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(normalize_target("ftp://example.com").is_err());
    }
}
