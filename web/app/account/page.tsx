"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { fetchMetrics, fetchPositions, fetchTrades } from "@/lib/api";
import type { Metrics, Positions, TradeRecord } from "@/lib/types";
import { fmtPct, fmtPrice, fmtSol, shorten } from "@/lib/format";
import EquityCurve from "@/components/account/EquityCurve";
import CandleChart from "@/components/account/CandleChart";

const POLL_MS = 30_000;

function Stat({
  label,
  value,
  valueCls,
  sub,
}: {
  label: string;
  value: string;
  valueCls?: string;
  sub?: string;
}) {
  return (
    <span className="flex shrink-0 flex-col justify-center gap-0.5 px-4 py-2 first:pl-0">
      <span className="micro-label">{label}</span>
      <span className="flex items-baseline gap-1.5">
        <span
          className={`font-mono text-[13px] font-medium tabular-nums ${valueCls ?? "text-ink"}`}
        >
          {value}
        </span>
        {sub && (
          <span className="font-mono text-[10px] tabular-nums text-ink-dim">
            {sub}
          </span>
        )}
      </span>
    </span>
  );
}

function pnlCls(n: number | null | undefined): string {
  if (n == null) return "text-ink-dim";
  return n >= 0 ? "text-buy" : "text-sell";
}

function fmtSignedSol(n: number | null | undefined): string {
  if (n == null) return "—";
  return `${n >= 0 ? "+" : "-"}${fmtSol(Math.abs(n))}`;
}

function fmtDuration(secs: number): string {
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  if (secs < 86_400) return `${(secs / 3600).toFixed(1)}h`;
  return `${(secs / 86_400).toFixed(1)}d`;
}

