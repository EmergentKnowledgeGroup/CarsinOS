import { describe, expect, test } from "vitest";

import type { FloorPresenceItem } from "./types";

describe("Floor presence contract", () => {
  test("represents every authoritative presence state and deep-link target", () => {
    const items: FloorPresenceItem[] = [
      {
        agent_id: "agent-busy",
        display_name: "Busy",
        activity: "busy",
        activity_label: "Working",
        mood: "focused",
        observed_at_ms: 1,
        source: "local_storage",
        target: { kind: "run", id: "run-1" },
      },
      {
        agent_id: "agent-recovering",
        display_name: "Recovering",
        activity: "recovering",
        activity_label: "Recovering",
        mood: "recovering",
        observed_at_ms: 2,
        source: "local_storage",
        target: { kind: "delegation", id: "delegation-1" },
      },
      {
        agent_id: "agent-offline",
        display_name: "Offline",
        activity: "offline",
        activity_label: "Offline",
        mood: "offline",
        observed_at_ms: 3,
        source: "local_storage",
        target: { kind: "session", id: "session-1" },
      },
      {
        agent_id: "agent-unknown",
        display_name: "Unknown",
        activity: "unknown",
        activity_label: "No recent observation",
        mood: "unknown",
        observed_at_ms: null,
        source: "local_storage",
        target: null,
      },
    ];

    expect(items.map((item) => item.activity)).toEqual([
      "busy",
      "recovering",
      "offline",
      "unknown",
    ]);
    expect(items.map((item) => item.target?.kind)).toEqual([
      "run",
      "delegation",
      "session",
      undefined,
    ]);
  });
});
