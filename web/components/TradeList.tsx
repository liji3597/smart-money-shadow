import type { TradeRecord } from "@/lib/types";
import { fmtSol, shorten, timeAgo } from "@/lib/format";

function modeBadge(mode: string): { label: string; cls: string } {
  switch (mode) {
    case "live":
      return {
        label: "LIVE",
        cls: "border-green-500/40 bg-green-500/10 text-green-400",
      };
    case "skipped_budget":
      return {
        label: "SKIPPED",
        cls: "border-yellow-500/40 bg-yellow-500/10 text-yellow-400",
      };
    case "dry_run":
    default:
      return {
        label: mode === "dry_run" ? "DRY" : mode.toUpperCase(),
        cls: "border-zinc-500/40 bg-zinc-500/10 text-zinc-400",
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
      <h2 className="mb-2 flex items-center gap-2 px-1 font-mono text-xs font-bold uppercase tracking-widest text-zinc-400">
        <span className="inline-block h-1.5 w-1.5 rounded-full bg-green-400" />
        Copy Trades
        <span className="text-zinc-600">({trades.length})</span>
      </h2>
      <div className="flex max-h-[46rem] flex-col gap-1.5 overflow-y-auto pr-1">
        {trades.length === 0 && (
          <div className="rounded-lg border border-dashed border-white/10 p-6 text-center font-mono text-xs text-zinc-600">
            no trades yet…
          </div>
        )}
        {trades.map((t, i) => {
          const badge = modeBadge(t.mode);
          return (
            <div
              key={`${t.signal_id}-${t.at}-${i}`}
              className="rounded border border-white/10 bg-white/[0.03] px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span
                  className={`rounded border px-1.5 py-0.5 font-mono text-[10px] font-bold ${badge.cls}`}
                >
                  {badge.label}
                </span>
                <span className="font-mono text-[11px] text-zinc-500">
                  {timeAgo(t.at)}
                </span>
              </div>
              <div className="mt-1 flex items-center justify-between font-mono text-xs">
                <button
                  onClick={() => onCopy(t.mint)}
                  title={`${t.mint} (click to copy)`}
                  className="cursor-pointer text-zinc-300 hover:text-cyan-300"
                >
                  {shorten(t.mint, 6, 6)}
                </button>
                <span className="font-semibold text-zinc-100">
                  {fmtSol(t.sol_amount)} SOL
                </span>
              </div>
              <div className="mt-0.5 flex items-center justify-between font-mono text-[10px] text-zinc-600">
                <span>
                  {t.signature ? (
                    <button
                      onClick={() => onCopy(t.signature!)}
                      title={`${t.signature} (click to copy)`}
                      className="cursor-pointer hover:text-cyan-300"
                    >
                      sig {shorten(t.signature, 5, 5)}
                    </button>
                  ) : (
                    "no sig"
                  )}
                  {t.landed_ms != null && ` · ${t.landed_ms}ms`}
                </span>
                {t.error && (
                  <span className="truncate text-red-400" title={t.error}>
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
