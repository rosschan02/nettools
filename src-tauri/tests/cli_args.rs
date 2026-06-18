use netools_lib::cli::{parse_cli_args, usage, CliCommand, OutputFormat};

#[test]
fn parses_lan_scan_and_ip_suggest() {
    let scan = parse_cli_args([
        "netools",
        "lan-scan",
        "192.168.10.0/24",
        "--timeout",
        "900",
        "--concurrency",
        "16",
        "--count",
        "5",
    ])
    .expect("lan scan args should parse");
    assert_eq!(
        scan.command,
        CliCommand::LanScan {
            cidr: Some("192.168.10.0/24".to_string()),
            timeout_ms: 900,
            concurrency: 16,
            suggestion_count: 5,
            suggest_only: false,
        }
    );

    let suggest =
        parse_cli_args(["netools", "ip-suggest", "--json"]).expect("ip suggest args should parse");
    assert_eq!(suggest.format, OutputFormat::Json);
    assert_eq!(
        suggest.command,
        CliCommand::LanScan {
            cidr: None,
            timeout_ms: 700,
            concurrency: 32,
            suggestion_count: 10,
            suggest_only: true,
        }
    );
}

#[test]
fn parses_diagnose_options() {
    let parsed = parse_cli_args([
        "netools",
        "diagnose",
        "https://example.com/health",
        "--no-trace",
        "--count",
        "6",
        "--timeout",
        "2500",
    ])
    .expect("diagnose args should parse");

    assert_eq!(
        parsed.command,
        CliCommand::Diagnose {
            target: "https://example.com/health".to_string(),
            include_trace: false,
            timeout_ms: 2500,
            max_hops: 20,
            ping_count: 6,
        }
    );
}

#[test]
fn parses_tcp_probe_defaults_and_json_flag() {
    let parsed = parse_cli_args([
        "netools",
        "tcp",
        "example.com",
        "443",
        "--timeout",
        "1500",
        "--json",
    ])
    .expect("tcp args should parse");

    assert_eq!(parsed.format, OutputFormat::Json);
    assert_eq!(
        parsed.command,
        CliCommand::TcpProbe {
            host: "example.com".to_string(),
            port: 443,
            timeout_ms: 1500,
            fingerprint: false,
        }
    );
}

#[test]
fn parses_scan_ports_and_concurrency() {
    let parsed = parse_cli_args([
        "netools",
        "scan",
        "127.0.0.1",
        "22,80,8000-8002",
        "--concurrency",
        "32",
        "--fingerprint",
    ])
    .expect("scan args should parse");

    assert_eq!(
        parsed.command,
        CliCommand::TcpScan {
            host: "127.0.0.1".to_string(),
            ports: vec![22, 80, 8000, 8001, 8002],
            timeout_ms: 1000,
            fingerprint: true,
            concurrency: 32,
        }
    );
}

#[test]
fn usage_documents_pure_cli_subcommands() {
    let text = usage();
    for needle in [
        "lan-info",
        "lan-scan",
        "ip-suggest",
        "diagnose",
        "ping",
        "tcp",
        "scan",
        "dns",
        "http",
        "trace",
    ] {
        assert!(text.contains(needle), "usage should mention {needle}");
    }
    assert!(text.contains("--json"));
}
