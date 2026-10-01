export function fmtUsd(n: number | null | undefined): string {
  if (n == null || Number.isNaN(n)) return "$0";
  const sign = n < 0 ? "-" : "";
  const abs = Math.abs(n);
  if (abs >= 1e9) return `${sign}$${(abs / 1e9).toFixed(2)}B`;
  if (abs >= 1e6) return `${sign}$${(abs / 1e6).toFixed(2)}M`;
  if (abs >= 1e3) return `${sign}$${(abs / 1e3).toFixed(1)}k`;
  if (abs >= 100) return `${sign}$${Math.round(abs)}`;
  if (abs >= 1) return `${sign}$${abs.toFixed(2)}`;
  return `${sign}$${abs.toFixed(2)}`;
}

export function fmtPrice(p: number | null | undefined): string {
  if (p == null || Number.isNaN(p) || p <= 0) return "$0";
  if (p >= 1000)
    return `$${p.toLocaleString("en-US", { maximumFractionDigits: 0 })}`;
  if (p >= 1) return `$${p.toFixed(2)}`;
  if (p >= 0.01) return `$${p.toFixed(4)}`;
  // tiny prices: 4 significant digits without exponential notation
  const exp = Math.floor(Math.log10(p));
  const decimals = Math.min(-exp + 3, 18);
  return `$${p.toFixed(decimals)}`;
}

export function fmtInt(n: number | null | undefined): string {
  if (n == null || Number.isNaN(n)) return "0";
  return Math.round(n).toLocaleString("en-US");
}

export function fmtPct(n: number | null | undefined): string {
  if (n == null || Number.isNaN(n)) return "—";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(1)}%`;
}

export function fmtSol(n: number | null | undefined): string {
  if (n == null || Number.isNaN(n)) return "0";
  if (n >= 1) return n.toFixed(2);
  return n.toFixed(4);
}

export function shorten(addr: string, head = 4, tail = 4): string {
  if (addr.length <= head + tail + 3) return addr;
  return `${addr.slice(0, head)}…${addr.slice(-tail)}`;
}

export function timeAgo(unixSecs: number | null | undefined): string {
  if (!unixSecs) return "—";
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - unixSecs);
  if (diff < 5) return "now";
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}
