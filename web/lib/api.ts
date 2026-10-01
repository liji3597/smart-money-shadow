import type {
  Health,
  Metrics,
  PerformanceStats,
  Positions,
  Signal,
  TokenInfo,
  TradeRecord,
} from "./types";

const API_BASE =
  process.env.NEXT_PUBLIC_API_BASE ?? "http://localhost:8080";

export const WS_URL = API_BASE.replace(/^http/, "ws") + "/ws";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`API ${path} failed: ${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

export async function fetchMetrics(): Promise<Metrics> {
  const data = await get<{ metrics: Metrics }>("/api/metrics");
  return data.metrics;
}

export async function fetchSignals(limit = 50): Promise<Signal[]> {
  const data = await get<{ signals: Signal[] }>(`/api/signals?limit=${limit}`);
  return data.signals;
}

export async function fetchTokens(limit = 100): Promise<TokenInfo[]> {
  const data = await get<{ tokens: TokenInfo[] }>(`/api/tokens?limit=${limit}`);
  return data.tokens;
}

export async function fetchTrades(limit = 50): Promise<TradeRecord[]> {
  const data = await get<{ trades: TradeRecord[] }>(`/api/trades?limit=${limit}`);
  return data.trades;
}

export async function fetchPositions(): Promise<Positions> {
  return get<Positions>("/api/positions");
}

export async function fetchSmartMoney(): Promise<string[]> {
  const data = await get<{ wallets: string[] }>("/api/smart-money");
  return data.wallets;
}

export async function fetchHealth(): Promise<Health> {
  return get<Health>("/api/health");
}

export async function fetchPerformance(): Promise<PerformanceStats> {
  const data = await get<{ performance: PerformanceStats }>("/api/performance");
  return data.performance;
}
