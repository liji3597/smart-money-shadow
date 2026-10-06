"use client";

import { useEffect, useState } from "react";
import { fetchOhlcv } from "@/lib/api";
import type { OhlcvBar } from "@/lib/types";
import { fmtPrice, fmtSol } from "@/lib/format";

const W = 720;
const H = 260;
const PAD_L = 64;
const PAD_R = 12;
const PAD_T = 16;
const PAD_B = 8;

interface Bar {
  t: number;
  o: number;
  h: number;
  l: number;
  c: number;
}

function nearestIdx(bars: Bar[], at: number): number | null {
  if (bars.length === 0) return null;
  let best = 0;
  for (let i = 1; i < bars.length; i++) {
    if (Math.abs(bars[i].t - at) < Math.abs(bars[best].t - at)) best = i;
  }
  // ignore markers far outside the candle window (> 5 bar gaps)
  const step = bars.length > 1 ? bars[1].t - bars[0].t : 60;
  return Math.abs(bars[best].t - at) <= Math.abs(step) * 5 ? best : null;
}

function Marker({
  xi,
  kind,
  yTop,
  yBot,
}: {
  xi: number | null;
  kind: "entry" | "exit";
  yTop: number;
  yBot: number;
}) {
  if (xi == null) return null;
  const up = kind === "entry";
  const y = up ? yBot : yTop;
  const d = up ? 5 : -5;
  return (
    <g>
      <line
        x1={xi}
        x2={xi}
        y1={yTop - 8}
        y2={yBot + 8}
        stroke="var(--color-accent)"
        strokeWidth="1"
        strokeDasharray="3 3"
        opacity="0.7"
      />
      <polygon
        points={`${xi - 4},${y + d} ${xi + 4},${y + d} ${xi},${y}`}
        fill="var(--color-accent)"
      />
      <text
        x={xi + 6}
        y={up ? yBot - 2 : yTop + 10}
        fill="var(--color-accent)"
        fontSize="9"
        fontFamily="var(--font-mono), ui-monospace, monospace"
      >
        {up ? "ENTRY" : "EXIT"}
      </text>
    </g>
  );
}

export default function CandleChart({
  mint,
  entryAt,
  exitAt,
  pnlSol,
}: {
  mint: string;
  entryAt: number;
  exitAt: number | null;
  pnlSol: number | null;
}) {
  const [bars, setBars] = useState<Bar[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    fetchOhlcv(mint, "1m", 200)
      .then((raw) => {
        if (!alive) return;
        setBars(
          raw
            .map((b: OhlcvBar) => ({
              t: b.time,
              o: parseFloat(b.open),
              h: parseFloat(b.high),
              l: parseFloat(b.low),
              c: parseFloat(b.close),
            }))
            .filter((b) => b.h > 0 && b.l > 0),
        );
      })
      .catch((e) => alive && setFailed(e instanceof Error ? e.message : "failed"));
    return () => {
      alive = false;
    };
  }, [mint]);

  if (failed) {
    return (
      <div className="p-6 text-center font-mono text-xs text-ink-dim">
        ohlcv unavailable — {failed}
      </div>
    );
  }
  if (bars == null) {
    return (
      <div className="p-6 text-center font-mono text-xs text-ink-dim">
        loading candles…
      </div>
    );
  }
  if (bars.length === 0) {
    return (
      <div className="p-6 text-center font-mono text-xs text-ink-dim">
        no candles for this mint — too illiquid or delisted
      </div>
    );
  }

  const innerW = W - PAD_L - PAD_R;
  const innerH = H - PAD_T - PAD_B;
  let min = Math.min(...bars.map((b) => b.l));
  let max = Math.max(...bars.map((b) => b.h));
  if (max - min < 1e-12) {
    max *= 1.01;
    min *= 0.99;
  }
  const span = max - min;
  min -= span * 0.06;
  max += span * 0.06;

  const stepX = innerW / bars.length;
  const cx = (i: number) => PAD_L + i * stepX + stepX / 2;
  const y = (v: number) => PAD_T + ((max - v) / (max - min)) * innerH;
  const bodyW = Math.max(1.5, stepX * 0.6);

  const yTicks = [min + span * 0.06, (min + max) / 2, max - span * 0.06];
  const entryIdx = nearestIdx(bars, entryAt);
  const exitIdx = exitAt != null ? nearestIdx(bars, exitAt) : null;

  return (
    <div>
      <div className="flex items-center justify-between px-3 pt-2">
        <span className="micro-label font-mono">{mint.slice(0, 8)}… 1m</span>
        {pnlSol != null && (
          <span
            className={`font-mono text-[11px] font-medium tabular-nums ${
              pnlSol >= 0 ? "text-buy" : "text-sell"
            }`}
          >
            {pnlSol >= 0 ? "+" : "-"}
            {fmtSol(Math.abs(pnlSol))} SOL
          </span>
        )}
      </div>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="block h-auto w-full"
        role="img"
        aria-label={`candles for ${mint}`}
      >
        {bars.map((b, i) => {
          const upDay = b.c >= b.o;
          const color = upDay ? "var(--color-buy)" : "var(--color-sell)";
          const top = y(Math.max(b.o, b.c));
          const bot = y(Math.min(b.o, b.c));
          return (
            <g key={b.t}>
              <line
                x1={cx(i)}
                x2={cx(i)}
                y1={y(b.h)}
                y2={y(b.l)}
                stroke={color}
                strokeWidth="1"
              />
              <rect
                x={cx(i) - bodyW / 2}
                y={top}
                width={bodyW}
                height={Math.max(1, bot - top)}
                fill={upDay ? color : "none"}
                stroke={color}
                strokeWidth="1"
              />
            </g>
          );
        })}
        {yTicks.map((v, i) => (
          <text
            key={i}
            x={PAD_L - 6}
            y={y(v)}
            dy="0.32em"
            textAnchor="end"
            fill="var(--color-ink-dim)"
            fontSize="9"
            fontFamily="var(--font-mono), ui-monospace, monospace"
          >
            {fmtPrice(v)}
          </text>
        ))}
        <Marker xi={entryIdx != null ? cx(entryIdx) : null} kind="entry" yTop={PAD_T} yBot={H - PAD_B} />
        <Marker xi={exitIdx != null ? cx(exitIdx) : null} kind="exit" yTop={PAD_T} yBot={H - PAD_B} />
      </svg>
    </div>
  );
}
