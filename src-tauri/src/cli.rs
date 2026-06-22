// 纯命令行入口。Linux 构建默认走这里，避免把 Tauri 图形界面带到服务器/ARM 设备上。
// 解析保持零额外依赖，避免为了 CLI 再引入 clap 之类的大依赖。

use crate::probe;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use tokio::runtime::Builder;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub command: CliCommand,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    LanInfo,
    LanScan {
        cidr: Option<String>,
        timeout_ms: u64,
        concurrency: usize,
        suggestion_count: usize,
        suggest_only: bool,
    },
    Diagnose {
        target: String,
        include_trace: bool,
        timeout_ms: u64,
        max_hops: u8,
        ping_count: u16,
    },
    Ping {
        host: String,
        count: u16,
        timeout_ms: u64,
    },
    TcpProbe {
        host: String,
        port: u16,
        timeout_ms: u64,
        fingerprint: bool,
    },
    TcpScan {
        host: String,
        ports: Vec<u16>,
        timeout_ms: u64,
        fingerprint: bool,
        concurrency: usize,
    },
    TcpPing {
        host: String,
        port: u16,
        count: u16,
        interval_ms: u64,
        timeout_ms: u64,
    },
    Dns {
        domain: String,
        record_type: String,
        servers: Vec<String>,
        timeout_ms: u64,
    },
    Http {
        url: String,
        method: String,
        headers: Vec<(String, String)>,
        body: Option<String>,
        timeout_ms: u64,
        follow_redirects: bool,
    },
    Trace {
        host: String,
        max_hops: u8,
        timeout_ms: u64,
    },
    Help,
}

pub fn main() -> i32 {
    match parse_cli_args(std::env::args()) {
        Ok(args) => {
            if args.command == CliCommand::Help {
                println!("{}", usage());
                return 0;
            }
            match run(args) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{}", usage());
            2
        }
    }
}

pub fn parse_cli_args<I, S>(args: I) -> Result<CliArgs, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut tokens: Vec<String> = args.into_iter().map(Into::into).collect();
    if !tokens.is_empty() {
        tokens.remove(0);
    }

    let mut format = OutputFormat::Human;
    tokens.retain(|t| {
        if t == "--json" {
            format = OutputFormat::Json;
            false
        } else {
            true
        }
    });

    if tokens.is_empty() || matches!(tokens[0].as_str(), "help" | "--help" | "-h") {
        return Ok(CliArgs {
            command: CliCommand::Help,
            format,
        });
    }

    let sub = tokens.remove(0);
    let command = match sub.as_str() {
        "lan-info" => parse_lan_info(tokens)?,
        "lan-scan" => parse_lan_scan(tokens, false)?,
        "ip-suggest" => parse_lan_scan(tokens, true)?,
        "diagnose" | "doctor" => parse_diagnose(tokens)?,
        "ping" => parse_ping(tokens)?,
        "tcp" => parse_tcp(tokens)?,
        "scan" => parse_scan(tokens)?,
        "tcp-ping" => parse_tcp_ping(tokens)?,
        "dns" => parse_dns(tokens)?,
        "http" => parse_http(tokens)?,
        "trace" | "traceroute" => parse_trace(tokens)?,
        other => return Err(format!("未知子命令: {other}")),
    };

    Ok(CliArgs { command, format })
}

pub fn usage() -> String {
    r#"netools - pure CLI network diagnostics

USAGE:
  netools lan-info [--json]
  netools lan-scan [cidr] [--timeout MS] [--concurrency N] [--count N] [--json]
  netools ip-suggest [cidr] [--timeout MS] [--concurrency N] [--count N] [--json]
  netools diagnose <host-or-url> [--no-trace] [--count N] [--max-hops N] [--timeout MS] [--json]
  netools ping <host> [--count N] [--timeout MS] [--json]
  netools tcp <host> <port> [--timeout MS] [--fingerprint] [--json]
  netools scan <host> <ports> [--timeout MS] [--concurrency N] [--fingerprint] [--json]
  netools tcp-ping <host> <port> [--count N] [--interval MS] [--timeout MS] [--json]
  netools dns <domain> [--type A] [--server IP ...] [--timeout MS] [--json]
  netools http <url> [--method GET] [--header 'K: V'] [--body TEXT] [--no-follow] [--timeout MS] [--json]
  netools trace <host> [--max-hops N] [--timeout MS] [--json]

PORTS:
  scan accepts comma/range syntax, e.g. 22,80,443,8000-8010
"#
    .to_string()
}

