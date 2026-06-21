import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import AddressInput from "../components/AddressInput";
import { useAddressHistory } from "../utils/history";

type DnsServerResult = {
  server: string;
  rtt_ms: number;
  success: boolean;
  records: string[];
  error: string | null;
};

const RECORD_TYPES = ["A", "AAAA", "CNAME", "MX", "NS", "TXT", "SOA", "SRV", "CAA"] as const;

const PRESET_SERVERS: { id: string; label: string }[] = [
  { id: "system", label: "系统默认" },
  { id: "1.1.1.1", label: "Cloudflare (1.1.1.1)" },
  { id: "8.8.8.8", label: "Google (8.8.8.8)" },
  { id: "9.9.9.9", label: "Quad9 (9.9.9.9)" },
  { id: "208.67.222.222", label: "OpenDNS" },
  { id: "223.5.5.5", label: "阿里 (223.5.5.5)" },
  { id: "119.29.29.29", label: "DNSPod (119.29.29.29)" },
];

export default function DnsPanel() {
  const [domain, setDomain] = useState("github.com");
  const [recordType, setRecordType] = useState<(typeof RECORD_TYPES)[number]>("A");
  const [selected, setSelected] = useState<Set<string>>(
    new Set(["system", "1.1.1.1", "8.8.8.8"])
  );
  const [customServer, setCustomServer] = useState("");
  const [timeoutMs, setTimeoutMs] = useState(2000);
  const domainHistory = useAddressHistory("dns.domain");

  const [results, setResults] = useState<DnsServerResult[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  }

  async function run() {
    domainHistory.add(domain);
    setRunning(true);
    setError(null);
    setResults([]);
    try {
      const servers = [...selected];
      if (customServer.trim()) servers.push(customServer.trim());
      if (servers.length === 0) {
        throw new Error("请至少选一个 DNS 服务器");
      }
      const r = await invoke<DnsServerResult[]>("dns_query", {
        domain: domain.trim(),
        recordType,
        servers,
        timeoutMs,
      });
      // 按 RTT 升序展示
      r.sort((a, b) => {
        if (a.success !== b.success) return a.success ? -1 : 1;
        return a.rtt_ms - b.rtt_ms;
      });
      setResults(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div>
      <h2>DNS</h2>
      <p className="subtitle">
        多 DNS 服务器并发查询 · 比较响应时间和返回记录
      </p>

      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <AddressInput
          value={domain}
          onChange={setDomain}
          history={domainHistory}
          placeholder="domain (e.g. github.com)"
          containerStyle={{ flex: 1, minWidth: 220 }}
        />
        <label>类型</label>
        <select
          value={recordType}
          onChange={(e) => setRecordType(e.currentTarget.value as (typeof RECORD_TYPES)[number])}
        >
          {RECORD_TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        <label>超时(ms)</label>
        <input
          type="number"
          value={timeoutMs}
          onChange={(e) => setTimeoutMs(Number(e.currentTarget.value))}
          min={100}
          max={30000}
          style={{ width: 80 }}
        />
        <button type="submit" disabled={running}>
          {running ? "查询中…" : "查询"}
        </button>
      </form>

      <div style={{ display: "flex", gap: 12, flexWrap: "wrap", margin: "8px 0 16px" }}>
        {PRESET_SERVERS.map((s) => (
          <label
            key={s.id}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              color: "#bbb",
              fontSize: 13,
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={selected.has(s.id)}
              onChange={() => toggle(s.id)}
              style={{ width: 14, height: 14 }}
            />
            {s.label}
          </label>
        ))}
        <input
          value={customServer}
          onChange={(e) => setCustomServer(e.currentTarget.value)}
          placeholder="自定义 IP"
          style={{ width: 140 }}
        />
      </div>

      {error && <div className="error-box">{error}</div>}

      {results.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          {results.map((r) => (
            <div
              key={r.server}
              style={{
                border: `1px solid ${r.success ? "#2a4a2a" : "#4a2a2a"}`,
                background: "#1f1f1f",
                borderRadius: 8,
                padding: "10px 14px",
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: 6,
                }}
              >
                <span style={{ fontWeight: 500, color: "#f0f0f0" }}>{r.server}</span>
                <span style={{ display: "flex", gap: 16, fontSize: 13 }}>
                  <span className={r.success ? "status-ok" : "status-fail"}>
                    {r.success ? "OK" : "FAIL"}
                  </span>
                  <span style={{ color: "#aaa" }}>{r.rtt_ms.toFixed(1)} ms</span>
                </span>
              </div>
              {r.success ? (
                <table style={{ background: "transparent" }}>
                  <tbody>
                    {r.records.map((rec, i) => (
                      <tr key={i}>
                        <td
                          style={{
                            fontFamily: "ui-monospace, SFMono-Regular, monospace",
                            fontSize: 12,
                            color: "#cde",
                            borderBottom: "none",
                            padding: "2px 0",
                          }}
                        >
                          {rec}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <div style={{ color: "#f87171", fontSize: 12 }}>
                  {r.error ?? "未知错误"}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