function fmtTime(at: number): string {
  return new Date(at * 1000).toLocaleString("en-GB", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function reasonBadge(reason: string | null): { label: string; cls: string } {
  switch (reason) {
    case "take_profit":
      return { label: "TAKE PROFIT", cls: "border-buy/40 text-buy" };
    case "stop_loss":
      return { label: "STOP LOSS", cls: "border-sell/40 text-sell" };
    case "time_stop":
      return { label: "TIME STOP", cls: "border-warn/40 text-warn" };
    case null:
      return { label: "OPEN", cls: "border-accent/40 text-accent" };
    default:
      return {
        label: reason.replace(/_/g, " ").toUpperCase(),
        cls: "border-line-strong text-ink-dim",
      };
  }
}

interface Round {
  signalId: string;
  mint: string;
  at: number; // sell time when closed, buy time when still open
  buyAt: number;
  solIn: number;
  solOut: number | null;
  reason: string | null;
  pnlSol: number | null;
  buySig?: string;
  sellSig?: string;
}

/** Pair live buys with live sells by signal_id; sells close the round. */
function buildRounds(trades: TradeRecord[]): Round[] {
  const buys = new Map<string, TradeRecord>();
  const sells = new Map<string, TradeRecord>();
  for (const t of trades) {
    if (t.mode === "live") buys.set(t.signal_id, t);
    else if (t.mode === "live_sell") sells.set(t.signal_id, t);
  }
  const rounds: Round[] = [];
  for (const [id, buy] of buys) {
    const sell = sells.get(id);
    rounds.push({
      signalId: id,
      mint: buy.mint,
      at: sell?.at ?? buy.at,
      buyAt: buy.at,
      solIn: buy.sol_amount,
      solOut: sell?.sol_amount ?? null,
      reason: sell?.reason ?? null,
      pnlSol: sell?.realized_pnl_sol ?? null,
      buySig: buy.signature,
      sellSig: sell?.signature,
    });
    if (sell) sells.delete(id);
  }
  // orphan sells (buy record aged out of the window) still get listed
  for (const [id, sell] of sells) {
    rounds.push({
      signalId: id,
      mint: sell.mint,
      at: sell.at,
      buyAt: sell.at,
      solIn: sell.sol_amount,
      solOut: sell.sol_amount,
      reason: sell.reason ?? null,
      pnlSol: sell.realized_pnl_sol ?? null,
      sellSig: sell.signature,
    });
  }
  return rounds.sort((a, b) => b.at - a.at);
}

export default function AccountPage() {
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [trades, setTrades] = useState<TradeRecord[] | null>(null);
  const [positions, setPositions] = useState<Positions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const [m, t, p] = await Promise.all([
          fetchMetrics(),
          fetchTrades(500),
          fetchPositions(),
        ]);
        if (!alive) return;
        setMetrics(m);
        setTrades(t);
        setPositions(p);
        setError(null);
        setNow(Date.now());
      } catch (e) {
        if (alive) setError(e instanceof Error ? e.message : "load failed");
      }
    };
    load();
    const t = window.setInterval(load, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, []);

  const loading = trades == null && error == null;

  // stats: closed live rounds only (live_sell records carry realized pnl)
  const closed = (trades ?? []).filter(
    (t) => t.mode === "live_sell" && t.realized_pnl_sol != null,
  );
  const todayStart = Math.floor(now / 1000 / 86_400) * 86_400; // 00:00 UTC
  const totalPnl = closed.reduce((s, t) => s + (t.realized_pnl_sol ?? 0), 0);
  const todayPnl = closed
    .filter((t) => t.at >= todayStart)
    .reduce((s, t) => s + (t.realized_pnl_sol ?? 0), 0);
  const wins = closed.filter((t) => (t.realized_pnl_sol ?? 0) > 0);
  const losses = closed.filter((t) => (t.realized_pnl_sol ?? 0) < 0);
  const winRate = closed.length > 0 ? (wins.length / closed.length) * 100 : null;
  const grossWin = wins.reduce((s, t) => s + (t.realized_pnl_sol ?? 0), 0);
  const grossLoss = Math.abs(losses.reduce((s, t) => s + (t.realized_pnl_sol ?? 0), 0));
  const profitFactor =
    closed.length === 0
      ? null
      : grossLoss > 0
        ? grossWin / grossLoss
        : grossWin > 0
          ? Infinity
          : null;

  const rounds = buildRounds(trades ?? []);
  const open = positions?.open ?? [];

  // equity curve: cumulative realized pnl, chronological
  const equity = [...closed]
    .sort((a, b) => a.at - b.at)
    .reduce<{ t: number; v: number }[]>((acc, t) => {
      acc.push({ t: t.at, v: (acc.at(-1)?.v ?? 0) + (t.realized_pnl_sol ?? 0) });
      return acc;
    }, []);

  // trade facts — all derived from the closed set
  const matched = rounds.filter((r) => r.reason != null && r.at > r.buyAt);
  const avgHold =
    matched.length > 0
      ? matched.reduce((s, r) => s + (r.at - r.buyAt), 0) / matched.length
      : null;
  const best = closed.reduce<TradeRecord | null>(
    (b, t) => (b == null || (t.realized_pnl_sol ?? 0) > (b.realized_pnl_sol ?? 0) ? t : b),
    null,
  );
  const worst = closed.reduce<TradeRecord | null>(
    (w, t) => (w == null || (t.realized_pnl_sol ?? 0) < (w.realized_pnl_sol ?? 0) ? t : w),
    null,
  );
  const tpCount = closed.filter((t) => t.reason === "take_profit").length;
  const tpShare = closed.length > 0 ? (tpCount / closed.length) * 100 : null;
  const facts: string[] = [];
  if (avgHold != null) facts.push(`avg hold ${fmtDuration(avgHold)}`);
  if (best) facts.push(`best ${fmtSignedSol(best.realized_pnl_sol)} ${best.reason ?? ""}`.trim());
  if (worst) facts.push(`worst ${fmtSignedSol(worst.realized_pnl_sol)} ${worst.reason ?? ""}`.trim());
  if (tpShare != null) facts.push(`${fmtPct(tpShare).replace("+", "")} of exits hit take-profit`);

  const balance =
    metrics?.wallet_balance_sol != null && metrics.wallet_balance_sol > 0
      ? metrics.wallet_balance_sol
      : null;

  const selectedRound = rounds.find((r) => r.signalId === selected) ?? null;

  return (
    <div className="min-h-screen pb-10">
      <header className="stagger flex items-center justify-between border-b border-line bg-panel-2 px-4 py-3">
        <Link
          href="/"
          className="micro-label transition-colors duration-150 hover:text-accent"
        >
          ← TERMINAL
        </Link>
        <span className="micro-label text-ink">ACCOUNT</span>
      </header>

      <div className="stagger stagger-2 flex flex-wrap items-stretch divide-x divide-line border-b border-line bg-panel-2 px-4">
        <Stat
          label="Wallet"
          value={balance != null ? balance.toFixed(4) : "—"}
          valueCls={balance != null ? undefined : "text-ink-dim"}
          sub={balance != null ? "SOL" : undefined}
        />
        <Stat
          label="Total PnL"
          value={closed.length > 0 ? fmtSignedSol(totalPnl) : "—"}
          valueCls={closed.length > 0 ? pnlCls(totalPnl) : "text-ink-dim"}
          sub={closed.length > 0 ? "SOL" : undefined}
        />
        <Stat
          label="Today PnL"
          value={closed.length > 0 ? fmtSignedSol(todayPnl) : "—"}
          valueCls={closed.length > 0 ? pnlCls(todayPnl) : "text-ink-dim"}
          sub={closed.length > 0 ? "SOL" : undefined}
        />
        <Stat
          label="Win Rate"
          value={winRate != null ? fmtPct(winRate).replace("+", "") : "—"}
          valueCls={winRate != null ? undefined : "text-ink-dim"}
        />
        <Stat
          label="Profit Factor"
          value={
            profitFactor == null ? "—" : profitFactor === Infinity ? "∞" : profitFactor.toFixed(2)
          }
          valueCls={profitFactor != null ? undefined : "text-ink-dim"}
        />
        <Stat
          label="Trades"
          value={closed.length > 0 ? String(closed.length) : "—"}
          valueCls={closed.length > 0 ? undefined : "text-ink-dim"}
        />
      </div>

      <main className="stagger stagger-3 mt-4 flex flex-col gap-4 px-4">
        {loading && (
          <div className="panel p-6 text-center font-mono text-xs text-ink-dim">
            loading…
          </div>
        )}
        {error && (
          <div className="panel p-6 text-center font-mono text-xs text-sell">
            failed to load account data — {error}
          </div>
        )}

        {!loading && !error && (
          <>
            <section>
              <h2 className="micro-label mb-2 mt-1 px-1">Equity</h2>
              <div className="panel p-2">
                <EquityCurve points={equity} />
              </div>
              {facts.length > 0 && (
                <p className="mt-2 px-1 font-mono text-[11px] tabular-nums text-ink-dim">
                  {facts.join(" · ")}
                </p>
              )}
            </section>

            {open.length > 0 && (
              <section>
                <h2 className="micro-label mb-2 mt-1 px-1">
                  Open Positions{" "}
                  <span className="font-mono tabular-nums">({open.length})</span>
                </h2>
                <div className="panel divide-y divide-line">
                  {open.map((p) => (
                    <div
                      key={p.signal_id}
                      className="flex items-center gap-3 px-3 py-2 font-mono text-[11px] tabular-nums"
                    >
                      <span className="min-w-0 flex-1 truncate text-ink" title={p.mint}>
                        {p.symbol ?? shorten(p.mint, 6, 6)}
                      </span>
                      <span className="text-ink-dim">
                        entry {fmtPrice(p.entry_price_usd)}
                      </span>
                      <span className="text-ink">{fmtSol(p.sol_in)} SOL</span>
                      <span className="text-ink-dim">
                        held {fmtDuration(now / 1000 - p.opened_at)}
                      </span>
                      <span className="badge border-buy/40 text-buy">OPEN</span>
                    </div>
                  ))}
                </div>
              </section>
            )}

            <section>
              <h2 className="micro-label mb-2 mt-1 px-1">
                Round Trips{" "}
                <span className="font-mono tabular-nums">({rounds.length})</span>
              </h2>
              <div className="panel overflow-hidden">
                <div className="micro-label flex items-center gap-3 border-b border-line bg-panel-2 px-3 py-2">
                  <span className="w-24 shrink-0">Time</span>
                  <span className="min-w-0 flex-1">Token</span>
                  <span className="w-20 text-right">In</span>
                  <span className="w-20 text-right">Out</span>
                  <span className="w-24 text-center">Result</span>
                  <span className="w-24 text-right">PnL</span>
                  <span className="w-20 text-right">Proof</span>
                </div>
                <div className="divide-y divide-line">
                  {rounds.length === 0 && (
                    <div className="p-6 text-center font-mono text-xs text-ink-dim">
                      no live rounds yet — closed trades appear here after the
                      first exit lands
                    </div>
                  )}
                  {rounds.map((r) => {
                    const badge = reasonBadge(r.reason);
                    const active = r.signalId === selected;
                    return (
                      <button
                        key={r.signalId}
                        onClick={() => setSelected(active ? null : r.signalId)}
                        className={`flex w-full cursor-pointer items-center gap-3 px-3 py-2 text-left font-mono text-[11px] tabular-nums transition-colors duration-150 hover:bg-panel-2 ${
                          active ? "bg-panel-2" : ""
                        }`}
                      >
                        <span className="w-24 shrink-0 text-ink-dim">
                          {fmtTime(r.at)}
                        </span>
                        <span
                          className="min-w-0 flex-1 truncate text-ink"
                          title={r.mint}
                        >
                          {shorten(r.mint, 6, 6)}
                        </span>
                        <span className="w-20 shrink-0 text-right text-ink">
                          {fmtSol(r.solIn)}
                        </span>
                        <span className="w-20 shrink-0 text-right text-ink-dim">
                          {r.solOut != null ? fmtSol(r.solOut) : "—"}
                        </span>
                        <span className="flex w-24 shrink-0 justify-center">
                          <span className={`badge ${badge.cls}`}>{badge.label}</span>
                        </span>
                        <span
                          className={`w-24 shrink-0 text-right ${pnlCls(r.pnlSol)}`}
                        >
                          {fmtSignedSol(r.pnlSol)}
                        </span>
                        <span className="flex w-20 shrink-0 justify-end gap-2">
                          {r.buySig && (
                            <a
                              href={`https://solscan.io/tx/${r.buySig}`}
                              target="_blank"
                              rel="noopener noreferrer"
                              onClick={(e) => e.stopPropagation()}
                              className="text-accent/80 transition-colors duration-150 hover:text-accent"
                            >
                              BUY ↗
                            </a>
                          )}
                          {r.sellSig && (
                            <a
                              href={`https://solscan.io/tx/${r.sellSig}`}
                              target="_blank"
                              rel="noopener noreferrer"
                              onClick={(e) => e.stopPropagation()}
                              className="text-accent/80 transition-colors duration-150 hover:text-accent"
                            >
                              SELL ↗
                            </a>
                          )}
                        </span>
                      </button>
                    );
                  })}
                </div>
                {selectedRound && (
                  <div className="border-t border-line">
                    <CandleChart
                      key={selectedRound.signalId}
                      mint={selectedRound.mint}
                      entryAt={selectedRound.buyAt}
                      exitAt={selectedRound.reason != null ? selectedRound.at : null}
                      pnlSol={selectedRound.pnlSol}
                    />
                  </div>
                )}
              </div>
            </section>
          </>
        )}
      </main>
    </div>
  );
}
