export function resolveOperatorTimezone(): string {
  try {
    const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    return typeof timezone === "string" && timezone.trim().length > 0
      ? timezone
      : "UTC";
  } catch {
    return "UTC";
  }
}

export function resolveTzOffsetMinutes(): number {
  return -new Date().getTimezoneOffset();
}
