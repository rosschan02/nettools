import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type LanInterface = {
  name: string;
  address: string;
  netmask: string;
  prefix: number;
  cidr: string;
  broadcast: string;
  mac: string | null;
};

type LanInfo = {
  interface_name: string;
  address: string;
  netmask: string;
  prefix: number;
  cidr: string;
  suggested_cidr: string;
  broadcast: string;
  gateway: string | null;
  mac: string | null;
  interfaces: LanInterface[];
};

type LanHost = {
  ip: string;
  mac: string | null;
  methods: string[];
  open_ports: number[];
  reserved_reason: string | null;
};

type ReservedAddress = {
  ip: string;
  reason: string;
};

type LanScanReport = {
  info: LanInfo;
  cidr: string;
  scanned_hosts: number;
  active_hosts: LanHost[];
  candidates: string[];
  reserved: ReservedAddress[];
  duration_ms: number;
  warning: string;
};

const METHOD_LABELS: Record<string, string> = {
  icmp: "Ping",
  tcp: "TCP",
  arp: "ARP",
  local: "本机",
};

function formatDuration(value: number): string {
  return value >= 1000 ? `${(value / 1000).toFixed(1)} 秒` : `${value.toFixed(0)} ms`;
}

function InfoItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="lan-info-item">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function MethodBadge({ method }: { method: string }) {
  return <span className={`lan-method ${method}`}>{METHOD_LABELS[method] ?? method}</span>;
}

