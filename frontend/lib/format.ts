const SATS_PER_BTC = 100_000_000;

const integer = new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 });

export function formatNumber(value: number): string {
  return integer.format(value);
}

/** 1250000000 → "1,250,000,000 sats" */
export function formatSats(sats: number, unit = true): string {
  const n = integer.format(sats);
  return unit ? `${n} sats` : n;
}

/** 1250000000 → "12.50000000 BTC" (exact; avoids float drift for large values) */
export function formatBTC(sats: number, unit = true): string {
  const negative = sats < 0;
  const abs = Math.abs(Math.trunc(sats));
  const whole = Math.floor(abs / SATS_PER_BTC);
  const frac = String(abs % SATS_PER_BTC).padStart(8, "0");
  const n = `${negative ? "-" : ""}${integer.format(whole)}.${frac}`;
  return unit ? `${n} BTC` : n;
}

const dateTime = new Intl.DateTimeFormat("en-GB", {
  year: "numeric",
  month: "short",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
  timeZone: "UTC",
});

/** Unix seconds → "30 Sept 2026, 21:42:30 UTC" */
export function formatTimestamp(unixSeconds: number): string {
  return `${dateTime.format(new Date(unixSeconds * 1000))} UTC`;
}

/** Unix seconds → "4m ago" / "in 2h" (regtest clocks can run ahead of wall time) */
export function formatRelative(unixSeconds: number, now = Date.now()): string {
  const diff = Math.round(now / 1000 - unixSeconds);
  const future = diff < 0;
  const s = Math.abs(diff);
  let text: string;
  if (s < 60) text = `${s}s`;
  else if (s < 3600) text = `${Math.floor(s / 60)}m`;
  else if (s < 86400) text = `${Math.floor(s / 3600)}h`;
  else text = `${Math.floor(s / 86400)}d`;
  return future ? `in ${text}` : `${text} ago`;
}

/** "6331d24c…3848b189" */
export function truncateHash(value: string, head = 8, tail = 8): string {
  if (value.length <= head + tail + 1) return value;
  return `${value.slice(0, head)}…${value.slice(-tail)}`;
}

/** nSequence as fixed-width hex, e.g. 0xfffffffd */
export function formatSequence(sequence: number): string {
  return `0x${sequence.toString(16).padStart(8, "0")}`;
}

export function percent(part: number, whole: number): string {
  if (whole <= 0) return "0.0%";
  return `${((part / whole) * 100).toFixed(1)}%`;
}
