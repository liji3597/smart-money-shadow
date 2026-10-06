import type { TradeRecord } from "@/lib/types";
import { fmtSol, shorten, timeAgo } from "@/lib/format";

function modeBadge(mode: string): { label: string; cls: string } {
  switch (mode) {
    case "live":
      return { label: "LIVE", cls: "border-buy/40 text-buy" };
    case "skipped_budget":
      return { label: "SKIPPED", cls: "border-warn/40 text-warn" };
    case "dry_run":
      return { label: "DRY", cls: "border-warn/40 text-warn" };
    default:
      return {
        label: mode.toUpperCase(),
        cls: "border-line-strong text-ink-dim",
      };
  }
}

export default function TradeList({
  trades,
  onCopy,
}: {
  trades: TradeRecord[];
  onCopy: (text: string) => void;
}) {
  return (
    <section className="flex min-h-0 flex-col">
      <h2 className="micro-label mb-2 mt-1 px-1">
        Copy Trades{" "}
        <span className="font-mono tabular-nums">({trades.length})</span>
      </h2>
      <div className="panel max-h-[46rem] divide-y divide-line overflow-y-auto">
        {trades.length === 0 && (
          <div className="p-6 text-center font-mono text-xs text-ink-dim">
            no trades yet…
          </div>
        )}
        {trades.map((t, i) => {
          const badge = modeBadge(t.mode);
          return (
            <div key={`${t.signal_id}-${t.at}-${i}`} className="px-3 py-2">
              <div className="flex items-center justify-between gap-2">
                <span className={`badge ${badge.cls}`}>{badge.label}</span>
                <span className="font-mono text-[11px] tabular-nums text-ink-dim">
                  {timeAgo(t.at)}
                </span>
              </div>
              <div className="mt-1.5 flex items-center justify-between font-mono text-xs">
                <button
                  onClick={() => onCopy(t.mint)}
                  title={`${t.mint} (click to copy)`}
                  className="cursor-pointer tabular-nums text-ink-dim hover:text-accent"
                >
                  {shorten(t.mint, 6, 6)}
                </button>
                <span className="font-medium tabular-nums text-ink">
                  {fmtSol(t.sol_amount)} SOL
                </span>
              </div>
              <div className="mt-1 flex items-center justify-between font-mono text-[10px] tabular-nums text-ink-dim">
                <span>
                  {t.signature ? (
                    <button
                      onClick={() => onCopy(t.signature!)}
                      title={`${t.signature} (click to copy)`}
                      className="cursor-pointer hover:text-accent"
                    >
                      sig {shorten(t.signature, 5, 5)}
                    </button>
                  ) : (
                    "no sig"
                  )}
                  {t.landed_ms != null && ` · ${t.landed_ms}ms`}
                </span>
                {t.error && (
                  <span className="truncate text-sell" title={t.error}>
                    err: {t.error}
                  </span>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
