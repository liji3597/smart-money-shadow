"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  fetchMetrics,
  fetchPerformance,
  fetchPositions,
  fetchSignals,
  fetchTokens,
  fetchTrades,
  WS_URL,
} from "@/lib/api";
import type {
  DexEvent,
  Metrics,
  PerformanceStats,
  Position,
  Positions,
  Signal,
  TokenInfo,
  TradeRecord,
  WsMessage,
} from "@/lib/types";
import { fmtUsd, shorten } from "@/lib/format";
import PositionsPanel from "@/components/PositionsPanel";
import SignalFeed from "@/components/SignalFeed";
import StatusStrip from "@/components/StatusStrip";
import TokenBoard from "@/components/TokenBoard";
import TradeList from "@/components/TradeList";
import EventTicker, { type TickerItem } from "@/components/EventTicker";

const MAX_SIGNALS = 100;
const MAX_TRADES = 100;
const MAX_TICKER = 40;
const MAX_TOKENS = 100;

export default function Dashboard() {
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [perf, setPerf] = useState<PerformanceStats | null>(null);
  const [signals, setSignals] = useState<Signal[]>([]);
  const [tokens, setTokens] = useState<TokenInfo[]>([]);
  const [trades, setTrades] = useState<TradeRecord[]>([]);
  const [positions, setPositions] = useState<Positions>({ open: [], closed: [] });
  const [ticker, setTicker] = useState<TickerItem[]>([]);
  const [wsConnected, setWsConnected] = useState(false);
  const [flashIds, setFlashIds] = useState<ReadonlySet<string>>(new Set());
  const [toast, setToast] = useState<{ id: number; msg: string } | null>(null);
  const [clock, setClock] = useState(() => Date.now());

  const tokensRef = useRef<TokenInfo[]>([]);
  const tickerId = useRef(0);
  const tickerBuf = useRef<TickerItem[]>([]);
  const tokenBuf = useRef<Map<string, TokenInfo>>(new Map());
  const toastTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    tokensRef.current = tokens;
  }, [tokens]);

  const showToast = useCallback((msg: string) => {
    window.clearTimeout(toastTimer.current);
    setToast({ id: Date.now(), msg });
    toastTimer.current = window.setTimeout(() => setToast(null), 2500);
  }, []);

  const copy = useCallback(
    async (text: string) => {
      try {
        await navigator.clipboard.writeText(text);
        showToast(`copied ${shorten(text, 6, 6)}`);
      } catch {
        showToast("copy failed");
      }
    },
    [showToast],
  );

  // clock tick so relative timestamps stay fresh
  useEffect(() => {
    const t = window.setInterval(() => setClock(Date.now()), 10_000);
    return () => window.clearInterval(t);
  }, []);

  // initial REST loads
  useEffect(() => {
    let alive = true;
    fetchSignals(50)
      .then((s) => alive && setSignals(s.slice(0, MAX_SIGNALS)))
      .catch(() => {});
    fetchTokens(100)
      .then((t) => alive && setTokens(t))
      .catch(() => {});
    fetchTrades(50)
      .then((t) => alive && setTrades(t.slice(0, MAX_TRADES)))
      .catch(() => {});
    fetchPositions()
      .then((p) => alive && setPositions(p))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  // metrics polling every 5s
  useEffect(() => {
    let alive = true;
    const load = () =>
      fetchMetrics()
        .then((m) => alive && setMetrics(m))
        .catch(() => {});
    load();
    const t = window.setInterval(load, 5000);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, []);

  // performance polling every 30s (backend backfills in 60s rounds);
  // re-fetch signals alongside so PnL badges pick up backfilled values;
  // positions ride along so last-seen prices stay fresh
  useEffect(() => {
    let alive = true;
    const load = () => {
      fetchPerformance()
        .then((p) => alive && setPerf(p))
        .catch(() => {});
      fetchSignals(50)
        .then((s) => alive && setSignals(s.slice(0, MAX_SIGNALS)))
        .catch(() => {});
      fetchPositions()
        .then((p) => alive && setPositions(p))
        .catch(() => {});
    };
    load();
    const t = window.setInterval(load, 30_000);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, []);

  const pushTicker = useCallback((item: TickerItem) => {
    tickerBuf.current.push(item);
  }, []);

  // The firehose runs ~80 events/sec; flush the ticker at 1 Hz so the
  // marquee is readable and the page doesn't re-render per event.
  useEffect(() => {
    const t = window.setInterval(() => {
      if (tickerBuf.current.length === 0) return;
      const batch = tickerBuf.current.splice(0).reverse();
      setTicker((prev) => [...batch, ...prev].slice(0, MAX_TICKER));
    }, 1000);
    return () => window.clearInterval(t);
  }, []);

  // Token board flushes at 0.2 Hz: the board re-renders the whole table, so
  // a slower cadence (plus bucketed sorting in TokenBoard) keeps rows stable.
  useEffect(() => {
    const t = window.setInterval(() => {
      const tokBatch = [...tokenBuf.current.values()];
      if (tokBatch.length === 0) return;
      tokenBuf.current.clear();
      setTokens((prev) => {
        const byMint = new Map(tokBatch.map((t) => [t.mint, t]));
        const known = new Set(prev.map((t) => t.mint));
        const updated = prev.map((t) => byMint.get(t.mint) ?? t);
        const fresh = tokBatch.filter((t) => !known.has(t.mint));
        return [...fresh.reverse(), ...updated].slice(0, MAX_TOKENS);
      });
    }, 5000);
    return () => window.clearInterval(t);
  }, []);

  const flashSignal = useCallback((id: string) => {
    setFlashIds((prev) => new Set(prev).add(id));
    window.setTimeout(() => {
      setFlashIds((prev) => {
        if (!prev.has(id)) return prev;
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }, 1600);
  }, []);

  const applyEvent = useCallback(
    (ev: DexEvent) => {
      // ticker line
      const sym = ev.mint
        ? (tokensRef.current.find((t) => t.mint === ev.mint)?.symbol ??
          shorten(ev.mint))
        : "";
      const id = ++tickerId.current;
      switch (ev.type) {
        case "swap":
          pushTicker({
            id,
            kind: ev.side === "sell" ? "sell" : "buy",
            text: `${ev.side === "sell" ? "SELL" : "BUY"} ${fmtUsd(
              ev.volume_usd ?? 0,
            )} ${sym} on ${ev.dex ?? "?"}`,
          });
          break;
        case "token_create":
          pushTicker({
            id,
            kind: "new",
            text: `NEW: ${ev.name ?? "?"} (${ev.symbol ?? sym}) on ${ev.dex ?? "?"}`,
          });
          break;
        case "pool_create":
          pushTicker({
            id,
            kind: "new",
            text: `POOL: ${ev.symbol ?? sym} on ${ev.dex ?? "?"}`,
          });
          break;
        case "surge":
          pushTicker({
            id,
            kind: "other",
            text: `SURGE x${ev.multiple?.toFixed(1) ?? "?"} ${sym} · ${fmtUsd(
              ev.volume_window_usd ?? 0,
            )}/${ev.window_secs ?? "?"}s`,
          });
          break;
        case "graduation":
          pushTicker({ id, kind: "new", text: `GRADUATED: ${sym}` });
          break;
        default:
          pushTicker({
            id,
            kind: "other",
            text: `${ev.type.toUpperCase()} ${sym}`,
          });
      }

      // incremental token board updates, buffered and flushed at 0.2 Hz
      if (ev.type === "swap" && ev.mint) {
        const vol = ev.volume_usd ?? 0;
        const now = Math.floor(Date.now() / 1000);
        const base =
          tokenBuf.current.get(ev.mint) ??
          tokensRef.current.find((t) => t.mint === ev.mint);
        const t: TokenInfo = base
          ? { ...base }
          : {
              mint: ev.mint,
              dex: "?",
              price_usd: 0,
              buy_volume_usd: 0,
              sell_volume_usd: 0,
              buy_count: 0,
              sell_count: 0,
              unique_traders: 0,
              graduated: false,
              last_activity: now,
            };
        if (ev.dex) t.dex = ev.dex;
        if (ev.price_usd) t.price_usd = ev.price_usd;
        if (ev.side === "sell") {
          t.sell_volume_usd += vol;
          t.sell_count += 1;
        } else {
          t.buy_volume_usd += vol;
          t.buy_count += 1;
        }
        t.last_activity = ev.block_time ?? now;
        tokenBuf.current.set(ev.mint, t);
      } else if (ev.type === "token_create" && ev.mint) {
        const now = Math.floor(Date.now() / 1000);
        const base =
          tokenBuf.current.get(ev.mint) ??
          tokensRef.current.find((t) => t.mint === ev.mint);
        const t: TokenInfo = base
          ? { ...base }
          : {
              mint: ev.mint,
              dex: "?",
              price_usd: 0,
              buy_volume_usd: 0,
              sell_volume_usd: 0,
              buy_count: 0,
              sell_count: 0,
              unique_traders: 0,
              graduated: false,
              last_activity: now,
            };
        if (ev.name) t.name = ev.name;
        if (ev.symbol) t.symbol = ev.symbol;
        if (ev.dex) t.dex = ev.dex;
        if (ev.pool) t.pool = ev.pool;
        if (ev.creator) t.creator = ev.creator;
        t.created_at = t.created_at ?? ev.block_time;
        t.last_activity = ev.block_time ?? now;
        tokenBuf.current.set(ev.mint, t);
      } else if (ev.type === "graduation" && ev.mint) {
        const base =
          tokenBuf.current.get(ev.mint) ??
          tokensRef.current.find((t) => t.mint === ev.mint);
        if (base) tokenBuf.current.set(ev.mint, { ...base, graduated: true });
      }
    },
    [pushTicker],
  );

  // websocket with exponential backoff reconnect
  useEffect(() => {
    let ws: WebSocket | null = null;
    let attempts = 0;
    let stopped = false;
    let timer: number | undefined;

    const connect = () => {
      if (stopped) return;
      ws = new WebSocket(WS_URL);
      ws.onopen = () => {
        attempts = 0;
        setWsConnected(true);
      };
      ws.onmessage = (e) => {
        let msg: WsMessage;
        try {
          msg = JSON.parse(e.data as string);
        } catch {
          return;
        }
        switch (msg.kind) {
          case "snapshot":
            setMetrics(msg.data.metrics);
            setSignals(msg.data.signals.slice(0, MAX_SIGNALS));
            setTrades(msg.data.trades.slice(0, MAX_TRADES));
            setPositions(msg.data.positions);
            break;
          case "signal":
            setSignals((prev) =>
              prev.some((s) => s.id === msg.data.id)
                ? prev
                : [msg.data, ...prev].slice(0, MAX_SIGNALS),
            );
            flashSignal(msg.data.id);
            break;
          case "trade":
            setTrades((prev) => [msg.data, ...prev].slice(0, MAX_TRADES));
            break;
          case "position":
            setPositions((prev) => {
              const p: Position = msg.data;
              if (p.status.state === "open") {
                const rest = prev.open.filter((o) => o.mint !== p.mint);
                return { ...prev, open: [p, ...rest] };
              }
              return {
                open: prev.open.filter((o) => o.mint !== p.mint),
                closed: [p, ...prev.closed.filter((c) => c.signal_id !== p.signal_id)].slice(0, 100),
              };
            });
            break;
          case "event":
            applyEvent(msg.data);
            break;
        }
      };
      ws.onclose = () => {
        setWsConnected(false);
        if (stopped) return;
        const delay = Math.min(30_000, 1000 * 2 ** attempts);
        attempts += 1;
        timer = window.setTimeout(connect, delay);
      };
      ws.onerror = () => {
        ws?.close();
      };
    };

    connect();
    return () => {
      stopped = true;
      window.clearTimeout(timer);
      ws?.close();
    };
  }, [applyEvent, flashSignal]);

  // latest risk score per mint (signals arrive newest-first) for TokenBoard
  const riskByMint = useMemo(() => {
    const m = new Map<string, number>();
    for (const s of signals) {
      if (!m.has(s.mint)) m.set(s.mint, s.risk_score);
    }
    return m;
  }, [signals]);

  return (
    <div className="min-h-screen pb-10">
      <header className="flex items-center justify-between border-b border-line bg-panel-2 px-4 py-3">
        <div className="flex items-baseline gap-3">
          <h1 className="text-[15px] font-bold tracking-[-0.02em] text-ink">
            SMART-MONEY <span className="text-accent">SHADOW</span>
          </h1>
          <span className="micro-label hidden sm:inline">
            solana smart money tracker
          </span>
        </div>
        <div className="flex items-center gap-3 font-mono text-[11px]">
          <span className="tabular-nums text-ink-dim" suppressHydrationWarning>
            {new Date(clock).toLocaleTimeString("en-GB")}
          </span>
          <span
            className={`badge gap-1.5 ${
              wsConnected
                ? "border-buy/40 text-buy"
                : "border-sell/40 text-sell"
            }`}
          >
            <span
              className={`inline-block h-1.5 w-1.5 rounded-full ${
                wsConnected ? "bg-buy animate-live" : "bg-sell"
              }`}
            />
            WS {wsConnected ? "CONNECTED" : "RECONNECTING"}
          </span>
        </div>
      </header>

      <StatusStrip metrics={metrics} perf={perf} />

      <main className="mt-4 grid gap-4 px-4 lg:grid-cols-10">
        <div className="order-1 lg:order-none lg:col-span-4">
          <SignalFeed signals={signals} flashIds={flashIds} onCopy={copy} />
        </div>
        <div className="order-3 lg:order-none lg:col-span-3">
          <TokenBoard tokens={tokens} riskByMint={riskByMint} onCopy={copy} />
        </div>
        <div className="order-2 flex flex-col gap-4 lg:order-none lg:col-span-3">
          <PositionsPanel
            open={positions.open}
            closed={positions.closed}
            onCopy={copy}
          />
          <TradeList trades={trades} onCopy={copy} />
        </div>
      </main>

      <EventTicker items={ticker} />

      {toast && (
        <div
          key={toast.id}
          className="fixed bottom-12 right-4 z-30 rounded-[10px] border border-accent/40 bg-panel-2 px-3 py-2 font-mono text-xs text-accent"
        >
          {toast.msg}
        </div>
      )}
    </div>
  );
}
