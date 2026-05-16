import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { parsePorts } from "../utils/ports";
import LatencyChart, { LatencyPoint } from "../components/LatencyChart";

type TcpProbeResult = {
  host: string;
  port: number;
  success: boolean;
  rtt_ms: number;
  error: string | null;
  service: string | null;
  banner: string | null;
};

type Mode = "single" | "scan" | "tcp-ping";

export default function TcpPanel() {
  const [mode, setMode] = useState<Mode>("single");
  const [host, setHost] = useState("github.com");
  const [singlePort, setSinglePort] = useState(443);
  const [portsExpr, setPortsExpr] = useState("22,80,443,3306,6379,8080-8090");
  const [count, setCount] = useState(5);
  const [intervalMs, setIntervalMs] = useState(500);
  const [timeoutMs, setTimeoutMs] = useState(1500);
  const [concurrency, setConcurrency] = useState(50);
  const [fingerprint, setFingerprint] = useState(true);

  const [results, setResults] = useState<TcpProbeResult[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState<number | null>(null);

  // TCP Ping 模式用事件流实时累积。这里 seq 由事件 payload 带来。
  const tcpPingSeqRef = useRef(0);
  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    let mounted = true;
    (async () => {
      const u = await listen<{ seq: number; result: TcpProbeResult }>(
        "tcp-ping-result",
        (event) => {
          if (!mounted) return;
          setResults((prev) => [...prev, event.payload.result]);
          tcpPingSeqRef.current = event.payload.seq;
        }
      );
      unlistenRef.current = u;
    })();
    return () => {
      mounted = false;
      unlistenRef.current?.();
      unlistenRef.current = null;
    };
  }, []);

  async function run() {
    setRunning(true);
    setError(null);
    setResults([]);
    setElapsed(null);
    const t0 = performance.now();
    try {
      if (mode === "single") {
        const r = await invoke<TcpProbeResult>("tcp_probe", {
          host,
          port: singlePort,
          timeoutMs,
          fingerprint,
        });
        setResults([r]);
      } else if (mode === "scan") {
        const ports = parsePorts(portsExpr);
        const r = await invoke<TcpProbeResult[]>("tcp_scan", {
          host,
          ports,
          timeoutMs,
          fingerprint,
          concurrency,
        });
        setResults(r);
      } else {
        // TCP Ping：结果靠 tcp-ping-result 事件实时累积，await 完成时再校对一次
        await invoke<TcpProbeResult[]>("tcp_ping", {
          host,
          port: singlePort,
          count,
          intervalMs,
          timeoutMs,
        });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
      setElapsed(performance.now() - t0);
    }
  }

  const open = results.filter((r) => r.success);
  const avgRtt = open.length
    ? (open.reduce((s, r) => s + r.rtt_ms, 0) / open.length).toFixed(2)
    : "—";

  return (
    <div>
      <h2>TCP</h2>
      <p className="subtitle">
        TCP 连接探测 · 端口扫描 · 服务指纹 · 连续 TCP Ping
      </p>

      <div className="mode-tabs">
        <button
          className={`mode-tab ${mode === "single" ? "active" : ""}`}
          onClick={() => setMode("single")}
        >
          单端口
        </button>
        <button
          className={`mode-tab ${mode === "scan" ? "active" : ""}`}
          onClick={() => setMode("scan")}
        >
          多端口扫描
        </button>
        <button
          className={`mode-tab ${mode === "tcp-ping" ? "active" : ""}`}
          onClick={() => setMode("tcp-ping")}
        >
          TCP Ping
        </button>
      </div>

      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <input
          value={host}
          onChange={(e) => setHost(e.currentTarget.value)}
          placeholder="host or IP"
          style={{ flex: 1, minWidth: 200 }}
        />

        {mode === "single" || mode === "tcp-ping" ? (
          <>
            <label>port</label>
            <input
              type="number"
              value={singlePort}
              onChange={(e) => setSinglePort(Number(e.currentTarget.value))}
              min={1}
              max={65535}
              style={{ width: 80 }}
            />
          </>
        ) : (
          <>
            <label>ports</label>
            <input
              value={portsExpr}
              onChange={(e) => setPortsExpr(e.currentTarget.value)}
              placeholder="80,443,8000-8010"
              style={{ flex: 2, minWidth: 220 }}
            />
          </>
        )}

        {mode === "tcp-ping" && (
          <>
            <label>count</label>
            <input
              type="number"
              value={count}
              onChange={(e) => setCount(Number(e.currentTarget.value))}
              min={1}
              max={100}
              style={{ width: 60 }}
            />
            <label>间隔(ms)</label>
            <input
              type="number"
              value={intervalMs}
              onChange={(e) => setIntervalMs(Number(e.currentTarget.value))}
              min={50}
              max={10000}
              style={{ width: 80 }}
            />
          </>
        )}

        {mode === "scan" && (
          <>
            <label>并发</label>
            <input
              type="number"
              value={concurrency}
              onChange={(e) => setConcurrency(Number(e.currentTarget.value))}
              min={1}
              max={256}
              style={{ width: 70 }}
            />
          </>
        )}

        <label>超时(ms)</label>
        <input
          type="number"
          value={timeoutMs}
          onChange={(e) => setTimeoutMs(Number(e.currentTarget.value))}
          min={100}
          max={30000}
          style={{ width: 80 }}
        />

        {mode !== "tcp-ping" && (
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: 4,
              color: "#aaa",
            }}
          >
            <input
              type="checkbox"
              checked={fingerprint}
              onChange={(e) => setFingerprint(e.currentTarget.checked)}
              style={{ width: 14, height: 14 }}
            />
            指纹
          </label>
        )}

        <button type="submit" disabled={running}>
          {running ? "运行中…" : "开始"}
        </button>
      </form>

      {error && <div className="error-box">{error}</div>}

      {results.length > 0 && (
        <>
          <div className="stats">
            {mode === "scan" && (
              <span>
                开放 <b>{open.length}/{results.length}</b>
              </span>
            )}
            {mode === "tcp-ping" && (
              <span>
                成功 <b>{open.length}/{results.length}</b>
              </span>
            )}
            <span>
              平均 RTT <b>{avgRtt} ms</b>
            </span>
            {elapsed !== null && (
              <span>
                总耗时 <b>{elapsed.toFixed(0)} ms</b>
              </span>
            )}
            {mode === "tcp-ping" && running && (
              <span style={{ color: "#4aa3ff" }}>● 实时</span>
            )}
          </div>

          {mode === "tcp-ping" && (
            <LatencyChart
              data={
                results.map((r, i) => ({
                  seq: i,
                  rtt_ms: r.success ? r.rtt_ms : null,
                })) as LatencyPoint[]
              }
            />
          )}

          <table>
            <thead>
              <tr>
                <th style={{ width: 60 }}>Port</th>
                <th style={{ width: 100 }}>RTT (ms)</th>
                <th style={{ width: 100 }}>Status</th>
                {(mode === "single" || mode === "scan") && (
                  <>
                    <th style={{ width: 160 }}>Service</th>
                    <th>Banner</th>
                  </>
                )}
                {mode === "tcp-ping" && <th>Error</th>}
              </tr>
            </thead>
            <tbody>
              {results.map((r, i) => (
                <tr key={`${r.port}-${i}`}>
                  <td>{r.port}</td>
                  <td>{r.rtt_ms.toFixed(2)}</td>
                  <td className={r.success ? "status-ok" : "status-fail"}>
                    {r.success ? "OPEN" : "CLOSED"}
                  </td>
                  {(mode === "single" || mode === "scan") && (
                    <>
                      <td>{r.service ?? "—"}</td>
                      <td>
                        {r.banner ? (
                          <span className="banner" title={r.banner}>
                            {r.banner.replace(/[\r\n]+/g, " ⏎ ").slice(0, 80)}
                          </span>
                        ) : (
                          "—"
                        )}
                      </td>
                    </>
                  )}
                  {mode === "tcp-ping" && (
                    <td className="status-fail" style={{ fontSize: 12 }}>
                      {r.error ?? ""}
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}
