import { describe, expect, it } from "vitest";
import {
  buildErrorReport,
  nextCrashWindow,
  shouldEnterSafeMode,
  type CrashWindowState,
} from "./errorRecovery";

describe("errorRecovery", () => {
  it("increments crash count inside window and resets outside window", () => {
    const base: CrashWindowState = {
      windowStartMs: 1_000,
      crashCount: 1,
    };

    expect(nextCrashWindow(base, 5_000, 10_000)).toEqual({
      windowStartMs: 1_000,
      crashCount: 2,
    });

    expect(nextCrashWindow(base, 20_500, 10_000)).toEqual({
      windowStartMs: 20_500,
      crashCount: 1,
    });
  });

  it("treats exact window boundary as outside the crash window", () => {
    const base: CrashWindowState = {
      windowStartMs: 1_000,
      crashCount: 2,
    };

    expect(nextCrashWindow(base, 11_000, 10_000)).toEqual({
      windowStartMs: 11_000,
      crashCount: 1,
    });
  });

  it("enters safe mode only at threshold", () => {
    expect(shouldEnterSafeMode(1, 3)).toBe(false);
    expect(shouldEnterSafeMode(2, 3)).toBe(false);
    expect(shouldEnterSafeMode(3, 3)).toBe(true);
  });

  it("builds redacted error reports with event context", () => {
    const error = new Error("token=sk-ant-abc123456");
    const report = buildErrorReport("Tab crash", error, "in Boards", [
      {
        event_id: "evt_1",
        event_type: "board.card.updated",
        entity: "board",
        ts_unix_ms: 10,
        payload: {
          title: "Ship patch",
        },
      },
    ]);

    expect(report).toContain("Tab crash");
    expect(report).toContain("[REDACTED]");
    expect(report).toContain("board.card.updated");
    expect(report).toContain("Card updated: Ship patch");
  });

  it("redacts sensitive tokens embedded inside event summaries", () => {
    const error = new Error("non-sensitive");
    const report = buildErrorReport("Tab crash", error, null, [
      {
        event_id: "evt_2",
        event_type: "board.card.updated",
        entity: "board",
        ts_unix_ms: 11,
        payload: {
          title: "sk-ant-sensitive-event-token",
        },
      },
    ]);

    expect(report).toContain("Card updated:");
    expect(report).toContain("[REDACTED]");
    expect(report).not.toContain("sk-ant-sensitive-event-token");
  });
});
