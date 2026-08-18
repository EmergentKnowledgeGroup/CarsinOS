import { describe, expect, it } from "vitest";
import {
  eventDomain,
  eventSummary,
  filterVisibleEvents,
  isHeartbeatEvent,
  redactEventPayload,
} from "./eventStream";

describe("eventStream helpers", () => {
  it("hides heartbeat events when raw mode is disabled", () => {
    const events = [
      { event_type: "heartbeat.gateway" },
      { event_type: "job.updated" },
      { event_type: "approval.requested" },
    ];

    expect(filterVisibleEvents(events, false).map((event) => event.event_type)).toEqual([
      "job.updated",
      "approval.requested",
    ]);
    expect(filterVisibleEvents(events, true).map((event) => event.event_type)).toEqual([
      "heartbeat.gateway",
      "job.updated",
      "approval.requested",
    ]);
  });

  it("maps domains from event type prefixes", () => {
    expect(eventDomain("job.updated")).toBe("job");
    expect(eventDomain("approval.requested")).toBe("approval");
    expect(eventDomain("heartbeat.gateway")).toBe("heartbeat");
    expect(eventDomain("plain")).toBe("plain");
    expect(isHeartbeatEvent("heartbeat.gateway")).toBe(true);
    expect(isHeartbeatEvent("job.updated")).toBe(false);
  });

  it("summarizes board, job, and approval payloads", () => {
    expect(eventSummary("board.card.updated", { title: "Ship patch" })).toBe(
      "Card updated: Ship patch"
    );
    expect(eventSummary("job.updated", { job_id: "job_123" })).toBe("job_123");
    expect(eventSummary("approval.requested", { status: "requested" })).toBe("requested");
    expect(eventSummary("channel.runtime", { channel: "anthropic" })).toBeNull();
  });

  it("redacts sensitive payload keys and inline token values", () => {
    const payload = {
      api_key: "sk-ant-12345abcdef",
      nested: {
        authorization: "Bearer secret-token-value",
        token_hint: "x-api-key: should-hide",
      },
      safe: "visible",
    };

    expect(redactEventPayload(payload)).toEqual({
      api_key: "[REDACTED]",
      nested: {
        authorization: "[REDACTED]",
        token_hint: "[REDACTED]",
      },
      safe: "visible",
    });
  });
});
