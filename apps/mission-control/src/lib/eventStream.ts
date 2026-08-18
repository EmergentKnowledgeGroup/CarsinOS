import { redactSecrets } from "./redaction";

export interface EventStreamLike {
  event_type: string;
}

export function isHeartbeatEvent(eventType: string): boolean {
  return eventType.startsWith("heartbeat.");
}

export function filterVisibleEvents<T extends EventStreamLike>(
  events: readonly T[],
  showRawEvents: boolean
): T[] {
  if (showRawEvents) {
    return [...events];
  }
  return events.filter((event) => !isHeartbeatEvent(event.event_type));
}

export function eventDomain(eventType: string): string {
  const dot = eventType.indexOf(".");
  return dot > 0 ? eventType.slice(0, dot) : eventType;
}

function payloadString(value: unknown): string {
  if (typeof value === "string" || typeof value === "number") {
    return String(value);
  }
  return "";
}

export function eventSummary(eventType: string, payload: Record<string, unknown>): string | null {
  if (eventType.startsWith("board.card.")) {
    const action = eventType.split(".").pop() ?? "updated";
    const title = payloadString(payload.title) || payloadString(payload.card_id);
    return title ? `Card ${action}: ${title}` : `Card ${action}`;
  }

  if (eventType.startsWith("job.")) {
    return payloadString(payload.job_id) || payloadString(payload.agent_id) || null;
  }

  if (eventType.startsWith("approval.")) {
    return payloadString(payload.decision) || payloadString(payload.status) || null;
  }

  return null;
}

export function redactEventPayload(payload: Record<string, unknown>): Record<string, unknown> {
  return redactSecrets(payload);
}
