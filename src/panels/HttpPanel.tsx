import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type TlsCertInfo = {
  subject: string;
  issuer: string;
  serial: string;
  sans: string[];
  not_before: string;
  not_after: string;
  days_until_expiry: number;
  signature_algorithm: string;
};

type HttpProbeResult = {
  success: boolean;
  status: number;
  status_text: string;
  final_url: string;
  redirects: string[];
  headers: [string, string][];
  body_preview: string;
  body_truncated: boolean;
  body_size_bytes: number;
  total_ms: number;
  error: string | null;
  tls: TlsCertInfo | null;
};

const METHODS = ["GET", "HEAD", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"] as const;
type ResultTab = "body" | "headers" | "tls" | "redirects";

function statusColor(status: number): string {
  if (status === 0) return "#888";
  if (status >= 500) return "#f87171";
  if (status >= 400) return "#fb923c";
  if (status >= 300) return "#fbbf24";
  if (status >= 200) return "#4ade80";
  return "#9ca3af";
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

export default function HttpPanel() {
  const [url, setUrl] = useState("https://github.com");
  const [method, setMethod] = useState<(typeof METHODS)[number]>("GET");
  const [headers, setHeaders] = useState<[string, string][]>([["Accept", "*/*"]]);
  const [body, setBody] = useState("");
  const [followRedirects, setFollowRedirects] = useState(true);
  const [timeoutMs, setTimeoutMs] = useState(10000);

  const [showHeaders, setShowHeaders] = useState(false);
  const [showBodyEditor, setShowBodyEditor] = useState(false);

  const [result, setResult] = useState<HttpProbeResult | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<ResultTab>("body");

  function updateHeader(i: number, key: string, val: string) {
    const next = headers.slice();
    next[i] = [key, val];
    setHeaders(next);
  }
  function addHeader() {
    setHeaders([...headers, ["", ""]]);
  }
  function removeHeader(i: number) {
    setHeaders(headers.filter((_, idx) => idx !== i));
  }

  async function run() {
    setRunning(true);
    setError(null);
    setResult(null);
    try {
      const cleanHeaders = headers.filter(([k]) => k.trim().length > 0);
      const r = await invoke<HttpProbeResult>("http_request", {
        url,
        method,
        headers: cleanHeaders,
        body: body.length > 0 ? body : null,
        timeoutMs,
        followRedirects,
      });
      setResult(r);
      // 默认展示哪个 tab：HTTPS 自动跳 tls，否则 body
      if (r.tls) setActiveTab("body");
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div>
      <h2>HTTP</h2>
      <p className="subtitle">HTTP/HTTPS 请求测试 · 时延 · 响应头 · TLS 证书</p>

      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <select
          value={method}
          onChange={(e) =>
            setMethod(e.currentTarget.value as (typeof METHODS)[number])
          }
        >
          {METHODS.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
        <input
          value={url}
          onChange={(e) => setUrl(e.currentTarget.value)}
          placeholder="https://example.com/path"
          style={{ flex: 1, minWidth: 300 }}
        />
        <button type="submit" disabled={running}>
          {running ? "请求中…" : "发送"}
        </button>
      </form>

      <div className="toolbar">
        <label style={{ display: "flex", alignItems: "center", gap: 4, color: "#aaa" }}>
          <input
            type="checkbox"
            checked={followRedirects}
            onChange={(e) => setFollowRedirects(e.currentTarget.checked)}
            style={{ width: 14, height: 14 }}
          />
          跟随重定向
        </label>
        <label>超时(ms)</label>
        <input
          type="number"
          value={timeoutMs}
          onChange={(e) => setTimeoutMs(Number(e.currentTarget.value))}
          min={100}
          max={60000}
          style={{ width: 90 }}
        />
        <button
          type="button"
          onClick={() => setShowHeaders((v) => !v)}
          style={{ background: "transparent", border: "1px solid #444", color: "#aaa" }}
        >
          {showHeaders ? "收起" : "展开"} Headers ({headers.filter((h) => h[0]).length})
        </button>
        {method !== "GET" && method !== "HEAD" && (
          <button
            type="button"
            onClick={() => setShowBodyEditor((v) => !v)}
            style={{ background: "transparent", border: "1px solid #444", color: "#aaa" }}
          >
            {showBodyEditor ? "收起" : "展开"} Body
          </button>
        )}
      </div>

      {showHeaders && (
        <div style={{ marginBottom: 12, background: "#1f1f1f", padding: 10, borderRadius: 6 }}>
          {headers.map(([k, v], i) => (
            <div key={i} style={{ display: "flex", gap: 6, marginBottom: 6 }}>
              <input
                value={k}
                onChange={(e) => updateHeader(i, e.currentTarget.value, v)}
                placeholder="Header"
                style={{ width: 200 }}
              />
              <input
                value={v}
                onChange={(e) => updateHeader(i, k, e.currentTarget.value)}
                placeholder="Value"
                style={{ flex: 1 }}
              />
              <button
                type="button"
                onClick={() => removeHeader(i)}
                style={{ background: "#3a2020", borderColor: "#3a2020" }}
              >
                −
              </button>
            </div>
          ))}
          <button
            type="button"
            onClick={addHeader}
            style={{ background: "#2a3a2a", borderColor: "#2a3a2a" }}
          >
            + 加一行
          </button>
        </div>
      )}

      {showBodyEditor && method !== "GET" && method !== "HEAD" && (
        <textarea
          value={body}
          onChange={(e) => setBody(e.currentTarget.value)}
          placeholder='{"key":"value"}'
          style={{
            width: "100%",
            minHeight: 100,
            marginBottom: 12,
            padding: 8,
            background: "#1c1c1c",
            color: "#f0f0f0",
            border: "1px solid #3a3a3a",
            borderRadius: 6,
            fontFamily: "ui-monospace, SFMono-Regular, monospace",
            fontSize: 12,
          }}
        />
      )}

      {error && <div className="error-box">{error}</div>}

      {result && (
        <>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 16,
              padding: "10px 14px",
              background: "#1f1f1f",
              borderRadius: 6,
              marginBottom: 12,
            }}
          >
            <span
              style={{
                fontSize: 18,
                fontWeight: 600,
                color: statusColor(result.status),
              }}
            >
              {result.status || "—"}
            </span>
            <span style={{ color: "#aaa" }}>{result.status_text}</span>
            <span style={{ color: "#888", marginLeft: "auto", fontSize: 13 }}>
              {result.total_ms.toFixed(0)} ms · {formatBytes(result.body_size_bytes)}
              {result.body_truncated && " (预览已截断)"}
            </span>
          </div>

          {result.error && <div className="error-box">{result.error}</div>}

          {result.final_url !== url && (
            <p style={{ color: "#888", fontSize: 12, margin: "0 0 8px" }}>
              最终 URL: <code>{result.final_url}</code>
            </p>
          )}

          <div className="mode-tabs">
            <button
              type="button"
              className={`mode-tab ${activeTab === "body" ? "active" : ""}`}
              onClick={() => setActiveTab("body")}
            >
              Body
            </button>
            <button
              type="button"
              className={`mode-tab ${activeTab === "headers" ? "active" : ""}`}
              onClick={() => setActiveTab("headers")}
            >
              Headers ({result.headers.length})
            </button>
            {result.tls && (
              <button
                type="button"
                className={`mode-tab ${activeTab === "tls" ? "active" : ""}`}
                onClick={() => setActiveTab("tls")}
              >
                TLS Cert
              </button>
            )}
            {result.redirects.length > 0 && (
              <button
                type="button"
                className={`mode-tab ${activeTab === "redirects" ? "active" : ""}`}
                onClick={() => setActiveTab("redirects")}
              >
                Redirects ({result.redirects.length})
              </button>
            )}
          </div>

          {activeTab === "body" && (
            <pre
              style={{
                background: "#0e0e0e",
                color: "#cde",
                padding: 12,
                borderRadius: 6,
                fontFamily: "ui-monospace, SFMono-Regular, monospace",
                fontSize: 12,
                maxHeight: 400,
                overflow: "auto",
                whiteSpace: "pre-wrap",
                wordBreak: "break-all",
              }}
            >
              {result.body_preview || "(empty body)"}
            </pre>
          )}

          {activeTab === "headers" && (
            <table>
              <tbody>
                {result.headers.map(([k, v], i) => (
                  <tr key={i}>
                    <td style={{ color: "#9cf", width: 220, verticalAlign: "top" }}>
                      {k}
                    </td>
                    <td style={{ wordBreak: "break-all", fontFamily: "ui-monospace, SFMono-Regular, monospace", fontSize: 12 }}>
                      {v}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}

          {activeTab === "tls" && result.tls && <TlsView tls={result.tls} />}

          {activeTab === "redirects" && (
            <ol style={{ paddingLeft: 20, color: "#cde", fontFamily: "ui-monospace, SFMono-Regular, monospace", fontSize: 12 }}>
              {result.redirects.map((u, i) => (
                <li key={i} style={{ marginBottom: 4, wordBreak: "break-all" }}>
                  {u}
                </li>
              ))}
            </ol>
          )}
        </>
      )}
    </div>
  );
}

function TlsView({ tls }: { tls: TlsCertInfo }) {
  const expColor =
    tls.days_until_expiry < 0
      ? "#f87171"
      : tls.days_until_expiry < 30
        ? "#fbbf24"
        : "#4ade80";

  return (
    <table>
      <tbody>
        <tr>
          <td style={{ color: "#888", width: 160 }}>Subject</td>
          <td style={{ fontFamily: "ui-monospace, SFMono-Regular, monospace", fontSize: 12 }}>
            {tls.subject}
          </td>
        </tr>
        <tr>
          <td style={{ color: "#888" }}>Issuer</td>
          <td style={{ fontFamily: "ui-monospace, SFMono-Regular, monospace", fontSize: 12 }}>
            {tls.issuer}
          </td>
        </tr>
        <tr>
          <td style={{ color: "#888" }}>Serial</td>
          <td style={{ fontFamily: "ui-monospace, SFMono-Regular, monospace", fontSize: 12 }}>
            {tls.serial}
          </td>
        </tr>
        <tr>
          <td style={{ color: "#888" }}>Signature</td>
          <td style={{ fontFamily: "ui-monospace, SFMono-Regular, monospace", fontSize: 12 }}>
            {tls.signature_algorithm}
          </td>
        </tr>
        <tr>
          <td style={{ color: "#888" }}>Valid From</td>
          <td>{tls.not_before}</td>
        </tr>
        <tr>
          <td style={{ color: "#888" }}>Expires</td>
          <td>
            {tls.not_after}{" "}
            <span style={{ color: expColor, marginLeft: 8 }}>
              ({tls.days_until_expiry < 0 ? "已过期 " : ""}
              {Math.abs(tls.days_until_expiry)} 天)
            </span>
          </td>
        </tr>
        <tr>
          <td style={{ color: "#888", verticalAlign: "top" }}>SAN</td>
          <td style={{ fontFamily: "ui-monospace, SFMono-Regular, monospace", fontSize: 12 }}>
            {tls.sans.length > 0 ? tls.sans.join(", ") : "—"}
          </td>
        </tr>
      </tbody>
    </table>
  );
}