fn parse_lan_info(tokens: Vec<String>) -> Result<CliCommand, String> {
    if tokens.is_empty() {
        Ok(CliCommand::LanInfo)
    } else {
        Err("lan-info 不接受位置参数".to_string())
    }
}

fn parse_lan_scan(tokens: Vec<String>, suggest_only: bool) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    if pos.len() > 1 {
        return Err("用法: lan-scan [cidr]".to_string());
    }
    Ok(CliCommand::LanScan {
        cidr: pos.first().cloned(),
        timeout_ms: opt_u64(&opts, "--timeout", 700)?,
        concurrency: opt_usize(&opts, "--concurrency", 32)?,
        suggestion_count: opt_usize(&opts, "--count", 10)?,
        suggest_only,
    })
}

fn parse_diagnose(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    let target = one_pos(&pos, "diagnose <host-or-url>")?;
    Ok(CliCommand::Diagnose {
        target,
        include_trace: !has_flag(&opts, "--no-trace"),
        timeout_ms: opt_u64(&opts, "--timeout", 2000)?,
        max_hops: opt_u8(&opts, "--max-hops", 20)?,
        ping_count: opt_u16(&opts, "--count", 4)?,
    })
}

fn parse_ping(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    let host = one_pos(&pos, "ping <host>")?;
    Ok(CliCommand::Ping {
        host,
        count: opt_u16(&opts, "--count", 4)?,
        timeout_ms: opt_u64(&opts, "--timeout", 1000)?,
    })
}

fn parse_tcp(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    if pos.len() != 2 {
        return Err("tcp 用法: tcp <host> <port>".into());
    }
    Ok(CliCommand::TcpProbe {
        host: pos[0].clone(),
        port: parse_port(&pos[1])?,
        timeout_ms: opt_u64(&opts, "--timeout", 1000)?,
        fingerprint: has_flag(&opts, "--fingerprint"),
    })
}

fn parse_scan(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    if pos.len() != 2 {
        return Err("scan 用法: scan <host> <ports>".into());
    }
    Ok(CliCommand::TcpScan {
        host: pos[0].clone(),
        ports: parse_ports(&pos[1])?,
        timeout_ms: opt_u64(&opts, "--timeout", 1000)?,
        fingerprint: has_flag(&opts, "--fingerprint"),
        concurrency: opt_usize(&opts, "--concurrency", 128)?,
    })
}

fn parse_tcp_ping(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    if pos.len() != 2 {
        return Err("tcp-ping 用法: tcp-ping <host> <port>".into());
    }
    Ok(CliCommand::TcpPing {
        host: pos[0].clone(),
        port: parse_port(&pos[1])?,
        count: opt_u16(&opts, "--count", 4)?,
        interval_ms: opt_u64(&opts, "--interval", 1000)?,
        timeout_ms: opt_u64(&opts, "--timeout", 1000)?,
    })
}

fn parse_dns(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    let domain = one_pos(&pos, "dns <domain>")?;
    Ok(CliCommand::Dns {
        domain,
        record_type: opt_string(&opts, "--type", "A"),
        servers: opt_strings(&opts, "--server"),
        timeout_ms: opt_u64(&opts, "--timeout", 2000)?,
    })
}

fn parse_http(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    let url = one_pos(&pos, "http <url>")?;
    let headers = opt_strings(&opts, "--header")
        .into_iter()
        .map(|h| parse_header(&h))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CliCommand::Http {
        url,
        method: opt_string(&opts, "--method", "GET"),
        headers,
        body: opt_optional_string(&opts, "--body"),
        timeout_ms: opt_u64(&opts, "--timeout", 5000)?,
        follow_redirects: !has_flag(&opts, "--no-follow"),
    })
}

fn parse_trace(tokens: Vec<String>) -> Result<CliCommand, String> {
    let (pos, opts) = split_options(tokens);
    let host = one_pos(&pos, "trace <host>")?;
    Ok(CliCommand::Trace {
        host,
        max_hops: opt_u8(&opts, "--max-hops", 30)?,
        timeout_ms: opt_u64(&opts, "--timeout", 3000)?,
    })
}

