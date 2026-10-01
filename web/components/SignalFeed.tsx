import type { Signal } from "@/lib/types";
import { fmtPct, fmtPrice, fmtUsd, shorten, timeAgo } from "@/lib/format";

function PnlBadge({ pct, window }: { pct: number; window: "1h" | "24h" }) {
  const cls =
    pct > 0
      ? "border-green-500/40 bg-green-500/10 text-green-400"
      : pct < 0
        ? "border-red-500/40 bg-red-500/10 text-red-400"
        : "border-zinc-500/40 bg-zinc-500/15 text-zinc-300";
  return (
    <span
      className={`rounded border px-1.5 py-0.5 text-[11px] font-bold ${cls}`}
      title={`signal PnL after ${window}`}
    >
      {fmtPct(pct)} {window}
    </span>
  );
}

function riskClasses(score: number): string {
  if (score < 30) return "border-green-500/40 bg-green-500/10 text-green-400";
  if (score <= 60) return "border-yellow-500/40 bg-yellow-500/10 text-yellow-400";
  return "border-red-500/40 bg-red-500/10 text-red-400";
}

function triggerBadge(trigger: string): { label: string; cls: string } {
  switch (trigger) {
    case "smart_money_buy":
      return {
        label: "SMART MONEY",
        cls: "border-indigo-400/40 bg-indigo-500/15 text-indigo-300",
      };
    case "volume_surge":
      return {
        label: "VOL SURGE",
        cls: "border-amber-400/40 bg-amber-500/15 text-amber-300",
      };
    default:
      return {
        label: trigger.replace(/_/g, " ").toUpperCase(),
        cls: "border-zinc-500/40 bg-zinc-500/15 text-zinc-300",
      };
  }
}

function SignalCard({
  signal,
  flash,
  onCopy,
}: {
  signal: Signal;
  flash: boolean;
  onCopy: (text: string) => void;
}) {
  const total = signal.buy_volume_usd + signal.sell_volume_usd;
  const buyPct = total > 0 ? (signal.buy_volume_usd / total) * 100 : 50;
  const badge = triggerBadge(signal.trigger);
  return (
    <article
      className={`rounded-lg border border-white/10 bg-white/[0.03] p-3 ${
        flash ? "animate-flash" : ""
      }`}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <button
            onClick={() => onCopy(signal.mint)}
            title={`${signal.mint} (click to copy)`}
            className="block max-w-full cursor-pointer truncate text-left font-mono text-sm font-semibold text-zinc-100 hover:text-cyan-300"
          >
            {signal.symbol
              ? `$${signal.symbol}`
              : signal.name || shorten(signal.mint, 6, 6)}
            {signal.symbol && signal.name && (
              <span className="ml-2 font-sans text-xs font-normal text-zinc-500">
                {signal.name}
              </span>
            )}
          </button>
          <div className="mt-0.5 font-mono text-[11px] text-zinc-500">
            {signal.dex} · {shorten(signal.mint)}
          </div>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1">
          <span
            className={`rounded border px-1.5 py-0.5 font-mono text-[10px] font-bold tracking-wide ${badge.cls}`}
          >
            {badge.label}
          </span>
          <span className="font-mono text-[11px] text-zinc-500">
            {timeAgo(signal.created_at)}
          </span>
        </div>
      </div>

      <div className="mt-2 flex items-center justify-between font-mono text-xs">
        <span className="flex items-center gap-1.5 text-zinc-300">
          {fmtPrice(signal.price_usd)}
          {signal.pnl_1h_pct != null && (
            <PnlBadge pct={signal.pnl_1h_pct} window="1h" />
          )}
          {signal.pnl_24h_pct != null && (
            <PnlBadge pct={signal.pnl_24h_pct} window="24h" />
          )}
        </span>
        <span
          className={`rounded border px-1.5 py-0.5 text-[11px] font-bold ${riskClasses(
            signal.risk_score,
          )}`}
          title={
            signal.risk_factors.length
              ? signal.risk_factors.join("\n")
              : "no risk factors"
          }
        >
          RISK {signal.risk_score}
        </span>
      </div>

      <div className="mt-2">
        <div className="flex justify-between font-mono text-[11px]">
          <span className="text-green-400">
            ↑ {fmtUsd(signal.buy_volume_usd)} ({signal.buy_count})
          </span>
          <span className="text-red-400">
            ↓ {fmtUsd(signal.sell_volume_usd)} ({signal.sell_count})
          </span>
        </div>
        <div className="mt-1 h-1.5 w-full overflow-hidden rounded bg-red-500/40">
          <div
            className="h-full rounded bg-green-500 transition-all duration-500"
            style={{ width: `${buyPct}%` }}
          />
        </div>
      </div>

      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] text-zinc-500">
        <span>{signal.unique_traders} traders</span>
        {signal.top10_holder_pct != null && (
          <span
            className={
              signal.top10_holder_pct > 50 ? "text-red-400" : "text-zinc-400"
            }
          >
            top10 {signal.top10_holder_pct.toFixed(1)}%
          </span>
        )}
        {signal.trigger_wallets.length > 0 && (
          <span className="flex items-center gap-1">
            <span>wallets:</span>
            {signal.trigger_wallets.slice(0, 3).map((w) => (
              <button
                key={w}
                onClick={() => onCopy(w)}
                title={`${w} (click to copy)`}
                className="cursor-pointer rounded bg-white/5 px-1 py-0.5 text-indigo-300 hover:bg-white/10"
              >
                {shorten(w)}
              </button>
            ))}
            {signal.trigger_wallets.length > 3 && (
              <span>+{signal.trigger_wallets.length - 3}</span>
            )}
          </span>
        )}
      </div>
    </article>
  );
}

export default function SignalFeed({
  signals,
  flashIds,
  onCopy,
}: {
  signals: Signal[];
  flashIds: ReadonlySet<string>;
  onCopy: (text: string) => void;
}) {
  return (
    <section className="flex min-h-0 flex-col">
      <h2 className="mb-2 flex items-center gap-2 px-1 font-mono text-xs font-bold uppercase tracking-widest text-zinc-400">
        <span className="inline-block h-1.5 w-1.5 rounded-full bg-amber-400" />
        Live Signals
        <span className="text-zinc-600">({signals.length})</span>
      </h2>
      <div className="flex max-h-[46rem] flex-col gap-2 overflow-y-auto pr-1">
        {signals.length === 0 && (
          <div className="rounded-lg border border-dashed border-white/10 p-6 text-center font-mono text-xs text-zinc-600">
            waiting for signals…
          </div>
        )}
        {signals.map((s) => (
          <SignalCard
            key={s.id}
            signal={s}
            flash={flashIds.has(s.id)}
            onCopy={onCopy}
          />
        ))}
      </div>
    </section>
  );
}
