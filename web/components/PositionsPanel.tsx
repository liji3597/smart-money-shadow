import type { Position } from "@/lib/types";
import { fmtPct, fmtPrice, fmtSol, shorten, timeAgo } from "@/lib/format";

function floatingPnlPct(p: Position): number | null {
  if (!p.last_price_usd || p.entry_price_usd <= 0) return null;
  return (p.last_price_usd / p.entry_price_usd - 1) * 100;
}

function pnlCls(pct: number | null): string {
  if (pct == null) return "text-zinc-500";
  return pct >= 0 ? "text-green-400" : "text-red-400";
}

function reasonBadge(reason: string): string {
  switch (reason) {
    case "take_profit":
      return "border-green-500/40 bg-green-500/10 text-green-400";
    case "stop_loss":
    case "sell_failed":
      return "border-red-500/40 bg-red-500/10 text-red-400";
    default:
      return "border-zinc-500/40 bg-zinc-500/10 text-zinc-400";
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
      <h2 className="mb-2 flex items-center gap-2 px-1 font-mono text-xs font-bold uppercase tracking-widest text-zinc-400">
        <span className="inline-block h-1.5 w-1.5 rounded-full bg-cyan-400" />
        Positions
        <span className="text-zinc-600">({open.length} open)</span>
      </h2>
      <div className="flex max-h-[22rem] flex-col gap-1.5 overflow-y-auto pr-1">
        {open.length === 0 && closed.length === 0 && (
          <div className="rounded-lg border border-dashed border-white/10 p-6 text-center font-mono text-xs text-zinc-600">
            no positions yet…
          </div>
        )}
        {open.map((p) => {
          const pnl = floatingPnlPct(p);
          return (
            <div
              key={p.signal_id}
              className="rounded border border-cyan-500/20 bg-cyan-500/[0.04] px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <button
                  onClick={() => onCopy(p.mint)}
                  title={`${p.mint} (click to copy)`}
                  className="cursor-pointer font-mono text-xs text-zinc-300 hover:text-cyan-300"
                >
                  {p.symbol ?? shorten(p.mint, 6, 6)}
                </button>
                <span className={`font-mono text-xs font-bold ${pnlCls(pnl)}`}>
                  {fmtPct(pnl)}
                </span>
              </div>
              <div className="mt-1 flex items-center justify-between font-mono text-[10px] text-zinc-500">
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
            <div
              key={`${p.signal_id}-closed`}
              className="rounded border border-white/10 bg-white/[0.03] px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-mono text-xs text-zinc-400">
                  {p.symbol ?? shorten(p.mint, 6, 6)}
                </span>
                <span
                  className={`rounded border px-1.5 py-0.5 font-mono text-[10px] font-bold ${reasonBadge(st.reason)}`}
                >
                  {st.reason.replace(/_/g, " ").toUpperCase()}
                </span>
              </div>
              <div className="mt-1 flex items-center justify-between font-mono text-[10px] text-zinc-500">
                <span>
                  {fmtSol(p.sol_in)} SOL in
                  {st.exit_price_usd != null && ` · exit ${fmtPrice(st.exit_price_usd)}`}
                </span>
                <span className={pnl == null ? "text-zinc-500" : pnl >= 0 ? "text-green-400" : "text-red-400"}>
                  {pnl == null ? "pnl n/a" : `${pnl >= 0 ? "+" : "-"}${fmtSol(Math.abs(pnl))} SOL`}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
