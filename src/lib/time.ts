// Relative times and countdowns. Pure, so tests pass the clock.

function plural(n: number, unit: string): string {
  return `${n} ${unit}${n === 1 ? "" : "s"} ago`;
}

/** "14 seconds ago", "3 minutes ago", "2 hours ago", "5 days ago". Never below one second. */
export function ago(then: Date, now: Date): string {
  const seconds = Math.max(1, Math.floor((now.getTime() - then.getTime()) / 1000));
  if (seconds < 60) return plural(seconds, "second");
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return plural(minutes, "minute");
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return plural(hours, "hour");
  return plural(Math.floor(hours / 24), "day");
}

/** "9:41": minutes and seconds left, never negative. */
export function countdown(until: Date, now: Date): string {
  const left = Math.max(0, Math.ceil((until.getTime() - now.getTime()) / 1000));
  const minutes = Math.floor(left / 60);
  const seconds = left % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/** The host and path of a URL, without the scheme: "longlines.io/relay". */
export function withoutScheme(url: string): string {
  return url.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").replace(/\/$/, "");
}
