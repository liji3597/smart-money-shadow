import type { Metrics, PerformanceStats } from "@/lib/types";
import { fmtInt, fmtPct, fmtUsd } from "@/lib/format";

function Item({
  label,
  value,
  accent,
  sub,
}: {
  label: string;
  value: string;
  accent?: string;
  sub?: string;
}) {
  return (
    <span className="flex shrink-0 items-baseline gap-1.5">
      <span className="text-[9px] uppercase tracking-widest text-zinc-500">
        {label}
      </span>
      <span
        className={`font-mono text-[11px] font-semibold ${accent ?? "text-zinc-200"}`}
      >
        {value}
      </span>
      {sub && (
        <span className="font-mono text-[9px] text-zinc-600">{sub}</span>
      )}
    </span>
  );
}

function pnlAccent(n: number | null | undefined): string | undefined {
  if (n == null || Number.isNaN(n)) return undefined;
  if (n > 0) return "text-green-400";
  if (n < 0) return "text-red-400";
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
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-white/10 px-4 py-1.5 lg:flex-nowrap lg:overflow-x-auto">
      <span className="flex shrink-0 items-center gap-1.5">
        <span className="text-[9px] uppercase tracking-widest text-zinc-500">
          Stream
        </span>
        <span
          className={`inline-block h-1.5 w-1.5 rounded-full ${
            connected
              ? "bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.9)]"
              : "bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.9)]"
          }`}
        />
        <span
          className={`font-mono text-[11px] font-semibold ${
            connected ? "text-green-400" : "text-red-400"
          }`}
        >
          {connected ? "LIVE" : "DOWN"}
        </span>
      </span>
      <Item label="Events/min" value={fmtInt(metrics?.events_per_min)} />
      <Item
        label="Volume"
        value={fmtUsd(metrics?.volume_usd_total)}
        accent="text-cyan-300"
      />
      <Item
        label="Signals"
        value={fmtInt(metrics?.signals_total)}
        accent="text-amber-300"
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
        accent={
          win1h == null
            ? "text-zinc-500"
            : win1h >= 50
              ? "text-green-400"
              : "text-red-400"
        }
      />
      <Item
        label="Avg 1H"
        value={fmtPct(perf?.avg_pnl_1h_pct)}
        accent={pnlAccent(perf?.avg_pnl_1h_pct) ?? "text-zinc-500"}
      />
      {metrics?.beam_last_latency_ms != null && (
        <Item
          label="Beam"
          value={`${metrics.beam_last_latency_ms}ms`}
          accent="text-fuchsia-300"
        />
      )}
    </div>
  );
}
