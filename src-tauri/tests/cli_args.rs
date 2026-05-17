use netools_lib::cli::{parse_cli_args, usage, CliCommand, OutputFormat};

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
    for needle in ["ping", "tcp", "scan", "dns", "http", "trace"] {
        assert!(text.contains(needle), "usage should mention {needle}");
    }
    assert!(text.contains("--json"));
}
