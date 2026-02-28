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
