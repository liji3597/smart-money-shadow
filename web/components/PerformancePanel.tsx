import type { PerformanceStats } from "@/lib/types";
import { fmtPct, shorten } from "@/lib/format";

function pnlAccent(n: number | null | undefined): string {
  if (n == null || Number.isNaN(n)) return "text-zinc-100";
  if (n > 0) return "text-green-400";
  if (n < 0) return "text-red-400";
  return "text-zinc-100";
}

function Card({
  label,
  value,
  sub,
  accent,
}: {
  label: string;
  value: string;
  sub?: string;
  accent?: string;
}) {
  return (
    <div className="flex min-w-28 flex-col gap-0.5 rounded border border-white/10 bg-white/[0.03] px-3 py-2">
      <span className="text-[10px] uppercase tracking-widest text-zinc-500">
        {label}
      </span>
      <span
        className={`font-mono text-sm font-semibold ${accent ?? "text-zinc-100"}`}
      >
        {value}
      </span>
      {sub && (
        <span className="font-mono text-[10px] text-zinc-600">{sub}</span>
      )}
    </div>
  );
}

export default function PerformancePanel({
  perf,
}: {
  perf: PerformanceStats | null;
}) {
  const best = perf?.best_signal;
  const worst = perf?.worst_signal;
  return (
    <section className="px-4 pb-1">
      <h2 className="mb-2 flex items-center gap-2 px-1 font-mono text-xs font-bold uppercase tracking-widest text-zinc-400">
        <span className="inline-block h-1.5 w-1.5 rounded-full bg-cyan-400" />
        Signal Performance
      </h2>
      <div className="flex flex-wrap items-stretch gap-2">
        <Card
          label="Win Rate 1h"
          value={fmtPct(perf?.win_rate_1h_pct).replace("+", "")}
          sub={`${perf?.measured_1h ?? 0} measured`}
          accent={
            perf?.win_rate_1h_pct == null
              ? undefined
              : perf.win_rate_1h_pct >= 50
                ? "text-green-400"
                : "text-red-400"
          }
        />
        <Card
          label="Avg PnL 1h"
          value={fmtPct(perf?.avg_pnl_1h_pct)}
          accent={pnlAccent(perf?.avg_pnl_1h_pct)}
        />
        <Card
          label="Win Rate 24h"
          value={fmtPct(perf?.win_rate_24h_pct).replace("+", "")}
          sub={`${perf?.measured_24h ?? 0} measured`}
          accent={
            perf?.win_rate_24h_pct == null
              ? undefined
              : perf.win_rate_24h_pct >= 50
                ? "text-green-400"
                : "text-red-400"
          }
        />
        <Card
          label="Avg PnL 24h"
          value={fmtPct(perf?.avg_pnl_24h_pct)}
          accent={pnlAccent(perf?.avg_pnl_24h_pct)}
        />
        <Card
          label="Signals"
          value={`${perf?.total_signals ?? 0}`}
          sub={`${perf?.pending_signals ?? 0} pending`}
          accent="text-amber-300"
        />
        {best && (
          <Card
            label={`Best (${best.window})`}
            value={fmtPct(best.pnl_pct)}
            sub={best.symbol ? `$${best.symbol}` : shorten(best.mint)}
            accent="text-green-400"
          />
        )}
        {worst && (
          <Card
            label={`Worst (${worst.window})`}
            value={fmtPct(worst.pnl_pct)}
            sub={worst.symbol ? `$${worst.symbol}` : shorten(worst.mint)}
            accent="text-red-400"
          />
        )}
      </div>
    </section>
  );
}
