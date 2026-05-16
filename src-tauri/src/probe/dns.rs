// DNS 查询模块。一次能并发问多个上游 DNS 服务器并比较响应。
//
// API 适配 hickory-resolver 0.26：
//   - 自定义服务器: NameServerConfig::udp_and_tcp(ip) -> ResolverConfig::from_parts -> Resolver::builder_with_config
//   - 系统服务器:   TokioResolver::builder_tokio()
//   - 查询:        resolver.lookup(name, record_type).await -> Lookup
//   - 取记录值:    lookup.answers() -> &[Record] -> record.data() -> &RData (Display)

use futures::future;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::RecordType;
use serde::Serialize;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize, Clone)]
pub struct DnsServerResult {
    pub server: String,
    pub rtt_ms: f64,
    pub success: bool,
    pub records: Vec<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn dns_query(
    domain: String,
    record_type: String,
    servers: Vec<String>,
    timeout_ms: u64,
) -> Vec<DnsServerResult> {
    let record_type = parse_record_type(&record_type);
    let timeout = Duration::from_millis(timeout_ms);

    // 空列表 = 走系统默认 DNS
    let server_list: Vec<String> = if servers.is_empty() {
        vec!["system".to_string()]
    } else {
        servers
    };

    let futures = server_list.into_iter().map(|s| {
        let d = domain.clone();
        async move { query_one(&d, record_type, &s, timeout).await }
    });

    future::join_all(futures).await
}

async fn query_one(
    domain: &str,
    record_type: RecordType,
    server: &str,
    timeout: Duration,
) -> DnsServerResult {
    let resolver = match build_resolver(server, timeout) {
        Ok(r) => r,
        Err(e) => {
            return DnsServerResult {
                server: server.to_string(),
                rtt_ms: 0.0,
                success: false,
                records: vec![],
                error: Some(e),
            };
        }
    };

    let start = Instant::now();
    let result = resolver.lookup(domain, record_type).await;
    let rtt_ms = start.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(lookup) => {
            let records: Vec<String> = lookup
                .answers()
                .iter()
                .map(|rec| rec.data.to_string())
                .collect();
            let success = !records.is_empty();
            DnsServerResult {
                server: server.to_string(),
                rtt_ms,
                success,
                error: if success { None } else { Some("无返回记录".into()) },
                records,
            }
        }
        Err(e) => DnsServerResult {
            server: server.to_string(),
            rtt_ms,
            success: false,
            records: vec![],
            error: Some(e.to_string()),
        },
    }
}

fn build_resolver(server: &str, timeout: Duration) -> Result<TokioResolver, String> {
    let mut opts = ResolverOpts::default();
    opts.timeout = timeout;
    opts.attempts = 1;

    if server == "system" {
        return TokioResolver::builder_tokio()
            .map_err(|e| format!("读取系统 DNS 配置失败: {}", e))?
            .with_options(opts)
            .build()
            .map_err(|e| format!("构建系统 resolver 失败: {}", e));
    }

    let ip = IpAddr::from_str(server)
        .map_err(|_| format!("非法 DNS 服务器 IP: {}", server))?;
    let nsc = NameServerConfig::udp_and_tcp(ip);
    let config = ResolverConfig::from_parts(None, vec![], vec![nsc]);
    TokioResolver::builder_with_config(config, TokioRuntimeProvider::default())
        .with_options(opts)
        .build()
        .map_err(|e| format!("构建 resolver 失败: {}", e))
}

fn parse_record_type(s: &str) -> RecordType {
    match s.to_ascii_uppercase().as_str() {
        "A" => RecordType::A,
        "AAAA" => RecordType::AAAA,
        "CNAME" => RecordType::CNAME,
        "MX" => RecordType::MX,
        "NS" => RecordType::NS,
        "TXT" => RecordType::TXT,
        "SOA" => RecordType::SOA,
        "SRV" => RecordType::SRV,
        "PTR" => RecordType::PTR,
        "CAA" => RecordType::CAA,
        _ => RecordType::A,
    }
}
