import { ReactNode, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import AddressInput from "../components/AddressInput";
import { useAddressHistory } from "../utils/history";

type DiagnosticStatus = "passed" | "warning" | "failed" | "skipped";

type DiagnosticStep<T> = {
  status: DiagnosticStatus;
  duration_ms: number;
  data: T | null;
  error: string | null;
};

type PingResult = {
  seq: number;
  rtt_ms: number;
  from: string;
  success: boolean;
  error: string | null;
};

type DnsServerResult = {
  server: string;
  rtt_ms: number;
  success: boolean;
  records: string[];
  error: string | null;
};

type TcpProbeResult = {
  host: string;
  port: number;
  success: boolean;
  rtt_ms: number;
  error: string | null;
};

type TlsCertInfo = {
  subject: string;
  issuer: string;
  not_after: string;
  days_until_expiry: number;
};

type HttpProbeResult = {
  success: boolean;
  status: number;
  status_text: string;
  final_url: string;
  redirects: string[];
  body_size_bytes: number;
  total_ms: number;
  error: string | null;
  tls: TlsCertInfo | null;
};

type TraceHop = {
  ttl: number;
  addr: string | null;
  rtts: (number | null)[];
};

type DiagnosticReport = {
  target: string;
  host: string;
  url: string;
  port: number;
  generated_at_unix_ms: number;
  total_ms: number;
  summary: {
    status: DiagnosticStatus;
    passed: number;
    warnings: number;
    failed: number;
    skipped: number;
  };
  ping: DiagnosticStep<PingResult[]>;
  dns: DiagnosticStep<DnsServerResult[]>;
  tcp: DiagnosticStep<TcpProbeResult>;
  http: DiagnosticStep<HttpProbeResult>;
  traceroute: DiagnosticStep<TraceHop[]>;
};

const STATUS_LABELS: Record<DiagnosticStatus, string> = {
  passed: "正常",
  warning: "注意",
  failed: "失败",
  skipped: "跳过",
};

function formatMs(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(2)} s` : `${value.toFixed(0)} ms`;
}

function average(values: number[]): number | null {
  if (values.length === 0) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function markdownCell(value: string): string {
  return value.split("|").join("\\|").split("\n").join(" ");
}

function reportToMarkdown(report: DiagnosticReport): string {
  const pingRows = report.ping.data ?? [];
  const successfulPings = pingRows.filter((row) => row.success);
  const pingAverage = average(successfulPings.map((row) => row.rtt_ms));
  const dnsRows = report.dns.data ?? [];
  const tcp = report.tcp.data;
  const http = report.http.data;
  const trace = report.traceroute.data ?? [];
  const lines = [
    "# netools 诊断报告",
    "",
    `- 目标：${report.target}`,
    `- 解析地址：${report.host}:${report.port}`,
    `- URL：${report.url}`,
    `- 生成时间：${new Date(report.generated_at_unix_ms).toLocaleString()}`,
    `- 总耗时：${formatMs(report.total_ms)}`,
    "",
    "## 概览",
    "",
    "| 检查 | 状态 | 详情 |",
    "| --- | --- | --- |",
    `| Ping | ${STATUS_LABELS[report.ping.status]} | ${successfulPings.length}/${pingRows.length} 成功${pingAverage == null ? "" : `，平均 ${pingAverage.toFixed(2)} ms`} |`,
    `| DNS | ${STATUS_LABELS[report.dns.status]} | ${dnsRows.filter((row) => row.success).length}/${dnsRows.length} 成功 |`,
    `| TCP | ${STATUS_LABELS[report.tcp.status]} | ${tcp ? `${tcp.host}:${tcp.port} ${tcp.success ? "开放" : "不可达"}` : report.tcp.error ?? "无结果"} |`,
    `| HTTP/TLS | ${STATUS_LABELS[report.http.status]} | ${http ? `HTTP ${http.status || "失败"}，${formatMs(http.total_ms)}` : report.http.error ?? "无结果"} |`,
    `| Traceroute | ${STATUS_LABELS[report.traceroute.status]} | ${trace.length} 跳 |`,
    "",
    "## DNS 结果",
    "",
    "| 服务器 | 状态 | RTT | 记录 |",
    "| --- | --- | --- | --- |",
    ...dnsRows.map(
      (row) =>
        `| ${markdownCell(row.server)} | ${row.success ? "正常" : "失败"} | ${row.rtt_ms.toFixed(1)} ms | ${markdownCell(row.records.join(", ") || row.error || "-")} |`,
    ),
  ];

  if (http?.tls) {
    lines.push(
      "",
      "## TLS 证书",
      "",
      `- Subject：${http.tls.subject}`,
      `- Issuer：${http.tls.issuer}`,
      `- 到期：${http.tls.not_after}（剩余 ${http.tls.days_until_expiry} 天）`,
    );
  }
  if (trace.length > 0) {
    lines.push(
      "",
      "## 路由",
      "",
      "| TTL | 地址 | RTT |",
      "| --- | --- | --- |",
      ...trace.map(
        (hop) =>
          `| ${hop.ttl} | ${markdownCell(hop.addr ?? "*")} | ${hop.rtts.map((rtt) => (rtt == null ? "*" : `${rtt.toFixed(1)} ms`)).join(", ")} |`,
      ),
    );
  }
  return lines.join("\n");
}

function StatusBadge({ status }: { status: DiagnosticStatus }) {
  return <span className={`diagnostic-badge ${status}`}>{STATUS_LABELS[status]}</span>;
}

function DiagnosticSection({
  title,
  step,
  summary,
  children,
}: {
  title: string;
  step: DiagnosticStep<unknown>;
  summary: string;
  children?: ReactNode;
}) {
  return (
    <details className={`diagnostic-section ${step.status}`} open={step.status !== "passed"}>
      <summary>
        <span className="diagnostic-section-title">{title}</span>
        <span className="diagnostic-section-summary">{summary}</span>
        <StatusBadge status={step.status} />
        <span className="diagnostic-duration">{formatMs(step.duration_ms)}</span>
      </summary>
      <div className="diagnostic-section-body">
        {step.error ? <div className="diagnostic-inline-error">{step.error}</div> : null}
        {children}
      </div>
    </details>
  );
}

export default function DiagnosticPanel() {
  const [target, setTarget] = useState("github.com");
  const [includeTrace, setIncludeTrace] = useState(true);
  const [timeoutMs, setTimeoutMs] = useState(2000);
  const [maxHops, setMaxHops] = useState(20);
  const [pingCount, setPingCount] = useState(4);
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copyLabel, setCopyLabel] = useState("复制 Markdown");
  const targetHistory = useAddressHistory("diagnostic.target");

  async function run() {
    targetHistory.add(target);
    setRunning(true);
    setError(null);
    setReport(null);
    setCopyLabel("复制 Markdown");
    try {
      const result = await invoke<DiagnosticReport>("diagnose", {
        target: target.trim(),
        includeTrace,
        timeoutMs,
        maxHops,
        pingCount,
      });
      setReport(result);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setRunning(false);
    }
  }

  async function copyMarkdown() {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(reportToMarkdown(report));
      setCopyLabel("已复制");
    } catch (caught) {
      setError(`复制失败: ${String(caught)}`);
    }
  }

  function exportJson() {
    if (!report) return;
    const blob = new Blob([JSON.stringify(report, null, 2)], { type: "application/json" });
    const href = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    const safeHost = report.host.replace(/[^a-z0-9.-]/gi, "_");
    anchor.href = href;
    anchor.download = `netools-${safeHost}-${new Date(report.generated_at_unix_ms).toISOString().slice(0, 10)}.json`;
    anchor.click();
    URL.revokeObjectURL(href);
  }

  const pingRows = report?.ping.data ?? [];
  const successfulPings = pingRows.filter((row) => row.success);
  const pingAverage = average(successfulPings.map((row) => row.rtt_ms));
  const dnsRows = report?.dns.data ?? [];
  const tcp = report?.tcp.data;
  const http = report?.http.data;
  const traceRows = report?.traceroute.data ?? [];

  return (
    <div className="diagnostic-panel">
      <div className="diagnostic-heading">
        <div>
          <h2>诊断报告</h2>
          <p className="subtitle">一次检查 DNS、Ping、TCP、HTTP/TLS 和路由路径</p>
        </div>
        {report ? (
          <div className="diagnostic-actions">
            <button type="button" className="secondary-button" onClick={copyMarkdown}>
              {copyLabel}
            </button>
            <button type="button" className="secondary-button" onClick={exportJson}>
              导出 JSON
            </button>
          </div>
        ) : null}
      </div>

      <form
        className="diagnostic-form"
        onSubmit={(event) => {
          event.preventDefault();
          void run();
        }}
      >
        <div className="diagnostic-target-row">
          <AddressInput
            value={target}
            onChange={setTarget}
            history={targetHistory}
            placeholder="域名或 URL，例如 github.com"
            ariaLabel="诊断目标"
            required
            containerStyle={{ flex: 1, minWidth: 220 }}
            style={{ padding: "9px 12px", background: "#171b1f" }}
          />
          <button type="submit" disabled={running}>
            {running ? "并行诊断中…" : "开始诊断"}
          </button>
        </div>
        <div className="diagnostic-options">
          <label>
            <input
              type="checkbox"
              checked={includeTrace}
              onChange={(event) => setIncludeTrace(event.currentTarget.checked)}
            />
            包含路由追踪
          </label>
          <label>
            Ping 次数
            <input
              type="number"
              value={pingCount}
              onChange={(event) => setPingCount(Number(event.currentTarget.value))}
              min={1}
              max={20}
            />
          </label>
          <label>
            单项超时
            <input
              type="number"
              value={timeoutMs}
              onChange={(event) => setTimeoutMs(Number(event.currentTarget.value))}
              min={100}
              max={30000}
              step={100}
            />
            ms
          </label>
          {includeTrace ? (
            <label>
              最大跳数
              <input
                type="number"
                value={maxHops}
                onChange={(event) => setMaxHops(Number(event.currentTarget.value))}
                min={1}
                max={64}
              />
            </label>
          ) : null}
        </div>
      </form>

      {running ? (
        <div className="diagnostic-running" role="status">
          <span className="diagnostic-spinner" />
          五项检查正在并行执行，路由追踪通常最后完成
        </div>
      ) : null}
      {error ? <div className="error-box">{error}</div> : null}

      {report ? (
        <div className="diagnostic-report">
          <section className={`diagnostic-overview ${report.summary.status}`}>
            <div>
              <span className="diagnostic-eyebrow">总体结果</span>
              <div className="diagnostic-overall-title">
                <StatusBadge status={report.summary.status} />
                <strong>{report.host}</strong>
              </div>
              <div className="diagnostic-meta">
                {report.url} · TCP {report.port} · {formatMs(report.total_ms)}
              </div>
            </div>
            <div className="diagnostic-counts">
              <span><b>{report.summary.passed}</b> 正常</span>
              <span><b>{report.summary.warnings}</b> 注意</span>
              <span><b>{report.summary.failed}</b> 失败</span>
              <span><b>{report.summary.skipped}</b> 跳过</span>
            </div>
          </section>

          <div className="diagnostic-sections">
            <DiagnosticSection
              title="Ping"
              step={report.ping}
              summary={`${successfulPings.length}/${pingRows.length} 成功${pingAverage == null ? "" : ` · 平均 ${pingAverage.toFixed(2)} ms`}`}
            >
              {pingRows.length > 0 ? (
                <table>
                  <thead><tr><th>Seq</th><th>From</th><th>RTT</th><th>状态</th></tr></thead>
                  <tbody>
                    {pingRows.map((row) => (
                      <tr key={row.seq}>
                        <td>{row.seq}</td><td>{row.from || "-"}</td>
                        <td>{row.success ? `${row.rtt_ms.toFixed(2)} ms` : "-"}</td>
                        <td className={row.success ? "status-ok" : "status-fail"}>{row.success ? "OK" : row.error ?? "FAIL"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : null}
            </DiagnosticSection>

            <DiagnosticSection
              title="DNS"
              step={report.dns}
              summary={`${dnsRows.filter((row) => row.success).length}/${dnsRows.length} 个解析器成功`}
            >
              {dnsRows.map((row) => (
                <div className="diagnostic-dns-row" key={row.server}>
                  <span>{row.server}</span>
                  <span className={row.success ? "status-ok" : "status-fail"}>{row.success ? `${row.rtt_ms.toFixed(1)} ms` : row.error}</span>
                  <code>{row.records.join(", ") || "-"}</code>
                </div>
              ))}
            </DiagnosticSection>

            <DiagnosticSection
              title="TCP"
              step={report.tcp}
              summary={tcp ? `${tcp.host}:${tcp.port} · ${tcp.success ? "端口开放" : "连接失败"}` : "无结果"}
            >
              {tcp ? <p className="diagnostic-detail-line">握手耗时 <b>{tcp.rtt_ms.toFixed(2)} ms</b></p> : null}
            </DiagnosticSection>

            <DiagnosticSection
              title="HTTP / TLS"
              step={report.http}
              summary={http ? `HTTP ${http.status || "失败"} · ${formatMs(http.total_ms)}` : "无结果"}
            >
              {http ? (
                <div className="diagnostic-http-grid">
                  <span>最终 URL</span><code>{http.final_url}</code>
                  <span>响应</span><b>{http.status || "-"} {http.status_text}</b>
                  <span>响应大小</span><b>{http.body_size_bytes.toLocaleString()} bytes</b>
                  <span>重定向</span><b>{http.redirects.length} 次</b>
                  {http.tls ? (
                    <>
                      <span>证书主体</span><code>{http.tls.subject}</code>
                      <span>证书颁发者</span><code>{http.tls.issuer}</code>
                      <span>证书有效期</span><b>{http.tls.days_until_expiry} 天</b>
                    </>
                  ) : null}
                </div>
              ) : null}
            </DiagnosticSection>

            <DiagnosticSection
              title="Traceroute"
              step={report.traceroute}
              summary={report.traceroute.status === "skipped" ? "未启用" : `${traceRows.length} 跳`}
            >
              {traceRows.length > 0 ? (
                <table>
                  <thead><tr><th>TTL</th><th>地址</th><th>探测 RTT</th></tr></thead>
                  <tbody>
                    {traceRows.map((hop) => (
                      <tr key={hop.ttl}>
                        <td>{hop.ttl}</td><td><code>{hop.addr ?? "*"}</code></td>
                        <td>{hop.rtts.map((rtt) => (rtt == null ? "*" : `${rtt.toFixed(1)} ms`)).join("  ")}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : null}
            </DiagnosticSection>
          </div>
        </div>
      ) : null}
    </div>
  );
}
