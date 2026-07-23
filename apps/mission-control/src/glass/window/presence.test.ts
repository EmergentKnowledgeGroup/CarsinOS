import { describe, expect, it } from "vitest";

import {
  isExecassPresence,
  presenceFreshness,
  presenceTargetDestination,
  sortPresence,
} from "./presence";
import type { FloorPresenceItem } from "./types";

const NOW = 1_800_000_000_000;

function item(overrides: Partial<FloorPresenceItem> = {}): FloorPresenceItem {
  return {
    agent_id: "agent-1",
    display_name: "Scout",
    activity: "busy",
    activity_label: "Working",
    mood: "focused",
    observed_at_ms: NOW - 1_000,
    source: "local_storage",
    target: null,
    ...overrides,
  };
}

describe("presenceFreshness", () => {
  it("is honest about missing observations", () => {
    const fresh = presenceFreshness(null, NOW);
    expect(fresh.tone).toBe("unknown");
    expect(fresh.label).toBe("No recent observation");
  });

  it("labels very recent observations as just now", () => {
    const fresh = presenceFreshness(NOW - 30_000, NOW);
    expect(fresh.tone).toBe("fresh");
    expect(fresh.label).toBe("Observed just now");
  });

  it("reports minutes and flags stale observations", () => {
    const threeMinutes = presenceFreshness(NOW - 3 * 60_000, NOW);
    expect(threeMinutes.tone).toBe("stale");
    expect(threeMinutes.label).toBe("Observed 3m ago");
  });

  it("reports hours for old observations", () => {
    const twoHours = presenceFreshness(NOW - 2 * 60 * 60_000, NOW);
    expect(twoHours.tone).toBe("stale");
    expect(twoHours.label).toBe("Observed 2h ago");
  });

  it("never claims a future observation", () => {
    const skewed = presenceFreshness(NOW + 60_000, NOW);
    expect(skewed.tone).toBe("fresh");
    expect(skewed.label).toBe("Observed just now");
  });
});

describe("presenceTargetDestination", () => {
  it("routes delegations and sessions to the Office floor", () => {
    expect(
      presenceTargetDestination({ kind: "delegation", id: "dlg-1" }),
    ).toEqual({
      kind: "office",
      tab: "assistant",
      label: "Go to the Office",
      delegationId: "dlg-1",
    });
    expect(presenceTargetDestination({ kind: "session", id: "s-1" })).toEqual({
      kind: "session",
      tab: "assistant",
      label: "Open the conversation",
      sessionId: "s-1",
    });
  });

  it("routes runs to the run history", () => {
    expect(presenceTargetDestination({ kind: "run", id: "run-1" })).toEqual({
      kind: "runbook",
      tab: "runbook",
      label: "Open the run history",
      runbookKind: "assistant_session_run",
      anchorId: "run-1",
    });
  });

  it("returns null for missing targets", () => {
    expect(presenceTargetDestination(null)).toBeNull();
  });
});

describe("isExecassPresence / sortPresence", () => {
  it("recognizes ExecAss by display name only", () => {
    expect(isExecassPresence(item({ display_name: "ExecAss" }))).toBe(true);
    expect(isExecassPresence(item({ display_name: "Scout" }))).toBe(false);
  });

  it("puts ExecAss first and keeps the rest in given order", () => {
    const items = [
      item({ agent_id: "a", display_name: "Scout" }),
      item({ agent_id: "b", display_name: "ExecAss" }),
      item({ agent_id: "c", display_name: "Archivist" }),
    ];
    expect(sortPresence(items).map((entry) => entry.agent_id)).toEqual([
      "b",
      "a",
      "c",
    ]);
    expect(items.map((entry) => entry.agent_id)).toEqual(["a", "b", "c"]);
  });
});
