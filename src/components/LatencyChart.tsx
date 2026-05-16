import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

export type LatencyPoint = {
  seq: number;
  rtt_ms: number | null; // null = 失败
};

type Props = {
  data: LatencyPoint[];
  height?: number;
  xLabel?: string;
};

export default function LatencyChart({ data, height = 220, xLabel = "Seq" }: Props) {
  const successful = data.filter((d) => d.rtt_ms != null) as { seq: number; rtt_ms: number }[];
  const avg = successful.length
    ? successful.reduce((s, d) => s + d.rtt_ms, 0) / successful.length
    : 0;

  // 给图表用：把 null 转为 undefined 让 Recharts 自动断开折线，而不是连一条到 0
  const chartData = data.map((d) => ({
    seq: d.seq,
    rtt: d.rtt_ms == null ? undefined : d.rtt_ms,
    failed: d.rtt_ms == null,
  }));

  return (
    <div style={{ width: "100%", height, marginTop: 12 }}>
      <ResponsiveContainer>
        <LineChart data={chartData} margin={{ top: 8, right: 16, left: -8, bottom: 8 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#2a2a2a" />
          <XAxis
            dataKey="seq"
            stroke="#888"
            label={{ value: xLabel, position: "insideBottom", offset: -4, fill: "#888", fontSize: 11 }}
            tick={{ fill: "#888", fontSize: 11 }}
          />
          <YAxis
            stroke="#888"
            label={{
              value: "RTT (ms)",
              angle: -90,
              position: "insideLeft",
              fill: "#888",
              fontSize: 11,
              dy: 30,
            }}
            tick={{ fill: "#888", fontSize: 11 }}
          />
          <Tooltip
            contentStyle={{
              background: "#1f1f1f",
              border: "1px solid #444",
              borderRadius: 6,
              fontSize: 12,
            }}
            labelStyle={{ color: "#aaa" }}
            itemStyle={{ color: "#cde" }}
            formatter={(v) =>
              typeof v === "number" ? `${v.toFixed(2)} ms` : "失败"
            }
            labelFormatter={(l) => `${xLabel} ${l}`}
          />
          {avg > 0 && (
            <ReferenceLine
              y={avg}
              stroke="#fbbf24"
              strokeDasharray="4 4"
              label={{
                value: `avg ${avg.toFixed(1)}ms`,
                position: "right",
                fill: "#fbbf24",
                fontSize: 10,
              }}
            />
          )}
          <Line
            type="monotone"
            dataKey="rtt"
            stroke="#4aa3ff"
            strokeWidth={2}
            dot={{ r: 3, fill: "#4aa3ff" }}
            activeDot={{ r: 5 }}
            connectNulls={false}
            isAnimationActive={false}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
