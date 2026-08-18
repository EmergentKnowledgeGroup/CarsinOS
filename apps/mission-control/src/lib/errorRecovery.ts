import { eventSummary } from "./eventStream";
import { redactSecrets } from "./redaction";

export interface CrashWindowState {
  windowStartMs: number;
  crashCount: number;
}

export interface ErrorEventContext {
  event_id: string;
  event_type: string;
  entity: string;
  ts_unix_ms: number;
  payload: Record<string, unknown>;
}

export function nextCrashWindow(
  previous: CrashWindowState,
  nowMs: number,
  windowMs: number
): CrashWindowState {
  if (nowMs - previous.windowStartMs >= windowMs) {
    return {
      windowStartMs: nowMs,
      crashCount: 1,
    };
  }

  return {
    windowStartMs: previous.windowStartMs,
    crashCount: previous.crashCount + 1,
  };
}

export function shouldEnterSafeMode(
  crashCount: number,
  threshold: number
): boolean {
  return crashCount >= threshold;
}

export function buildErrorReport(
  title: string,
  error: Error,
  componentStack: string | null,
  events: readonly ErrorEventContext[]
): string {
  const cleanMessage = redactSecrets(error.message);
  const cleanStack = error.stack ? redactSecrets(error.stack) : "(no stack)";
  const cleanComponentStack = componentStack ? redactSecrets(componentStack) : "(none)";

  const eventLines = events.slice(0, 10).map((event) => {
    const summary = eventSummary(event.event_type, event.payload) ?? "(no summary)";
    const safeType = redactSecrets(event.event_type);
    const safeEntity = redactSecrets(event.entity);
    const safeSummary = redactSecrets(summary);
    return `${event.ts_unix_ms} ${safeType} ${safeEntity} ${safeSummary}`;
  });

  return [
    `${title}`,
    `message: ${cleanMessage}`,
    `stack: ${cleanStack}`,
    `component_stack: ${cleanComponentStack}`,
    "recent_events:",
    ...(eventLines.length > 0 ? eventLines : ["(none)"]),
  ].join("\n");
}
