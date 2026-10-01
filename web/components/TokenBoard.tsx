import type { TokenInfo } from "@/lib/types";
import { fmtPrice, fmtUsd, shorten, timeAgo } from "@/lib/format";

export default function TokenBoard({
  tokens,
  onCopy,
}: {
  tokens: TokenInfo[];
  onCopy: (text: string) => void;
}) {
  return (
    <section>
      <h2 className="mb-2 flex items-center gap-2 px-1 font-mono text-xs font-bold uppercase tracking-widest text-zinc-400">
        <span className="inline-block h-1.5 w-1.5 rounded-full bg-cyan-400" />
        Token Board
        <span className="text-zinc-600">({tokens.length})</span>
      </h2>
      <div className="overflow-x-auto rounded-lg border border-white/10">
        <table className="w-full min-w-[56rem] border-collapse font-mono text-xs">
          <thead>
            <tr className="bg-white/[0.04] text-left text-[10px] uppercase tracking-wider text-zinc-500">
              <th className="px-3 py-2 font-medium">Token</th>
              <th className="px-3 py-2 font-medium">DEX</th>
              <th className="px-3 py-2 text-right font-medium">Price</th>
              <th className="px-3 py-2 text-right font-medium">Buy Vol</th>
              <th className="px-3 py-2 text-right font-medium">Sell Vol</th>
              <th className="px-3 py-2 font-medium">B/S</th>
              <th className="px-3 py-2 text-right font-medium">Traders</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 font-medium">Curve</th>
              <th className="px-3 py-2 text-right font-medium">Active</th>
            </tr>
          </thead>
          <tbody>
            {tokens.length === 0 && (
              <tr>
                <td
                  colSpan={10}
                  className="px-3 py-6 text-center text-zinc-600"
                >
                  waiting for tokens…
                </td>
              </tr>
            )}
            {tokens.map((t) => {
              const total = t.buy_volume_usd + t.sell_volume_usd;
              const buyPct = total > 0 ? (t.buy_volume_usd / total) * 100 : 50;
              return (
                <tr
                  key={t.mint}
                  className="border-t border-white/5 hover:bg-white/[0.03]"
                >
                  <td className="px-3 py-2">
                    <button
                      onClick={() => onCopy(t.mint)}
                      title={`${t.mint} (click to copy)`}
                      className="cursor-pointer text-left text-zinc-100 hover:text-cyan-300"
                    >
                      {t.symbol ? `$${t.symbol}` : shorten(t.mint, 5, 5)}
                      {t.name && (
                        <span className="ml-1.5 text-[10px] text-zinc-500">
                          {t.name}
                        </span>
                      )}
                    </button>
                  </td>
                  <td className="px-3 py-2 text-zinc-400">{t.dex}</td>
                  <td className="px-3 py-2 text-right text-zinc-200">
                    {fmtPrice(t.price_usd)}
                  </td>
                  <td className="px-3 py-2 text-right text-green-400">
                    {fmtUsd(t.buy_volume_usd)}
                  </td>
                  <td className="px-3 py-2 text-right text-red-400">
                    {fmtUsd(t.sell_volume_usd)}
                  </td>
                  <td className="px-3 py-2">
                    <div className="h-1.5 w-16 overflow-hidden rounded bg-red-500/40">
                      <div
                        className="h-full rounded bg-green-500"
                        style={{ width: `${buyPct}%` }}
                      />
                    </div>
                  </td>
                  <td className="px-3 py-2 text-right text-zinc-300">
                    {t.unique_traders}
                  </td>
                  <td className="px-3 py-2">
                    {t.graduated ? (
                      <span className="rounded border border-green-500/40 bg-green-500/10 px-1.5 py-0.5 text-[10px] font-bold text-green-400">
                        GRAD
                      </span>
                    ) : (
                      <span className="text-zinc-600">—</span>
                    )}
                  </td>
                  <td className="px-3 py-2">
                    {t.progress_pct != null ? (
                      <div className="flex items-center gap-1.5">
                        <div className="h-1.5 w-14 overflow-hidden rounded bg-white/10">
                          <div
                            className="h-full rounded bg-fuchsia-400"
                            style={{
                              width: `${Math.min(100, t.progress_pct)}%`,
                            }}
                          />
                        </div>
                        <span className="text-[10px] text-zinc-400">
                          {t.progress_pct.toFixed(0)}%
                        </span>
                      </div>
                    ) : (
                      <span className="text-zinc-600">—</span>
                    )}
                  </td>
                  <td className="px-3 py-2 text-right text-zinc-500">
                    {timeAgo(t.last_activity)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
