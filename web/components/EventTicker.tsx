import { useState } from "react";

export interface TickerItem {
  id: number;
  kind: "buy" | "sell" | "new" | "other";
  text: string;
}

const kindCls: Record<TickerItem["kind"], string> = {
  buy: "text-buy",
  sell: "text-sell",
  new: "text-ink",
  other: "text-ink-dim",
};

export default function EventTicker({ items }: { items: TickerItem[] }) {
  const [collapsed, setCollapsed] = useState(false);
  const loop = items.length > 0 ? [...items, ...items] : [];
  if (collapsed) {
    return (
      <button
        onClick={() => setCollapsed(false)}
        title="expand event ticker"
        aria-label="expand event ticker"
        className="fixed inset-x-0 bottom-0 z-20 block h-1 cursor-pointer bg-accent/25 transition-colors hover:bg-accent/60"
      />
    );
  }
  return (
    <div className="fixed inset-x-0 bottom-0 z-20 h-8 overflow-hidden border-t border-line bg-panel-2">
      {items.length === 0 ? (
        <div className="flex h-full items-center px-4 font-mono text-[11px] text-ink-dim">
          waiting for events…
        </div>
      ) : (
        <div className="animate-marquee flex h-full w-max items-center whitespace-nowrap px-4 font-mono text-[11px] tabular-nums hover:[animation-play-state:paused]">
          {loop.map((it, i) => (
            <span key={`${it.id}-${i}`} className="flex items-center">
              <span className={kindCls[it.kind]}>{it.text}</span>
              <span className="mx-4 h-3 w-px bg-line" />
            </span>
          ))}
        </div>
      )}
      <button
        onClick={() => setCollapsed(true)}
        title="collapse event ticker"
        aria-label="collapse event ticker"
        className="absolute right-0 top-0 flex h-full w-8 cursor-pointer items-center justify-center border-l border-line bg-panel-2 font-mono text-xs text-ink-dim hover:text-ink"
      >
        −
      </button>
    </div>
  );
}
