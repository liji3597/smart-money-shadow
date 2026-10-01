import type { Metrics } from "@/lib/types";
import { fmtInt, fmtUsd } from "@/lib/format";

function Stat({
  label,
  value,
  accent,
}: {
  label: string;
  value: string;
  accent?: string;
}) {
  return (
    <div className="flex min-w-28 flex-col gap-0.5 rounded border border-white/10 bg-white/[0.03] px-3 py-2">
      <span className="text-[10px] uppercase tracking-widest text-zinc-500">
        {label}
      </span>
      <span className={`font-mono text-sm font-semibold ${accent ?? "text-zinc-100"}`}>
        {value}
      </span>
    </div>
  );
}

export default function MetricsBar({ metrics }: { metrics: Metrics | null }) {
  const connected = metrics?.stream_connected ?? false;
  return (
    <div className="flex flex-wrap items-stretch gap-2 px-4 py-3">
      <div className="flex min-w-28 flex-col gap-0.5 rounded border border-white/10 bg-white/[0.03] px-3 py-2">
        <span className="text-[10px] uppercase tracking-widest text-zinc-500">
          Stream
        </span>
        <span className="flex items-center gap-1.5 font-mono text-sm font-semibold">
          <span
            className={`inline-block h-2 w-2 rounded-full ${
              connected
                ? "bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.9)]"
                : "bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.9)]"
            }`}
          />
          <span className={connected ? "text-green-400" : "text-red-400"}>
            {connected ? "LIVE" : "DOWN"}
          </span>
        </span>
      </div>
      <Stat label="Events/min" value={fmtInt(metrics?.events_per_min)} />
      <Stat
        label="Volume"
        value={fmtUsd(metrics?.volume_usd_total)}
        accent="text-cyan-300"
      />
      <Stat label="Swaps" value={fmtInt(metrics?.swaps_total)} />
      <Stat label="New Tokens" value={fmtInt(metrics?.token_creates_total)} />
      <Stat
        label="Signals"
        value={fmtInt(metrics?.signals_total)}
        accent="text-amber-300"
      />
      <Stat label="Smart Money" value={fmtInt(metrics?.smart_money_count)} />
      <Stat label="Tracked" value={fmtInt(metrics?.tracked_tokens)} />
      {metrics?.beam_last_latency_ms != null && (
        <Stat
          label="Beam Latency"
          value={`${metrics.beam_last_latency_ms}ms`}
          accent="text-fuchsia-300"
        />
      )}
    </div>
  );
}
