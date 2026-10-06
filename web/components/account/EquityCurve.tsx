import { fmtSol } from "@/lib/format";

interface Point {
  t: number; // unix seconds
  v: number; // cumulative realized pnl, SOL
}

const W = 720;
const H = 220;
const PAD_L = 52;
const PAD_R = 12;
const PAD_T = 12;
const PAD_B = 22;

function fmtClock(at: number): string {
  return new Date(at * 1000).toLocaleTimeString("en-GB", {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Cumulative realized-PnL curve. Pure SVG, no chart lib. */
export default function EquityCurve({ points }: { points: Point[] }) {
  if (points.length === 0) {
    return (
      <div className="p-6 text-center font-mono text-xs text-ink-dim">
        no equity yet — the curve draws after the first closed round
      </div>
    );
  }

  const innerW = W - PAD_L - PAD_R;
  const innerH = H - PAD_T - PAD_B;
  const values = points.map((p) => p.v);
  let min = Math.min(0, ...values);
  let max = Math.max(0, ...values);
  if (max - min < 1e-9) {
    max += 0.001;
    min -= 0.001;
  }
  const span = max - min;
  min -= span * 0.08;
  max += span * 0.08;

  const x = (i: number) =>
    PAD_L + (points.length === 1 ? innerW / 2 : (i / (points.length - 1)) * innerW);
  const y = (v: number) => PAD_T + ((max - v) / (max - min)) * innerH;

  const line = points.map((p, i) => `${x(i)},${y(p.v)}`).join(" ");
  const baseY = y(min);
  const area = `${PAD_L},${baseY} ${line} ${x(points.length - 1)},${baseY}`;
  const last = points[points.length - 1];

  const yTicks = [min + span * 0.08, (min + max) / 2, max - span * 0.08];
  const xTickIdx = [...new Set([0, 1, 2, 3].map((k) => Math.round((k * (points.length - 1)) / 3)))];

  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      className="block h-auto w-full"
      role="img"
      aria-label="equity curve"
    >
      {min < 0 && max > 0 && (
        <line
          x1={PAD_L}
          x2={W - PAD_R}
          y1={y(0)}
          y2={y(0)}
          stroke="var(--color-line-strong)"
          strokeDasharray="4 4"
          strokeWidth="1"
        />
      )}
      <polygon points={area} fill="var(--color-buy)" opacity="0.08" />
      <polyline
        points={line}
        fill="none"
        stroke="var(--color-buy)"
        strokeWidth="1.5"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
      <circle cx={x(points.length - 1)} cy={y(last.v)} r="3" fill="var(--color-buy)" />
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
          {fmtSol(v)}
        </text>
      ))}
      {xTickIdx.map((i) => (
        <text
          key={i}
          x={x(i)}
          y={H - 6}
          textAnchor="middle"
          fill="var(--color-ink-dim)"
          fontSize="9"
          fontFamily="var(--font-mono), ui-monospace, monospace"
        >
          {fmtClock(points[i].t)}
        </text>
      ))}
    </svg>
  );
}
