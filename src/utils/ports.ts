/**
 * 解析端口表达式：
 *   "80"              -> [80]
 *   "80,443"          -> [80, 443]
 *   "8000-8003"       -> [8000, 8001, 8002, 8003]
 *   "22,80,443,8000-8010"
 * 返回去重后的升序端口列表；非法 token 抛出友好错误。
 */
export function parsePorts(input: string): number[] {
  const tokens = input
    .split(",")
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  if (tokens.length === 0) {
    throw new Error("端口列表为空");
  }
  const set = new Set<number>();
  for (const tok of tokens) {
    if (tok.includes("-")) {
      const [lo, hi] = tok.split("-").map((s) => s.trim());
      const a = Number(lo);
      const b = Number(hi);
      if (!isValidPort(a) || !isValidPort(b) || a > b) {
        throw new Error(`非法端口范围: "${tok}"`);
      }
      for (let p = a; p <= b; p++) set.add(p);
    } else {
      const p = Number(tok);
      if (!isValidPort(p)) {
        throw new Error(`非法端口: "${tok}"`);
      }
      set.add(p);
    }
  }
  return [...set].sort((a, b) => a - b);
}

function isValidPort(n: number): boolean {
  return Number.isInteger(n) && n >= 1 && n <= 65535;
}
