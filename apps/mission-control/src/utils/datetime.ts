export function toInputDateTimeValue(unixMs: number | null): string {
  if (unixMs === null || unixMs === undefined) {
    return "";
  }
  const date = new Date(unixMs);
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

export function fromInputDateTimeValue(value: string): number | null {
  if (!value.trim()) {
    return null;
  }
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? null : parsed;
}

export function formatDateTime(unixMs: number | null | undefined): string {
  if (unixMs === null || unixMs === undefined) {
    return "n/a";
  }
  return new Date(unixMs).toLocaleString();
}

/** Relative time string: "just now", "3m ago", "in 12m", etc. */
export function formatRelative(unixMs: number | null | undefined): string {
  if (unixMs === null || unixMs === undefined) return "—";
  const diff = unixMs - Date.now();
  if (diff < 0) {
    const ago = Math.abs(diff);
    if (ago < 60_000) return "just now";
    if (ago < 3_600_000) return `${Math.floor(ago / 60_000)}m ago`;
    if (ago < 86_400_000) return `${Math.floor(ago / 3_600_000)}h ago`;
    return `${Math.floor(ago / 86_400_000)}d ago`;
  }
  if (diff < 60_000) return "< 1m";
  if (diff < 3_600_000) return `in ${Math.floor(diff / 60_000)}m`;
  if (diff < 86_400_000) return `in ${Math.floor(diff / 3_600_000)}h`;
  return `in ${Math.floor(diff / 86_400_000)}d`;
}
