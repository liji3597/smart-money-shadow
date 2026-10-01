export interface TickerItem {
  id: number;
  kind: "buy" | "sell" | "new" | "other";
  text: string;
}

const kindCls: Record<TickerItem["kind"], string> = {
  buy: "text-green-400",
  sell: "text-red-400",
  new: "text-cyan-300",
  other: "text-zinc-400",
};

export default function EventTicker({ items }: { items: TickerItem[] }) {
  const loop = items.length > 0 ? [...items, ...items] : [];
  return (
    <div className="fixed inset-x-0 bottom-0 z-20 h-8 overflow-hidden border-t border-white/10 bg-[#05070a]/95 backdrop-blur">
      {items.length === 0 ? (
        <div className="flex h-full items-center px-4 font-mono text-[11px] text-zinc-600">
          waiting for events…
        </div>
      ) : (
        <div className="animate-marquee flex h-full w-max items-center gap-8 whitespace-nowrap px-4 font-mono text-[11px] hover:[animation-play-state:paused]">
          {loop.map((it, i) => (
            <span key={`${it.id}-${i}`} className={kindCls[it.kind]}>
              {it.text}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