fn run(args: CliArgs) -> Result<(), String> {
    let rt = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("创建 tokio runtime 失败: {e}"))?;

    rt.block_on(async move {
        match args.command {
            CliCommand::LanInfo => {
                let r = probe::lan::lan_info().await?;
                print_output(&args.format, &r, format_lan_info(&r))
            }
            CliCommand::LanScan {
                cidr,
                timeout_ms,
                concurrency,
                suggestion_count,
                suggest_only,
            } => {
                let r = probe::lan::lan_scan(cidr, timeout_ms, concurrency, suggestion_count, None)
                    .await?;
                print_output(&args.format, &r, format_lan_scan(&r, suggest_only))
            }
            CliCommand::Diagnose {
                target,
                include_trace,
                timeout_ms,
                max_hops,
                ping_count,
            } => {
                let r = probe::diagnostic::diagnose(
                    target,
                    include_trace,
                    timeout_ms,
                    max_hops,
                    ping_count,
                )
                .await?;
                print_output(&args.format, &r, format_diagnostic(&r))
            }
            CliCommand::Ping {
                host,
                count,
                timeout_ms,
            } => {
                let r = probe::ping::ping_host_cli(host, count, timeout_ms).await?;
                print_output(&args.format, &r, format_ping(&r))
            }
            CliCommand::TcpProbe {
                host,
                port,
                timeout_ms,
                fingerprint,
            } => {
                let r = probe::tcp::tcp_probe(host, port, timeout_ms, fingerprint).await;
                print_output(&args.format, &r, format_tcp_probe(&r))
            }
            CliCommand::TcpScan {
                host,
                ports,
                timeout_ms,
                fingerprint,
                concurrency,
            } => {
                let r =
                    probe::tcp::tcp_scan(host, ports, timeout_ms, fingerprint, concurrency).await;
                print_output(&args.format, &r, format_tcp_scan(&r))
            }
            CliCommand::TcpPing {
                host,
                port,
                count,
                interval_ms,
                timeout_ms,
            } => {
                let r = probe::tcp::tcp_ping_cli(host, port, count, interval_ms, timeout_ms).await;
                print_output(&args.format, &r, format_tcp_scan(&r))
            }
            CliCommand::Dns {
                domain,
                record_type,
                servers,
                timeout_ms,
            } => {
                let r = probe::dns::dns_query(domain, record_type, servers, timeout_ms).await;
                print_output(&args.format, &r, format_dns(&r))
            }
            CliCommand::Http {
                url,
                method,
                headers,
                body,
                timeout_ms,
                follow_redirects,
            } => {
                let r = probe::http::http_request(
                    url,
                    method,
                    headers,
                    body,
                    timeout_ms,
                    follow_redirects,
                )
                .await;
                print_output(&args.format, &r, format_http(&r))
            }
            CliCommand::Trace {
                host,
                max_hops,
                timeout_ms,
            } => {
                let r = probe::traceroute::traceroute_collect(host, max_hops, timeout_ms).await?;
                print_output(&args.format, &r, format_trace(&r))
            }
            CliCommand::Help => {
                println!("{}", usage());
                Ok(())
            }
        }
    })
}

fn print_output<T: Serialize>(
    format: &OutputFormat,
    value: &T,
    human: String,
) -> Result<(), String> {
    match format {
        OutputFormat::Human => println!("{human}"),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
        ),
    }
    Ok(())
}

fn format_ping(rows: &[probe::ping::PingResult]) -> String {
    let mut out = String::new();
    for r in rows {
        if r.success {
            let _ = writeln!(out, "seq={} from={} rtt={:.2}ms", r.seq, r.from, r.rtt_ms);
        } else {
            let _ = writeln!(
                out,
                "seq={} failed: {}",
                r.seq,
                r.error.as_deref().unwrap_or("unknown")
            );
        }
    }
    out.trim_end().to_string()
}

fn format_lan_info(info: &probe::lan::LanInfo) -> String {
    format!(
        "interface: {}\naddress: {}\nnetmask: {} (/{})\nsubnet: {}\nsuggested_scan: {}\nbroadcast: {}\ngateway: {}\nmac: {}",
        info.interface_name,
        info.address,
        info.netmask,
        info.prefix,
        info.cidr,
        info.suggested_cidr,
        info.broadcast,
        info.gateway.as_deref().unwrap_or("unknown"),
        info.mac.as_deref().unwrap_or("unknown"),
    )
}

