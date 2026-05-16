import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import LatencyChart, { LatencyPoint } from "../components/LatencyChart";

type PingResult = {
  seq: number;
  rtt_ms: number;
  from: string;
  success: boolean;
  error: string | null;
};

export default function PingPanel() {
  const [host, setHost] = useState("8.8.8.8");
  const [count, setCount] = useState(10);
  const [results, setResults] = useState<PingResult[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [backend, setBackend] = useState<string>("…");

  const unlistenRef = useRef<UnlistenFn | null>(null);

  useEffect(() => {
    invoke<string>("ping_backend").then(setBackend).catch(() => setBackend("?"));
  }, []);

  useEffect(() => {
    let mounted = true;
    (async () => {
      const u = await listen<PingResult>("ping-result", (event) => {
        if (!mounted) return;
        setResults((prev) => [...prev, event.payload]);
      });
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
    try {
      // command 返回的最终 Vec 我们这里不用，结果靠 ping-result 事件实时累积
      await invoke<PingResult[]>("ping_host", {
        host,
        count,
        timeoutMs: 2000,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }

  const ok = results.filter((r) => r.success);
  const avg = ok.length
    ? (ok.reduce((s, r) => s + r.rtt_ms, 0) / ok.length).toFixed(2)
    : "—";
  const loss = results.length
    ? (((results.length - ok.length) / results.length) * 100).toFixed(0)
    : "—";

  const chartData: LatencyPoint[] = results.map((r) => ({
    seq: r.seq,
    rtt_ms: r.success ? r.rtt_ms : null,
  }));

  return (
    <div>
      <h2>Ping</h2>
      <p className="subtitle">
        ICMP 探测 · backend:{" "}
        <code style={{ color: backend === "raw" ? "#f9a" : "#9cf" }}>
          {backend}
        </code>
      </p>

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
          style={{ flex: 1, minWidth: 220 }}
        />
        <label>count</label>
        <input
          type="number"
          value={count}
          onChange={(e) => setCount(Number(e.currentTarget.value))}
          min={1}
          max={100}
          style={{ width: 70 }}
        />
        <button type="submit" disabled={running}>
          {running ? "Pinging…" : "Ping"}
        </button>
      </form>

      {error && <div className="error-box">{error}</div>}

      {results.length > 0 && (
        <>
          <div className="stats">
            <span>
              成功 <b>{ok.length}/{results.length}</b>
            </span>
            <span>
              丢包率 <b>{loss}%</b>
            </span>
            <span>
              平均 RTT <b>{avg} ms</b>
            </span>
            {running && <span style={{ color: "#4aa3ff" }}>● 实时</span>}
          </div>

          <LatencyChart data={chartData} />

          <table>
            <thead>
              <tr>
                <th style={{ width: 60 }}>Seq</th>
                <th>From</th>
                <th style={{ width: 120 }}>RTT (ms)</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {results.map((r) => (
                <tr key={r.seq}>
                  <td>{r.seq}</td>
                  <td>{r.from}</td>
                  <td>{r.success ? r.rtt_ms.toFixed(2) : "—"}</td>
                  <td className={r.success ? "status-ok" : "status-fail"}>
                    {r.success ? "OK" : r.error ?? "FAIL"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}
