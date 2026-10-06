import type { Position } from "@/lib/types";
import { fmtPct, fmtPrice, fmtSol, shorten, timeAgo } from "@/lib/format";

function floatingPnlPct(p: Position): number | null {
  if (!p.last_price_usd || p.entry_price_usd <= 0) return null;
  return (p.last_price_usd / p.entry_price_usd - 1) * 100;
}

function pnlCls(pct: number | null): string {
  if (pct == null) return "text-ink-dim";
  return pct >= 0 ? "text-buy" : "text-sell";
}

function reasonBadge(reason: string): string {
  switch (reason) {
    case "take_profit":
      return "border-buy/40 text-buy";
    case "stop_loss":
    case "sell_failed":
      return "border-sell/40 text-sell";
    default:
      return "border-line-strong text-ink-dim";
  }
}

export default function PositionsPanel({
  open,
  closed,
  onCopy,
}: {
  open: Position[];
  closed: Position[];
  onCopy: (text: string) => void;
}) {
  return (
    <section className="flex min-h-0 flex-col">
      <h2 className="micro-label mb-2 mt-1 px-1">
        Positions{" "}
        <span className="font-mono tabular-nums">({open.length} open)</span>
      </h2>
      <div className="panel max-h-[22rem] divide-y divide-line overflow-y-auto">
        {open.length === 0 && closed.length === 0 && (
          <div className="p-6 text-center font-mono text-xs text-ink-dim">
            no open positions — exits arm automatically after the next entry
          </div>
        )}
        {open.map((p) => {
          const pnl = floatingPnlPct(p);
          return (
            <div key={p.signal_id} className="px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-buy" />
                  <button
                    onClick={() => onCopy(p.mint)}
                    title={`${p.mint} (click to copy)`}
                    className="cursor-pointer truncate font-mono text-xs text-ink hover:text-accent"
                  >
                    {p.symbol ?? shorten(p.mint, 6, 6)}
                  </button>
                </span>
                <span
                  className={`font-mono text-xs font-medium tabular-nums ${pnlCls(pnl)}`}
                >
                  {fmtPct(pnl)}
                </span>
              </div>
              <div className="mt-1 flex items-center justify-between font-mono text-[10px] tabular-nums text-ink-dim">
                <span>
                  entry {fmtPrice(p.entry_price_usd)}
                  {p.last_price_usd != null && ` → ${fmtPrice(p.last_price_usd)}`}
                </span>
                <span>
                  {fmtSol(p.sol_in)} SOL · {timeAgo(p.opened_at)}
                </span>
              </div>
            </div>
          );
        })}
        {closed.slice(0, 20).map((p) => {
          const st = p.status;
          if (st.state !== "closed") return null;
          const pnl = st.pnl_sol ?? null;
          return (
            <div key={`${p.signal_id}-closed`} className="px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <span className="truncate font-mono text-xs text-ink-dim">
                  {p.symbol ?? shorten(p.mint, 6, 6)}
                </span>
                <span className={`badge ${reasonBadge(st.reason)}`}>
                  {st.reason.replace(/_/g, " ").toUpperCase()}
                </span>
              </div>
              <div className="mt-1 flex items-center justify-between font-mono text-[10px] tabular-nums text-ink-dim">
                <span>
                  {fmtSol(p.sol_in)} SOL in
                  {st.exit_price_usd != null && ` · exit ${fmtPrice(st.exit_price_usd)}`}
                </span>
                <span
                  className={
                    pnl == null
                      ? "text-ink-dim"
                      : pnl >= 0
                        ? "text-buy"
                        : "text-sell"
                  }
                >
                  {pnl == null
                    ? "pnl n/a"
                    : `${pnl >= 0 ? "+" : "-"}${fmtSol(Math.abs(pnl))} SOL`}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
