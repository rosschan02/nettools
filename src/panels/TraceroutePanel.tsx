import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import AddressInput from "../components/AddressInput";
import { useAddressHistory } from "../utils/history";

type TraceHop = {
  ttl: number;
  addr: string | null;
  rtts: (number | null)[];
  raw: string;
};

type TraceDone = {
  total_ms: number;
  hop_count: number;
};

function stats(rtts: (number | null)[]) {
  const ok = rtts.filter((v): v is number => v != null);
  if (ok.length === 0) return { min: null, avg: null, max: null, loss: 100 };
  const min = Math.min(...ok);
  const max = Math.max(...ok);
  const avg = ok.reduce((s, v) => s + v, 0) / ok.length;
  const loss = ((rtts.length - ok.length) / rtts.length) * 100;
  return { min, avg, max, loss };
}

export default function TraceroutePanel() {
  const [host, setHost] = useState("google.com");
  const [maxHops, setMaxHops] = useState(30);
  const [timeoutMs, setTimeoutMs] = useState(3000);

  const [hops, setHops] = useState<TraceHop[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [totalMs, setTotalMs] = useState<number | null>(null);
  const hostHistory = useAddressHistory("traceroute.host");

  // 用 ref 保存 unlisten 函数，组件卸载时一次性清理
  const unlistenRefs = useRef<UnlistenFn[]>([]);

  useEffect(() => {
    // 注册一次事件监听，整个 panel 生命周期共用
    (async () => {
      const u1 = await listen<TraceHop>("trace-hop", (event) => {
        setHops((prev) => [...prev, event.payload]);
      });
      const u2 = await listen<TraceDone>("trace-done", (event) => {
        setTotalMs(event.payload.total_ms);
      });
      unlistenRefs.current.push(u1, u2);
    })();
    return () => {
      unlistenRefs.current.forEach((u) => u());
      unlistenRefs.current = [];
    };
  }, []);

  async function run() {
    hostHistory.add(host);
    setRunning(true);
    setError(null);
    setHops([]);
    setTotalMs(null);
    try {
      await invoke("traceroute_run", {
        host: host.trim(),
        maxHops,
        timeoutMs,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div>
      <h2>Traceroute</h2>
      <p className="subtitle">
        逐跳追踪路由路径 · 实时显示 · 子进程模式（无需 sudo）
      </p>

      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <AddressInput
          value={host}
          onChange={setHost}
          history={hostHistory}
          placeholder="host or IP"
          containerStyle={{ flex: 1, minWidth: 240 }}
        />
        <label>最大跳数</label>
        <input
          type="number"
          value={maxHops}
          onChange={(e) => setMaxHops(Number(e.currentTarget.value))}
          min={1}
          max={64}
          style={{ width: 70 }}
        />
        <label>每跳超时(ms)</label>
        <input
          type="number"
          value={timeoutMs}
          onChange={(e) => setTimeoutMs(Number(e.currentTarget.value))}
          min={500}
          max={10000}
          style={{ width: 90 }}
        />
        <button type="submit" disabled={running}>
          {running ? "追踪中…" : "开始"}
        </button>
      </form>

      {error && <div className="error-box">{error}</div>}

      {(hops.length > 0 || running) && (
        <>
          <div className="stats">
            <span>
              已收到 <b>{hops.length}</b> 跳
            </span>
            {totalMs !== null && (
              <span>
                总耗时 <b>{(totalMs / 1000).toFixed(2)} s</b>
              </span>
            )}
            {running && (
              <span style={{ color: "#4aa3ff" }}>● 实时追踪中</span>
            )}
          </div>

          <table>
            <thead>
              <tr>
                <th style={{ width: 50 }}>TTL</th>
                <th style={{ width: 180 }}>Address</th>
                <th style={{ width: 80 }}>Min</th>
                <th style={{ width: 80 }}>Avg</th>
                <th style={{ width: 80 }}>Max</th>
                <th style={{ width: 70 }}>Loss</th>
                <th>Probes</th>
              </tr>
            </thead>
            <tbody>
              {hops.map((h) => {
                const s = stats(h.rtts);
                const isTimeout = h.addr == null && s.loss === 100;
                return (
                  <tr key={`${h.ttl}-${h.raw}`}>
                    <td>{h.ttl}</td>
                    <td
                      style={{
                        fontFamily: "ui-monospace, SFMono-Regular, monospace",
                        fontSize: 12,
                      }}
                      className={isTimeout ? "status-fail" : undefined}
                    >
                      {h.addr ?? "* * *"}
                    </td>
                    <td>{s.min !== null ? s.min.toFixed(2) : "—"}</td>
                    <td>{s.avg !== null ? s.avg.toFixed(2) : "—"}</td>
                    <td>{s.max !== null ? s.max.toFixed(2) : "—"}</td>
                    <td
                      style={{
                        color:
                          s.loss === 0
                            ? "#4ade80"
                            : s.loss < 100
                              ? "#fbbf24"
                              : "#f87171",
                      }}
                    >
                      {s.loss.toFixed(0)}%
                    </td>
                    <td style={{ fontSize: 11, color: "#888" }}>
                      {h.rtts
                        .map((v) => (v == null ? "*" : `${v.toFixed(1)}`))
                        .join("  ")}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}
