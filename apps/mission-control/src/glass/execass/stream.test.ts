import { beforeEach, describe, expect, test } from "vitest";

import { fixtureEventEnvelope } from "./fixtures";
import {
  buildResumeFrame,
  initialStreamState,
  invalidationTargets,
  loadStreamCursor,
  reduceFrame,
  resumeAfterRefetch,
  saveStreamCursor,
} from "./stream";
import type { ExecassWsFrame } from "./types";

const eventFrame = (sequence: number, dup = `dup-${sequence}`): ExecassWsFrame => ({
  type: "execass.v1.event",
  event: fixtureEventEnvelope({
    global_sequence: sequence,
    duplicate_identity: dup,
  }),
});

const refetchFrame = (
  consumer: number,
  requested: number,
): ExecassWsFrame => ({
  type: "execass.v1.summary_refetch_required",
  reason: "gap",
  consumer_cursor: consumer,
  requested_cursor: requested,
  head_global_sequence: 9999,
});

describe("reduceFrame", () => {
  test("applies a fresh event and advances the cursor", () => {
    const state = initialStreamState(1000);
    const { state: next, effect } = reduceFrame(state, eventFrame(1001));
    expect(effect.kind).toBe("apply-event");
    expect(next.cursor).toBe(1001);
  });

  test("ignores an event at or below the durable cursor", () => {
    const state = initialStreamState(1000);
    const { state: next, effect } = reduceFrame(state, eventFrame(1000));
    expect(effect.kind).toBe("ignore");
    expect(next.cursor).toBe(1000);
  });

  test("deduplicates by duplicate_identity even when the sequence is new", () => {
    const state = initialStreamState(1000);
    const first = reduceFrame(state, eventFrame(1001, "dup-x"));
    const second = reduceFrame(first.state, eventFrame(1002, "dup-x"));
    expect(second.effect.kind).toBe("ignore");
    expect(second.state.cursor).toBe(1001);
  });

  test("fails closed on a forward gap instead of advancing past unseen events", () => {
    const state = initialStreamState(1000);
    const { state: next, effect } = reduceFrame(state, eventFrame(1002));
    expect(effect.kind).toBe("refetch-summary");
    expect(next.refetchRequired).toBe(true);
    expect(next.cursor).toBe(1000);
    expect(next.resumeCursor).toBe(1000);
  });

  test("an out-of-order late event cannot move the cursor after a detected gap", () => {
    const gap = reduceFrame(initialStreamState(1000), eventFrame(1002)).state;
    const { state: next, effect } = reduceFrame(gap, eventFrame(1001));
    expect(effect.kind).toBe("ignore");
    expect(next.cursor).toBe(1000);
    expect(next.refetchRequired).toBe(true);
  });

  test("a refetch frame demands a summary refetch and records ONLY consumer_cursor", () => {
    const state = initialStreamState(1000);
    const { state: next, effect } = reduceFrame(state, refetchFrame(950, 1000));
    expect(effect.kind).toBe("refetch-summary");
    expect(next.refetchRequired).toBe(true);
    // never requested_cursor (1000) or head_global_sequence (9999)
    expect(next.resumeCursor).toBe(950);
  });

  test("events arriving while a refetch is pending are ignored, not applied speculatively", () => {
    const state = initialStreamState(1000);
    const refetched = reduceFrame(state, refetchFrame(950, 1000)).state;
    const { effect } = reduceFrame(refetched, eventFrame(1001));
    expect(effect.kind).toBe("ignore");
  });
});

describe("resumeAfterRefetch", () => {
  test("clears the refetch flag and resumes from the exact consumer cursor", () => {
    const state = reduceFrame(initialStreamState(1000), refetchFrame(950, 1000)).state;
    const resumed = resumeAfterRefetch(state);
    expect(resumed.refetchRequired).toBe(false);
    expect(resumed.cursor).toBe(950);
    expect(resumed.resumeCursor).toBeNull();
  });
});

describe("buildResumeFrame", () => {
  test("builds the wire resume frame for the stable client identity", () => {
    expect(buildResumeFrame("mission-control-desktop", 950)).toEqual({
      type: "execass.v1.resume",
      client_id: "mission-control-desktop",
      cursor: 950,
    });
  });
});

describe("cursor persistence", () => {
  beforeEach(() => localStorage.clear());

  test("round-trips the last durably handled cursor per client identity", () => {
    saveStreamCursor("client-a", 1234);
    saveStreamCursor("client-b", 9);
    expect(loadStreamCursor("client-a")).toBe(1234);
    expect(loadStreamCursor("client-b")).toBe(9);
  });

  test("returns 0 for an unknown client or corrupt value", () => {
    localStorage.setItem("mc-execass-cursor-v1:bad", "not-a-number");
    expect(loadStreamCursor("unknown")).toBe(0);
    expect(loadStreamCursor("bad")).toBe(0);
  });
});

describe("invalidationTargets", () => {
  test("summary changes invalidate the summary projection", () => {
    const envelope = fixtureEventEnvelope({
      event_name: "execass.v1.summary.changed",
    });
    expect(invalidationTargets(envelope)).toContain("summary");
  });

  test("delegation transitions invalidate summary and that delegation's detail", () => {
    const envelope = fixtureEventEnvelope({
      event_name: "execass.v1.delegation.transitioned",
      safe_payload: {
        summary: "moved",
        delegation_id: "dlg-1",
        decision_id: null,
        receipt_ref: null,
        authoritative_deep_link: null,
      },
    });
    const targets = invalidationTargets(envelope);
    expect(targets).toContain("summary");
    expect(targets).toContain("delegation-detail");
  });

  test("policy, runtime-host, stop, integrity, and notification events map to their surfaces", () => {
    expect(
      invalidationTargets(
        fixtureEventEnvelope({ event_name: "execass.v1.policy.changed" }),
      ),
    ).toContain("policy");
    expect(
      invalidationTargets(
        fixtureEventEnvelope({ event_name: "execass.v1.runtime_host.changed" }),
      ),
    ).toContain("runtime-host");
    expect(
      invalidationTargets(
        fixtureEventEnvelope({ event_name: "execass.v1.global_stop.changed" }),
      ),
    ).toContain("stop-all");
    expect(
      invalidationTargets(
        fixtureEventEnvelope({
          event_name: "execass.v1.receipt.integrity_failed",
        }),
      ),
    ).toContain("integrity");
    expect(
      invalidationTargets(
        fixtureEventEnvelope({
          event_name: "execass.v1.notification.scheduled",
        }),
      ),
    ).toContain("notifications");
  });
});
