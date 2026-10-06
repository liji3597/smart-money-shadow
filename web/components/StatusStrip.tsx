import type { Metrics, PerformanceStats } from "@/lib/types";
import { fmtInt, fmtPct, fmtUsd } from "@/lib/format";

function Item({
  label,
  value,
  valueCls,
  sub,
}: {
  label: string;
  value: string;
  valueCls?: string;
  sub?: string;
}) {
  return (
    <span className="flex shrink-0 flex-col justify-center gap-0.5 px-4 py-2 first:pl-0">
      <span className="micro-label">{label}</span>
      <span className="flex items-baseline gap-1.5">
        <span
          className={`font-mono text-[13px] font-medium tabular-nums ${valueCls ?? "text-ink"}`}
        >
          {value}
        </span>
        {sub && (
          <span className="font-mono text-[10px] tabular-nums text-ink-dim">
            {sub}
          </span>
        )}
      </span>
    </span>
  );
}

function pnlCls(n: number | null | undefined): string | undefined {
  if (n == null || Number.isNaN(n)) return undefined;
  if (n > 0) return "text-buy";
  if (n < 0) return "text-sell";
  return undefined;
}

export default function StatusStrip({
  metrics,
  perf,
}: {
  metrics: Metrics | null;
  perf: PerformanceStats | null;
}) {
  const connected = metrics?.stream_connected ?? false;
  const win1h = perf?.win_rate_1h_pct;
  return (
    <div className="flex flex-wrap items-stretch divide-x divide-line border-b border-line bg-panel-2 px-4 lg:flex-nowrap lg:overflow-x-auto">
      <span className="flex shrink-0 flex-col justify-center gap-0.5 px-4 py-2 first:pl-0">
        <span className="micro-label">Stream</span>
        <span className="flex items-center gap-1.5">
          <span
            className={`inline-block h-1.5 w-1.5 rounded-full ${
              connected ? "bg-buy animate-live" : "bg-sell"
            }`}
          />
          <span
            className={`font-mono text-[13px] font-medium ${
              connected ? "text-buy" : "text-sell"
            }`}
          >
            {connected ? "LIVE" : "DOWN"}
          </span>
        </span>
      </span>
      <Item label="Events/min" value={fmtInt(metrics?.events_per_min)} />
      <Item label="Volume" value={fmtUsd(metrics?.volume_usd_total)} />
      <Item
        label="Signals"
        value={fmtInt(metrics?.signals_total)}
        sub={
          metrics?.signals_filtered_total != null
            ? `+${fmtInt(metrics.signals_filtered_total)} filtered`
            : undefined
        }
      />
      <Item label="Smart Money" value={fmtInt(metrics?.smart_money_count)} />
      <Item label="Tracked" value={fmtInt(metrics?.tracked_tokens)} />
      <Item
        label="1H Win"
        value={win1h == null ? "—" : fmtPct(win1h).replace("+", "")}
        valueCls={
          win1h == null
            ? "text-ink-dim"
            : win1h >= 50
              ? "text-buy"
              : "text-sell"
        }
      />
      <Item
        label="Avg 1H"
        value={fmtPct(perf?.avg_pnl_1h_pct)}
        valueCls={pnlCls(perf?.avg_pnl_1h_pct) ?? "text-ink-dim"}
      />
      {metrics?.beam_last_latency_ms != null && (
        <Item
          label="Beam"
          value={`${metrics.beam_last_latency_ms}ms`}
          valueCls="text-accent"
        />
      )}
      <Item
        label="Wallet"
        value={
          metrics?.wallet_balance_sol != null && metrics.wallet_balance_sol > 0
            ? metrics.wallet_balance_sol.toFixed(4)
            : "—"
        }
        valueCls={
          metrics?.wallet_balance_sol != null && metrics.wallet_balance_sol > 0
            ? undefined
            : "text-ink-dim"
        }
        sub={
          metrics?.wallet_balance_sol != null && metrics.wallet_balance_sol > 0
            ? "SOL"
            : undefined
        }
      />
    </div>
  );
}