export default function LanPanel() {
  const [info, setInfo] = useState<LanInfo | null>(null);
  const [cidr, setCidr] = useState("");
  const [timeoutMs, setTimeoutMs] = useState(700);
  const [concurrency, setConcurrency] = useState(32);
  const [suggestionCount, setSuggestionCount] = useState(12);
  const [report, setReport] = useState<LanScanReport | null>(null);
  const [loadingInfo, setLoadingInfo] = useState(true);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedIp, setCopiedIp] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    invoke<LanInfo>("lan_info")
      .then((result) => {
        if (!mounted) return;
        setInfo(result);
        setCidr(result.suggested_cidr);
      })
      .catch((caught) => {
        if (mounted) setError(String(caught));
      })
      .finally(() => {
        if (mounted) setLoadingInfo(false);
      });
    return () => {
      mounted = false;
    };
  }, []);

  async function refreshInfo() {
    setLoadingInfo(true);
    setError(null);
    try {
      const result = await invoke<LanInfo>("lan_info");
      setInfo(result);
      setCidr(result.suggested_cidr);
      setReport(null);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setLoadingInfo(false);
    }
  }

  async function runScan() {
    setRunning(true);
    setError(null);
    setReport(null);
    setCopiedIp(null);
    try {
      const result = await invoke<LanScanReport>("lan_scan", {
        cidr: cidr.trim() || null,
        timeoutMs,
        concurrency,
        suggestionCount,
      });
      setReport(result);
      setInfo(result.info);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setRunning(false);
    }
  }

  async function copyIp(ip: string) {
    try {
      await navigator.clipboard.writeText(ip);
      setCopiedIp(ip);
    } catch (caught) {
      setError(`复制失败: ${String(caught)}`);
    }
  }

  return (
    <div className="lan-panel">
      <div className="lan-heading">
        <div>
          <h2>局域网 IP 助手</h2>
          <p className="subtitle">识别当前子网 · 查找在线设备 · 推荐疑似空闲地址</p>
        </div>
        <button
          type="button"
          className="secondary-button"
          onClick={() => void refreshInfo()}
          disabled={loadingInfo || running}
        >
          {loadingInfo ? "识别中…" : "重新识别网卡"}
        </button>
      </div>

      <div className="lan-safety-note">
        <strong>使用边界</strong>
        <span>仅扫描你有权管理的本地网络。无响应不代表永久空闲，正式分配前仍需核对 DHCP 租约和保留范围。</span>
      </div>

      {info ? (
        <section className="lan-info-grid" aria-label="当前网络信息">
          <InfoItem label="网络接口" value={info.interface_name} />
          <InfoItem label="本机地址" value={info.address} />
          <InfoItem label="默认网关" value={info.gateway ?? "未识别"} />
          <InfoItem label="实际子网" value={info.cidr} />
          <InfoItem label="扫描建议" value={info.suggested_cidr} />
          <InfoItem label="本机 MAC" value={info.mac ?? "未识别"} />
        </section>
      ) : null}

      <form
        className="lan-scan-form"
        onSubmit={(event) => {
          event.preventDefault();
          void runScan();
        }}
      >
        <div className="lan-primary-controls">
          <label>
            扫描范围
            <input
              value={cidr}
              onChange={(event) => setCidr(event.currentTarget.value)}
              placeholder="192.168.1.0/24"
              required
              disabled={!info || running}
            />
          </label>
          <button type="submit" disabled={!info || running || loadingInfo}>
            {running ? "正在扫描…" : "扫描并推荐 IP"}
          </button>
        </div>
        <div className="lan-scan-options">
          <label>
            单主机超时
            <input
              type="number"
              value={timeoutMs}
              onChange={(event) => setTimeoutMs(Number(event.currentTarget.value))}
              min={100}
              max={5000}
              step={100}
            />
            ms
          </label>
          <label>
            并发
            <input
              type="number"
              value={concurrency}
              onChange={(event) => setConcurrency(Number(event.currentTarget.value))}
              min={1}
              max={64}
            />
          </label>
          <label>
            候选数量
            <input
              type="number"
              value={suggestionCount}
              onChange={(event) => setSuggestionCount(Number(event.currentTarget.value))}
              min={1}
              max={100}
            />
          </label>
          <span>探测：ICMP + TCP 22/80/443/445/3389/9100 + ARP</span>
        </div>
      </form>

      {running ? (
        <div className="diagnostic-running" role="status">
          <span className="diagnostic-spinner" />
          正在并发探测 {cidr}，较大的子网可能需要十几秒
        </div>
      ) : null}
      {error ? <div className="error-box">{error}</div> : null}

      {report ? (
        <div className="lan-results">
          <section className="lan-result-summary">
            <div>
              <span>扫描范围</span>
              <strong>{report.cidr}</strong>
            </div>
            <div><span>已探测</span><strong>{report.scanned_hosts}</strong></div>
            <div><span>发现设备</span><strong>{report.active_hosts.length}</strong></div>
            <div><span>总耗时</span><strong>{formatDuration(report.duration_ms)}</strong></div>
          </section>

          <section className="lan-candidates">
            <div className="lan-section-heading">
              <div>
                <h3>疑似空闲候选</h3>
                <p>本次扫描没有发现响应，点击即可复制</p>
              </div>
              <span>{report.candidates.length} 个</span>
            </div>
            {report.candidates.length > 0 ? (
              <div className="lan-candidate-list">
                {report.candidates.map((ip) => (
                  <button
                    type="button"
                    className={copiedIp === ip ? "copied" : ""}
                    key={ip}
                    onClick={() => void copyIp(ip)}
                    title="复制 IP"
                  >
                    <code>{ip}</code>
                    <span>{copiedIp === ip ? "已复制" : "复制"}</span>
                  </button>
                ))}
              </div>
            ) : (
              <div className="lan-empty">当前范围内没有可推荐的候选地址。</div>
            )}
            <div className="lan-warning">{report.warning}</div>
          </section>

          <section className="lan-hosts">
            <div className="lan-section-heading">
              <div>
                <h3>在线或已占用设备</h3>
                <p>ARP 条目可能包含最近通信过但当前休眠的设备</p>
              </div>
              <span>{report.active_hosts.length} 台</span>
            </div>
            {report.active_hosts.length > 0 ? (
              <table>
                <thead>
                  <tr><th>IP</th><th>MAC</th><th>发现方式</th><th>开放端口</th><th>备注</th></tr>
                </thead>
                <tbody>
                  {report.active_hosts.map((host) => (
                    <tr key={host.ip}>
                      <td><code>{host.ip}</code></td>
                      <td><code>{host.mac ?? "-"}</code></td>
                      <td>
                        <div className="lan-methods">
                          {host.methods.map((method) => <MethodBadge method={method} key={method} />)}
                        </div>
                      </td>
                      <td>{host.open_ports.length > 0 ? host.open_ports.join(", ") : "-"}</td>
                      <td>{host.reserved_reason ?? "-"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className="lan-empty">没有发现在线设备；请适当增加超时后重试。</div>
            )}
          </section>

          <section className="lan-reserved">
            <h3>明确保留</h3>
            <div>
              {report.reserved.map((row) => (
                <span key={`${row.ip}-${row.reason}`}><code>{row.ip}</code>{row.reason}</span>
              ))}
            </div>
          </section>
        </div>
      ) : null}
    </div>
  );
}
