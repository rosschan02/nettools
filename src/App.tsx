import { useState } from "react";
import PingPanel from "./panels/PingPanel";
import TcpPanel from "./panels/TcpPanel";
import DnsPanel from "./panels/DnsPanel";
import HttpPanel from "./panels/HttpPanel";
import TraceroutePanel from "./panels/TraceroutePanel";
import "./App.css";

type Tool = "ping" | "tcp" | "dns" | "http" | "traceroute";

const TOOLS: { id: Tool; label: string; enabled: boolean }[] = [
  { id: "ping", label: "Ping", enabled: true },
  { id: "tcp", label: "TCP", enabled: true },
  { id: "dns", label: "DNS", enabled: true },
  { id: "http", label: "HTTP", enabled: true },
  { id: "traceroute", label: "Traceroute", enabled: true },
];

function App() {
  const [tool, setTool] = useState<Tool>("ping");

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="sidebar-title">netools</div>
        {TOOLS.map((t) => (
          <button
            key={t.id}
            className={`nav-item ${tool === t.id ? "active" : ""}`}
            onClick={() => t.enabled && setTool(t.id)}
            disabled={!t.enabled}
            style={!t.enabled ? { opacity: 0.4 } : undefined}
            title={t.enabled ? "" : "未实现"}
          >
            {t.label}
            {!t.enabled && (
              <span style={{ fontSize: 11, marginLeft: 6, color: "#666" }}>
                soon
              </span>
            )}
          </button>
        ))}
      </aside>

      <main className="panel">
        {tool === "ping" && <PingPanel />}
        {tool === "tcp" && <TcpPanel />}
        {tool === "dns" && <DnsPanel />}
        {tool === "http" && <HttpPanel />}
        {tool === "traceroute" && <TraceroutePanel />}
      </main>
    </div>
  );
}

export default App;
