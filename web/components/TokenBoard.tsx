import { useMemo } from "react";
import type { TokenInfo } from "@/lib/types";
import { fmtPrice, shorten } from "@/lib/format";

// Sort rows by 10-second activity buckets instead of raw timestamps: a token
// only changes position when it crosses a bucket boundary, so rows stay put
// while their numbers refresh. JS sort is stable, so tokens inside the same
// bucket keep their previous relative order.
const BUCKET_SECS = 10;
const bucketOf = (t: TokenInfo) => Math.floor(t.last_activity / BUCKET_SECS);

function riskCls(score: number): string {
  if (score < 30) return "text-green-400";
  if (score <= 60) return "text-yellow-400";
  return "text-red-400";
}

export default function TokenBoard({
  tokens,
  riskByMint,
  onCopy,
}: {
  tokens: TokenInfo[];
  riskByMint: ReadonlyMap<string, number>;
  onCopy: (text: string) => void;
}) {
  const rows = useMemo(
    () => [...tokens].sort((a, b) => bucketOf(b) - bucketOf(a)),
    [tokens],
  );
  return (
    <section className="flex min-h-0 flex-col">
      <h2 className="mb-2 flex items-center gap-2 px-1 font-mono text-xs font-bold uppercase tracking-widest text-zinc-400">
        <span className="inline-block h-1.5 w-1.5 rounded-full bg-cyan-400" />
        Token Board
        <span className="text-zinc-600">({tokens.length})</span>
      </h2>
      <div className="rounded-lg border border-white/10">
        <div className="flex items-center gap-2 border-b border-white/10 bg-white/[0.04] px-2 py-1.5 font-mono text-[9px] uppercase tracking-wider text-zinc-500">
          <span className="min-w-0 flex-1">Token</span>
          <span className="w-16 text-right">Price</span>
          <span className="w-12 text-center">B/S</span>
          <span className="w-7 text-right">Rsk</span>
        </div>
        <div className="max-h-[46rem] overflow-y-auto">
          {rows.length === 0 && (
            <div className="px-2 py-6 text-center font-mono text-[11px] text-zinc-600">
              waiting for tokens…
            </div>
          )}
          {rows.map((t) => {
            const total = t.buy_volume_usd + t.sell_volume_usd;
            const buyPct = total > 0 ? (t.buy_volume_usd / total) * 100 : 50;
            const risk = riskByMint.get(t.mint);
            return (
              <div
                key={t.mint}
                className="flex items-center gap-2 border-t border-white/5 px-2 py-1 font-mono text-[11px] hover:bg-white/[0.03]"
              >
                <button
                  onClick={() => onCopy(t.mint)}
                  title={`${t.mint} (click to copy)`}
                  className="min-w-0 flex-1 cursor-pointer truncate text-left text-zinc-100 hover:text-cyan-300"
                >
                  {t.graduated && (
                    <span
                      className="mr-1 inline-block h-1.5 w-1.5 rounded-full bg-green-400 align-middle"
                      title="graduated"
                    />
                  )}
                  {t.symbol ? `$${t.symbol}` : shorten(t.mint, 4, 4)}
                </button>
                <span className="w-16 shrink-0 truncate text-right text-zinc-300">
                  {fmtPrice(t.price_usd)}
                </span>
                <span className="w-12 shrink-0">
                  <span className="block h-1 w-full overflow-hidden rounded bg-red-500/40">
                    <span
                      className="block h-full rounded bg-green-500"
                      style={{ width: `${buyPct}%` }}
                    />
                  </span>
                </span>
                <span
                  className={`w-7 shrink-0 text-right ${
                    risk == null ? "text-zinc-700" : riskCls(risk)
                  }`}
                >
                  {risk ?? "—"}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
