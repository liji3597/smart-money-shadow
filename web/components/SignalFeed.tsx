import type { Signal } from "@/lib/types";
import { fmtPct, fmtPrice, fmtUsd, shorten, timeAgo } from "@/lib/format";

function pnlCls(pct: number): string {
  if (pct > 0) return "text-buy";
  if (pct < 0) return "text-sell";
  return "text-ink-dim";
}

function riskColor(score: number): string {
  if (score < 30) return "var(--color-buy)";
  if (score <= 60) return "var(--color-warn)";
  return "var(--color-sell)";
}

function RiskMeter({ score, factors }: { score: number; factors: string[] }) {
  const segments = 5;
  const filled = Math.min(
    segments,
    Math.max(1, Math.round(score / (100 / segments))),
  );
  const color = riskColor(score);
  return (
    <span
      className="flex shrink-0 items-center gap-1.5"
      title={factors.length ? factors.join("\n") : "no risk factors"}
    >
      <span className="flex h-2 items-stretch gap-px">
        {Array.from({ length: segments }, (_, i) => (
          <span
            key={i}
            className="w-1 rounded-[1px]"
            style={{
              backgroundColor:
                i < filled ? color : "var(--color-line-strong)",
            }}
          />
        ))}
      </span>
      <span
        className="font-mono text-[11px] font-medium tabular-nums"
        style={{ color }}
      >
        {score}
      </span>
    </span>
  );
}

function triggerBadge(trigger: string): { label: string; cls: string } {
  switch (trigger) {
    case "smart_money_buy":
      return { label: "SMART MONEY", cls: "border-accent/40 text-accent" };
    case "volume_surge":
      return { label: "VOL SURGE", cls: "border-warn/40 text-warn" };
    default:
      return {
        label: trigger.replace(/_/g, " ").toUpperCase(),
        cls: "border-line-strong text-ink-dim",
      };
  }
}

function SignalRow({
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
    <article className={`px-3 py-3 ${flash ? "animate-flash" : ""}`}>
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <button
            onClick={() => onCopy(signal.mint)}
            title={`${signal.mint} (click to copy)`}
            className="block max-w-full cursor-pointer truncate text-left text-[15px] font-bold tracking-[-0.02em] text-ink hover:text-accent"
          >
            {signal.symbol
              ? `$${signal.symbol}`
              : signal.name || shorten(signal.mint, 6, 6)}
            {signal.symbol && signal.name && (
              <span className="ml-2 text-[13px] font-medium tracking-normal text-ink-dim">
                {signal.name}
              </span>
            )}
          </button>
          <div className="micro-label mt-1 font-mono">
            {signal.dex ? `${signal.dex} · ` : ""}
            {shorten(signal.mint)}
          </div>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1.5">
          <span className={`badge ${badge.cls}`}>{badge.label}</span>
          <span className="micro-label font-mono tabular-nums">
            {timeAgo(signal.created_at)}
          </span>
        </div>
      </div>

      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="flex items-baseline gap-2 font-mono text-[15px] font-medium tabular-nums text-ink">
          {fmtPrice(signal.price_usd)}
          {signal.pnl_1h_pct != null && (
            <span
              className={`text-[11px] tabular-nums ${pnlCls(signal.pnl_1h_pct)}`}
              title="signal PnL after 1h"
            >
              {fmtPct(signal.pnl_1h_pct)} 1h
            </span>
          )}
          {signal.pnl_24h_pct != null && (
            <span
              className={`text-[11px] tabular-nums ${pnlCls(signal.pnl_24h_pct)}`}
              title="signal PnL after 24h"
            >
              {fmtPct(signal.pnl_24h_pct)} 24h
            </span>
          )}
        </span>
        <RiskMeter score={signal.risk_score} factors={signal.risk_factors} />
      </div>

      <div className="mt-2.5">
        <div className="flex justify-between font-mono text-[11px] tabular-nums">
          <span className="text-buy">
            {fmtUsd(signal.buy_volume_usd)} ({signal.buy_count})
          </span>
          <span className="text-sell">
            {fmtUsd(signal.sell_volume_usd)} ({signal.sell_count})
          </span>
        </div>
        <div className="mt-1 h-1 w-full overflow-hidden rounded-[2px] bg-sell/30">
          <div
            className="h-full rounded-[2px] bg-buy transition-all duration-500"
            style={{ width: `${buyPct}%` }}
          />
        </div>
      </div>

      <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] tabular-nums text-ink-dim">
        <span>{signal.unique_traders} traders</span>
        {signal.top10_holder_pct != null && (
          <span
            className={
              signal.top10_holder_pct > 50 ? "text-sell" : "text-ink-dim"
            }
          >
            top10 {signal.top10_holder_pct.toFixed(1)}%
          </span>
        )}
        {signal.trigger_wallets.length > 0 && (
          <span className="flex items-center gap-1.5">
            <span>wallets:</span>
            {signal.trigger_wallets.slice(0, 3).map((w) => (
              <button
                key={w}
                onClick={() => onCopy(w)}
                title={`${w} (click to copy)`}
                className="cursor-pointer text-accent/80 hover:text-accent"
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
      <h2 className="micro-label mb-2 mt-1 px-1">
        Live Signals{" "}
        <span className="font-mono tabular-nums">({signals.length})</span>
      </h2>
      <div className="panel max-h-[46rem] overflow-y-auto">
        {signals.length === 0 && (
          <div className="p-6 text-center font-mono text-xs text-ink-dim">
            waiting for signals…
          </div>
        )}
        <div className="flex flex-col divide-y divide-line">
          {signals.map((s) => (
            <SignalRow
              key={s.id}
              signal={s}
              flash={flashIds.has(s.id)}
              onCopy={onCopy}
            />
          ))}
        </div>
      </div>
    </section>
  );
}
