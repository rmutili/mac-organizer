const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;
const DAY_SECONDS = 86_400;

export function formatBytes(bytes: number): string {
  const exponent = Math.min(Math.floor(Math.log(Math.max(bytes, 1)) / Math.log(1024)), UNITS.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${exponent === 0 ? value.toFixed(0) : value.toFixed(1)} ${UNITS[exponent]}`;
}

function plural(n: number, unit: string): string {
  return `${n} ${unit}${n === 1 ? "" : "s"} ago`;
}

/** Plain-language age of a Unix timestamp (seconds) relative to `now`. */
export function formatAge(timestamp: number, now: number): string {
  const days = Math.max(0, Math.floor((now - timestamp) / DAY_SECONDS));
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 60) return plural(days, "day");
  if (days < 365) return plural(Math.floor(days / 30), "month");
  return plural(Math.floor(days / 365), "year");
}

export function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}
