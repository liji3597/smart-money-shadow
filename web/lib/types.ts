export interface Metrics {
  started_at: number;
  events_total: number;
  swaps_total: number;
  token_creates_total: number;
  pool_creates_total: number;
  graduations_total: number;
  volume_usd_total: number;
  signals_total: number;
  trades_total: number;
  smart_money_count: number;
  tracked_tokens: number;
  events_per_min: number;
  stream_connected: boolean;
  stream_reconnects: number;
  last_event_slot: number;
  beam_last_latency_ms: number | null;
  signals_filtered_total?: number;
}

export interface Signal {
  id: string;
  mint: string;
  symbol?: string;
  name?: string;
  dex: string;
  trigger: string; // "smart_money_buy" | "volume_surge"
  trigger_wallets: string[];
  price_usd: number;
  buy_volume_usd: number;
  sell_volume_usd: number;
  buy_count: number;
  sell_count: number;
  unique_traders: number;
  risk_score: number; // 0 = clean, 100 = likely rug
  risk_factors: string[];
  top10_holder_pct?: number;
  created_at: number; // unix seconds
  // merged in by the API from the PnL tracker (present once backfilled)
  current_price_usd?: number;
  pnl_1h_pct?: number;
  pnl_24h_pct?: number;
}

export interface PerfPeak {
  signal_id: string;
  mint: string;
  symbol?: string;
  pnl_pct: number;
  window: "1h" | "24h";
}

export interface PerformanceStats {
  total_signals: number;
  pending_signals: number;
  measured_1h: number;
  win_rate_1h_pct?: number;
  avg_pnl_1h_pct?: number;
  measured_24h: number;
  win_rate_24h_pct?: number;
  avg_pnl_24h_pct?: number;
  best_signal?: PerfPeak;
  worst_signal?: PerfPeak;
}

export interface TokenInfo {
  mint: string;
  name?: string;
  symbol?: string;
  dex: string;
  pool?: string;
  creator?: string;
  created_at?: number;
  price_usd: number;
  buy_volume_usd: number;
  sell_volume_usd: number;
  buy_count: number;
  sell_count: number;
  unique_traders: number;
  graduated: boolean;
  progress_pct?: number;
  last_activity: number;
}

export interface TradeRecord {
  signal_id: string;
  mint: string;
  mode: string; // "dry_run" | "dry_sell" | "live" | "live_sell" | "skipped_*" | "live_error"
  sol_amount: number;
  signature?: string;
  landed_ms?: number;
  error?: string;
  realized_pnl_sol?: number;
  reason?: string;
  at: number;
}

export interface Position {
  signal_id: string;
  mint: string;
  symbol?: string;
  dex: string;
  entry_price_usd: number;
  sol_in: number;
  tokens: number; // 0 = PnL tracked by price ratio only
  opened_at: number;
  last_price_usd?: number;
  status:
    | { state: "open" }
    | {
        state: "closed";
        reason: string;
        exit_price_usd?: number | null;
        pnl_sol?: number | null;
        closed_at: number;
      };
}

export interface Positions {
  open: Position[];
  closed: Position[];
}

// DexEvent tagged union (data.type ∈ swap|token_create|pool_create|meme|graduation|surge|other).
// Kept as one flat interface with optional fields so unknown variants still typecheck.
export interface DexEvent {
  type: string;
  signature?: string;
  slot?: number;
  block_time?: number;
  dex?: string;
  pool?: string;
  mint?: string;
  quote_mint?: string;
  trader?: string;
  side?: "buy" | "sell";
  base_amount?: number;
  quote_amount?: number;
  price_usd?: number;
  volume_usd?: number;
  price_impact_pct?: number;
  name?: string;
  symbol?: string;
  uri?: string;
  creator?: string;
  trigger_time?: number;
  mcap_at_trigger?: number;
  price_at_trigger?: number;
  volume_window_usd?: number;
  multiple?: number;
  trades?: number;
  window_secs?: number;
}

export interface Snapshot {
  metrics: Metrics;
  signals: Signal[];
  trades: TradeRecord[];
  positions: Positions;
}

export type WsMessage =
  | { kind: "snapshot"; data: Snapshot }
  | { kind: "event"; data: DexEvent }
  | { kind: "signal"; data: Signal }
  | { kind: "trade"; data: TradeRecord }
  | { kind: "position"; data: Position };

export interface Health {
  ok: boolean;
  stream_connected: boolean;
  uptime_secs: number;
}
