import { eventSummary, redactEventPayload } from "./eventStream";
import type { WsEventFrame } from "../types";

export type LiveFeedDomain =
  | "all"
  | "approvals"
  | "jobs"
  | "boards"
  | "mail"
  | "channels"
  | "system"
  | "other";

export type LiveFeedSeverity = "critical" | "high" | "normal" | "low";

export type LiveFeedSeverityFilter =
  | "all"
  | "critical_high"
  | "critical"
  | "high"
  | "normal"
  | "low";

export interface LiveFeedEvent {
  event_id: string;
  event_type: string;
  entity: string;
  ts_unix_ms: number;
  timestamp_utc: string;
  arrival_index: number;
  domain: Exclude<LiveFeedDomain, "all">;
  severity: LiveFeedSeverity;
  summary: string;
  payload_redacted: Record<string, unknown>;
}

export interface LiveFeedOverflowResult {
  events: LiveFeedEvent[];
  dropped: Record<LiveFeedSeverity, number>;
}

export interface RecoveryPruneOptions {
  now_ms: number;
  retention_window_ms: number;
  max_bytes: number;
}

export const LIVE_FEED_IN_MEMORY_CAP = 2_000;
export const LIVE_FEED_MAX_RENDERED_ROWS = 300;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toDomain(eventType: string, payload: Record<string, unknown>): Exclude<LiveFeedDomain, "all"> {
  const payloadDomain = payload.domain;
  if (typeof payloadDomain === "string") {
    const normalized = payloadDomain.trim().toLowerCase();
    if (
      normalized === "approvals" ||
      normalized === "jobs" ||
      normalized === "boards" ||
      normalized === "mail" ||
      normalized === "channels" ||
      normalized === "system"
    ) {
      return normalized;
    }
  }

  if (eventType.startsWith("approval.")) return "approvals";
  if (eventType.startsWith("job.")) return "jobs";
  if (eventType.startsWith("board.")) return "boards";
  if (eventType.startsWith("agent_mail.")) return "mail";
  if (eventType.startsWith("channel.")) return "channels";
  if (eventType.startsWith("gateway.") || eventType.startsWith("system.") || eventType.startsWith("extension.")) {
    return "system";
  }
  return "other";
}

function toSeverity(eventType: string, payload: Record<string, unknown>): LiveFeedSeverity {
  const payloadSeverity = payload.severity;
  if (typeof payloadSeverity === "string") {
    const normalized = payloadSeverity.trim().toLowerCase();
    if (normalized === "critical" || normalized === "high" || normalized === "normal" || normalized === "low") {
      return normalized;
    }
    if (normalized === "error") {
      return "high";
    }
    if (normalized === "warning") {
      return "normal";
    }
  }

  if (eventType.includes("critical")) return "critical";
  if (eventType.startsWith("approval.") || eventType.startsWith("channel.")) return "high";
  if (eventType.startsWith("heartbeat.")) return "low";
  return "normal";
}

function toSummary(eventType: string, payload: Record<string, unknown>): string {
  if (typeof payload.summary === "string" && payload.summary.trim().length > 0) {
    return payload.summary.trim();
  }
  const computed = eventSummary(eventType, payload);
  if (computed && computed.trim().length > 0) {
    return computed.trim();
  }
  return eventType;
}

export function normalizeLiveFeedEvent(frame: WsEventFrame, arrivalIndex: number): LiveFeedEvent {
  const payload = isRecord(frame.payload) ? frame.payload : {};
  const domain = toDomain(frame.event_type, payload);
  const severity = toSeverity(frame.event_type, payload);
  const summary = toSummary(frame.event_type, payload);

  return {
    event_id: frame.event_id,
    event_type: frame.event_type,
    entity: frame.entity,
    ts_unix_ms: frame.ts_unix_ms,
    timestamp_utc: new Date(frame.ts_unix_ms).toISOString(),
    arrival_index: arrivalIndex,
    domain,
    severity,
    summary,
    payload_redacted: redactEventPayload(payload),
  };
}

export function applyLiveFeedOverflowPolicy(
  events: readonly LiveFeedEvent[],
  cap = LIVE_FEED_IN_MEMORY_CAP
): LiveFeedOverflowResult {
  const next = [...events];
  const dropped: Record<LiveFeedSeverity, number> = {
    critical: 0,
    high: 0,
    normal: 0,
    low: 0,
  };
  if (cap < 1) {
    return {
      events: [],
      dropped,
    };
  }

  const evictionOrder: LiveFeedSeverity[] = ["low", "normal", "high", "critical"];

  while (next.length > cap) {
    let evicted = false;
    for (const severity of evictionOrder) {
      const index = (() => {
        for (let i = next.length - 1; i >= 0; i -= 1) {
          if (next[i].severity === severity) {
            return i;
          }
        }
        return -1;
      })();

      if (index >= 0) {
        next.splice(index, 1);
        dropped[severity] += 1;
        evicted = true;
        break;
      }
    }

    if (!evicted) {
      break;
    }
  }

  return {
    events: next,
    dropped,
  };
}

export function filterLiveFeedEvents(
  events: readonly LiveFeedEvent[],
  domainFilter: LiveFeedDomain,
  severityFilter: LiveFeedSeverityFilter
): LiveFeedEvent[] {
  return events.filter((event) => {
    if (domainFilter !== "all" && event.domain !== domainFilter) {
      return false;
    }

    if (severityFilter === "all") {
      return true;
    }
    if (severityFilter === "critical_high") {
      return event.severity === "critical" || event.severity === "high";
    }
    return event.severity === severityFilter;
  });
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}

export function pruneRecoveryLog(
  events: readonly LiveFeedEvent[],
  options: RecoveryPruneOptions
): LiveFeedEvent[] {
  const minTs = options.now_ms - Math.max(1, options.retention_window_ms);
  const bounded = events
    .filter((event) => event.ts_unix_ms >= minTs)
    .sort((left, right) => {
      if (left.ts_unix_ms !== right.ts_unix_ms) {
        return left.ts_unix_ms - right.ts_unix_ms;
      }
      return left.arrival_index - right.arrival_index;
    });

  if (bounded.length === 0) {
    return bounded;
  }

  const maxBytes = Math.max(1024, options.max_bytes);
  while (bounded.length > 0) {
    const size = utf8Length(JSON.stringify(bounded));
    if (size <= maxBytes) {
      break;
    }
    bounded.shift();
  }

  return bounded;
}

export function countRecentHighSeverityEvents(
  events: readonly LiveFeedEvent[],
  nowMs: number,
  windowMs: number
): number {
  const minTs = nowMs - Math.max(1, windowMs);
  let count = 0;
  for (const event of events) {
    if (event.ts_unix_ms < minTs) {
      break;
    }
    if (event.severity === "critical" || event.severity === "high") {
      count += 1;
    }
  }
  return count;
}

export function hasCriticalEventWithinWindow(
  events: readonly LiveFeedEvent[],
  nowMs: number,
  windowMs: number
): boolean {
  const minTs = nowMs - Math.max(1, windowMs);
  for (const event of events) {
    if (event.ts_unix_ms < minTs) {
      return false;
    }
    if (event.severity === "critical") {
      return true;
    }
  }
  return false;
}