fn format_lan_scan(report: &probe::lan::LanScanReport, suggest_only: bool) -> String {
    let mut out = String::new();
    if !suggest_only {
        let _ = writeln!(
            out,
            "scan {} via {} ({} hosts, {} active, {:.0}ms)",
            report.cidr,
            report.info.interface_name,
            report.scanned_hosts,
            report.active_hosts.len(),
            report.duration_ms,
        );
        for host in &report.active_hosts {
            let _ = writeln!(
                out,
                "{:<15} {:<17} via={}{}{}",
                host.ip,
                host.mac.as_deref().unwrap_or("-"),
                host.methods.join(","),
                if host.open_ports.is_empty() {
                    String::new()
                } else {
                    format!(" ports={:?}", host.open_ports)
                },
                host.reserved_reason
                    .as_ref()
                    .map(|reason| format!(" [{reason}]"))
                    .unwrap_or_default(),
            );
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out, "疑似空闲候选:");
    for ip in &report.candidates {
        let _ = writeln!(out, "  {ip}");
    }
    let _ = write!(out, "\nwarning: {}", report.warning);
    out
}

fn format_diagnostic(report: &probe::diagnostic::DiagnosticReport) -> String {
    let mut out = format!(
        "diagnostic report for {} ({}:{})\noverall: {} | passed={} warnings={} failed={} skipped={} | {:.0}ms",
        report.host,
        report.host,
        report.port,
        diagnostic_status(report.summary.status),
        report.summary.passed,
        report.summary.warnings,
        report.summary.failed,
        report.summary.skipped,
        report.total_ms,
    );

    let ping_detail = report
        .ping
        .data
        .as_ref()
        .map(|rows| {
            format!(
                "{}/{} replies",
                rows.iter().filter(|r| r.success).count(),
                rows.len()
            )
        })
        .unwrap_or_else(|| report.ping.error.clone().unwrap_or_default());
    let dns_detail = report
        .dns
        .data
        .as_ref()
        .map(|rows| {
            format!(
                "{}/{} resolvers",
                rows.iter().filter(|r| r.success).count(),
                rows.len()
            )
        })
        .unwrap_or_else(|| report.dns.error.clone().unwrap_or_default());
    let tcp_detail = report
        .tcp
        .data
        .as_ref()
        .map(|result| if result.success { "open" } else { "closed" }.to_string())
        .unwrap_or_else(|| report.tcp.error.clone().unwrap_or_default());
    let http_detail = report
        .http
        .data
        .as_ref()
        .map(|result| format!("HTTP {} {:.0}ms", result.status, result.total_ms))
        .unwrap_or_else(|| report.http.error.clone().unwrap_or_default());
    let trace_detail = report
        .traceroute
        .data
        .as_ref()
        .map(|rows| format!("{} hops", rows.len()))
        .unwrap_or_else(|| {
            report
                .traceroute
                .error
                .clone()
                .unwrap_or_else(|| "disabled".to_string())
        });

    for (name, status, detail) in [
        ("ping", report.ping.status, ping_detail),
        ("dns", report.dns.status, dns_detail),
        ("tcp", report.tcp.status, tcp_detail),
        ("http/tls", report.http.status, http_detail),
        ("traceroute", report.traceroute.status, trace_detail),
    ] {
        let _ = write!(
            out,
            "\n{:<10} {:<7} {}",
            name,
            diagnostic_status(status),
            detail
        );
    }
    out
}

fn diagnostic_status(status: probe::diagnostic::DiagnosticStatus) -> &'static str {
    match status {
        probe::diagnostic::DiagnosticStatus::Passed => "passed",
        probe::diagnostic::DiagnosticStatus::Warning => "warning",
        probe::diagnostic::DiagnosticStatus::Failed => "failed",
        probe::diagnostic::DiagnosticStatus::Skipped => "skipped",
    }
}

fn format_tcp_probe(r: &probe::tcp::TcpProbeResult) -> String {
    format!(
        "{}:{} {} rtt={:.2}ms{}{}",
        r.host,
        r.port,
        if r.success { "open" } else { "closed" },
        r.rtt_ms,
        r.service
            .as_ref()
            .map(|s| format!(" service={s}"))
            .unwrap_or_default(),
        r.error
            .as_ref()
            .map(|e| format!(" error={e}"))
            .unwrap_or_default()
    )
}

fn format_tcp_scan(rows: &[probe::tcp::TcpProbeResult]) -> String {
    rows.iter()
        .map(format_tcp_probe)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_dns(rows: &[probe::dns::DnsServerResult]) -> String {
    let mut out = String::new();
    for r in rows {
        if r.success {
            let _ = writeln!(
                out,
                "{} {:.2}ms {}",
                r.server,
                r.rtt_ms,
                r.records.join(", ")
            );
        } else {
            let _ = writeln!(
                out,
                "{} failed: {}",
                r.server,
                r.error.as_deref().unwrap_or("unknown")
            );
        }
    }
    out.trim_end().to_string()
}

fn format_http(r: &probe::http::HttpProbeResult) -> String {
    let mut out = format!(
        "{} {} {:.2}ms {} bytes\nfinal_url: {}",
        r.status, r.status_text, r.total_ms, r.body_size_bytes, r.final_url
    );
    if let Some(e) = &r.error {
        let _ = write!(out, "\nerror: {e}");
    }
    if let Some(tls) = &r.tls {
        let _ = write!(
            out,
            "\ntls_subject: {}\ntls_expires_in_days: {}",
            tls.subject, tls.days_until_expiry
        );
    }
    if !r.body_preview.is_empty() {
        let _ = write!(out, "\n\n{}", r.body_preview);
    }
    out
}

fn format_trace(rows: &[probe::traceroute::TraceHop]) -> String {
    let mut out = String::new();
    for h in rows {
        let rtts = h
            .rtts
            .iter()
            .map(|r| {
                r.map(|v| format!("{v:.2}ms"))
                    .unwrap_or_else(|| "*".to_string())
            })
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "{:>2} {:<20} {}",
            h.ttl,
            h.addr.as_deref().unwrap_or("*"),
            rtts
        );
    }
    out.trim_end().to_string()
}

