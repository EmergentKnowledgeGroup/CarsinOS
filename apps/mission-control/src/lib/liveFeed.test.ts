import { describe, expect, it } from "vitest";
import {
  applyLiveFeedOverflowPolicy,
  countRecentHighSeverityEvents,
  filterLiveFeedEvents,
  hasCriticalEventWithinWindow,
  normalizeLiveFeedEvent,
  pruneRecoveryLog,
  type LiveFeedEvent,
} from "./liveFeed";

const BASE_TS = 1_700_000_000_000;

function makeEvent(partial: Partial<LiveFeedEvent> & { event_id: string }): LiveFeedEvent {
  return {
    event_id: partial.event_id,
    event_type: partial.event_type ?? "gateway.notice",
    entity: partial.entity ?? "system",
    ts_unix_ms: partial.ts_unix_ms ?? BASE_TS,
    timestamp_utc: partial.timestamp_utc ?? new Date(partial.ts_unix_ms ?? BASE_TS).toISOString(),
    arrival_index: partial.arrival_index ?? 1,
    domain: partial.domain ?? "system",
    severity: partial.severity ?? "normal",
    summary: partial.summary ?? "summary",
    payload_redacted: partial.payload_redacted ?? {},
  };
}

describe("liveFeed helpers", () => {
  it("normalizes ws events with domain/severity fallback and redacted payload", () => {
    const normalized = normalizeLiveFeedEvent(
      {
        schema_version: "v1",
        event_id: "evt-1",
        event_type: "approval.requested",
        ts_unix_ms: BASE_TS,
        entity: "approval",
        payload: {
          summary: "Approval required",
          api_key: "sk-ant-unsafe",
        },
      },
      7
    );

    expect(normalized.domain).toBe("approvals");
    expect(normalized.severity).toBe("high");
    expect(normalized.summary).toBe("Approval required");
    expect(normalized.payload_redacted.api_key).toBe("[REDACTED]");
    expect(normalized.arrival_index).toBe(7);
  });

  it("drops oldest low/normal events first under cap pressure", () => {
    const seed = [
      makeEvent({ event_id: "new-critical", severity: "critical", ts_unix_ms: BASE_TS + 40, arrival_index: 4 }),
      makeEvent({ event_id: "new-high", severity: "high", ts_unix_ms: BASE_TS + 30, arrival_index: 3 }),
      makeEvent({ event_id: "old-normal", severity: "normal", ts_unix_ms: BASE_TS + 20, arrival_index: 2 }),
      makeEvent({ event_id: "old-low", severity: "low", ts_unix_ms: BASE_TS + 10, arrival_index: 1 }),
    ];

    const result = applyLiveFeedOverflowPolicy(seed, 2);

    expect(result.events.map((event) => event.event_id)).toEqual(["new-critical", "new-high"]);
    expect(result.dropped.low).toBe(1);
    expect(result.dropped.normal).toBe(1);
    expect(result.dropped.critical).toBe(0);
  });

  it("filters by domain and severity", () => {
    const events = [
      makeEvent({ event_id: "a", domain: "approvals", severity: "high" }),
      makeEvent({ event_id: "b", domain: "jobs", severity: "critical" }),
      makeEvent({ event_id: "c", domain: "system", severity: "normal" }),
    ];

    expect(filterLiveFeedEvents(events, "all", "critical_high").map((event) => event.event_id)).toEqual([
      "a",
      "b",
    ]);
    expect(filterLiveFeedEvents(events, "jobs", "all").map((event) => event.event_id)).toEqual(["b"]);
  });

  it("prunes recovery log by retention window and max bytes", () => {
    const events = [
      makeEvent({ event_id: "old", ts_unix_ms: BASE_TS - 61_000, arrival_index: 1 }),
      makeEvent({ event_id: "mid", ts_unix_ms: BASE_TS - 10_000, arrival_index: 2, payload_redacted: { v: "x".repeat(600) } }),
      makeEvent({ event_id: "new", ts_unix_ms: BASE_TS, arrival_index: 3, payload_redacted: { v: "x".repeat(600) } }),
    ];

    const pruned = pruneRecoveryLog(events, {
      now_ms: BASE_TS,
      retention_window_ms: 60_000,
      max_bytes: 1_500,
    });

    expect(pruned.some((event) => event.event_id === "old")).toBe(false);
    expect(pruned.length).toBe(1);
    expect(pruned[0].event_id).toBe("new");
  });

  it("counts recent high-severity activity and critical hits", () => {
    const events = [
      makeEvent({ event_id: "critical-now", severity: "critical", ts_unix_ms: BASE_TS }),
      makeEvent({ event_id: "high-now", severity: "high", ts_unix_ms: BASE_TS - 1_000 }),
      makeEvent({ event_id: "low-old", severity: "low", ts_unix_ms: BASE_TS - 100_000 }),
    ];

    expect(countRecentHighSeverityEvents(events, BASE_TS, 60_000)).toBe(2);
    expect(hasCriticalEventWithinWindow(events, BASE_TS, 60_000)).toBe(true);
  });

  it("handles very large payloads with summary-first normalization", () => {
    const huge = "x".repeat(200_000);
    const normalized = normalizeLiveFeedEvent(
      {
        schema_version: "v1",
        event_id: "evt-large",
        event_type: "gateway.notice",
        ts_unix_ms: BASE_TS,
        entity: "system",
        payload: {
          summary: "large payload received",
          domain: "system",
          severity: "normal",
          blob: huge,
        },
      },
      99
    );

    expect(normalized.summary).toBe("large payload received");
    expect(typeof normalized.payload_redacted.blob).toBe("string");
  });
});