fn split_options(tokens: Vec<String>) -> (Vec<String>, Vec<(String, Option<String>)>) {
    let mut pos = Vec::new();
    let mut opts = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.starts_with("--") {
            if i + 1 < tokens.len() && !tokens[i + 1].starts_with("--") {
                opts.push((t.clone(), Some(tokens[i + 1].clone())));
                i += 2;
            } else {
                opts.push((t.clone(), None));
                i += 1;
            }
        } else {
            pos.push(t.clone());
            i += 1;
        }
    }
    (pos, opts)
}

fn one_pos(pos: &[String], usage: &str) -> Result<String, String> {
    if pos.len() == 1 {
        Ok(pos[0].clone())
    } else {
        Err(format!("用法: {usage}"))
    }
}

fn has_flag(opts: &[(String, Option<String>)], name: &str) -> bool {
    opts.iter().any(|(k, _)| k == name)
}

fn opt_optional_string(opts: &[(String, Option<String>)], name: &str) -> Option<String> {
    opts.iter()
        .find(|(k, _)| k == name)
        .and_then(|(_, v)| v.clone())
}

fn opt_string(opts: &[(String, Option<String>)], name: &str, default: &str) -> String {
    opt_optional_string(opts, name).unwrap_or_else(|| default.to_string())
}

fn opt_strings(opts: &[(String, Option<String>)], name: &str) -> Vec<String> {
    opts.iter()
        .filter(|(k, _)| k == name)
        .filter_map(|(_, v)| v.clone())
        .collect()
}

fn opt_u64(opts: &[(String, Option<String>)], name: &str, default: u64) -> Result<u64, String> {
    opt_optional_string(opts, name)
        .map(|v| v.parse().map_err(|_| format!("{name} 需要整数")))
        .unwrap_or(Ok(default))
}

fn opt_u16(opts: &[(String, Option<String>)], name: &str, default: u16) -> Result<u16, String> {
    opt_optional_string(opts, name)
        .map(|v| v.parse().map_err(|_| format!("{name} 需要 0-65535 整数")))
        .unwrap_or(Ok(default))
}

fn opt_u8(opts: &[(String, Option<String>)], name: &str, default: u8) -> Result<u8, String> {
    opt_optional_string(opts, name)
        .map(|v| v.parse().map_err(|_| format!("{name} 需要 0-255 整数")))
        .unwrap_or(Ok(default))
}

fn opt_usize(
    opts: &[(String, Option<String>)],
    name: &str,
    default: usize,
) -> Result<usize, String> {
    opt_optional_string(opts, name)
        .map(|v| v.parse().map_err(|_| format!("{name} 需要整数")))
        .unwrap_or(Ok(default))
}

fn parse_port(s: &str) -> Result<u16, String> {
    let p: u16 = s.parse().map_err(|_| format!("非法端口: {s}"))?;
    if p == 0 {
        Err("端口不能为 0".into())
    } else {
        Ok(p)
    }
}

fn parse_ports(spec: &str) -> Result<Vec<u16>, String> {
    let mut set = BTreeSet::new();
    for part in spec.split(',').filter(|p| !p.trim().is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let start = parse_port(a.trim())?;
            let end = parse_port(b.trim())?;
            if start > end {
                return Err(format!("非法端口范围: {part}"));
            }
            for p in start..=end {
                set.insert(p);
            }
        } else {
            set.insert(parse_port(part.trim())?);
        }
    }
    if set.is_empty() {
        return Err("端口列表不能为空".into());
    }
    Ok(set.into_iter().collect())
}

fn parse_header(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once(':')
        .ok_or_else(|| format!("header 需要 'K: V' 格式: {s}"))?;
    let key = k.trim();
    if key.is_empty() {
        return Err("header 名不能为空".into());
    }
    Ok((key.to_string(), v.trim().to_string()))
}
