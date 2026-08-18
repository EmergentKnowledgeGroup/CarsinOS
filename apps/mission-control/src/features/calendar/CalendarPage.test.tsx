// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MissionControlCalendarJob, MissionControlCalendarWeekResponse } from "../../types";
import { CalendarPage } from "./CalendarPage";

type TestGlobal = typeof globalThis & {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
};

const job: MissionControlCalendarJob = {
  job_id: "job-calendar-safety",
  name: "Daily digest",
  agent_id: "agent-1",
  enabled: true,
  schedule_kind: "interval",
  interval_seconds: 3600,
  cron_expr: null,
  next_run_at: Date.now() + 3600000,
  last_run_at: null,
  last_error: null,
  lane: "always_running",
  primary_action: "Review digest queue",
};

const calendarWeek: MissionControlCalendarWeekResponse = {
  week_start_ms: Date.now(),
  week_end_ms: Date.now() + 7 * 86400000,
  generated_at_ms: Date.now(),
  always_running: [job],
  next_up: [job],
  jobs: [job],
};

describe("CalendarPage safety", () => {
  let container: HTMLDivElement | null;
  let root: Root | null;
  let previousActEnvironment: boolean | undefined;

  beforeEach(() => {
    localStorage.clear();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    previousActEnvironment = (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT;
    (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0);
      return 0;
    });
    vi.stubGlobal("cancelAnimationFrame", () => undefined);
  });

  afterEach(() => {
    if (root) {
      act(() => {
        root?.unmount();
      });
    }
    container?.remove();
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
    root = null;
    container = null;
    if (previousActEnvironment === undefined) {
      delete (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT;
    } else {
      (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT = previousActEnvironment;
    }
  });

  it("opens details without running from week or always-running selection, then runs once explicitly", async () => {
    if (!container || !root) {
      throw new Error("test container was not initialized");
    }
    const onRunCalendarJobNow = vi.fn(async () => undefined);

    await act(async () => {
      root?.render(
        <CalendarPage
          calendarWeek={calendarWeek}
          calendarAlwaysRunning={[job]}
          calendarNextUp={[job]}
          calendarJobs={[job]}
          agents={[]}
          execAssAgentId={null}
          onRunCalendarJobNow={onRunCalendarJobNow}
          onToggleCalendarJob={vi.fn(async () => undefined)}
          onLoadCalendarJobHistory={vi.fn(async () => [])}
          onCreateExecAssHeartbeatJob={vi.fn(async () => undefined)}
          strategyReady={false}
          taskByJobId={new Map()}
          describeStrategyTask={() => null}
          onOpenStrategyTask={() => false}
          runbookEnabled={false}
          runbookByJobId={new Map()}
          onOpenJobRunbook={() => false}
        />
      );
    });

    const click = async (selector: string) => {
      const element = container?.querySelector<HTMLElement>(selector);
      if (!element) {
        throw new Error(`missing ${selector}`);
      }
      await act(async () => {
        element.click();
      });
    };

    await click(".mc-cal-job-block");
    expect(onRunCalendarJobNow).not.toHaveBeenCalled();
    expect(container.querySelector('[role="dialog"]')?.textContent).toContain(
      "Nothing runs until you choose Run now"
    );

    await click('[aria-label="Close"]');
    await click(".mc-cal-always-chip");
    expect(onRunCalendarJobNow).not.toHaveBeenCalled();
    await click('[aria-label="Close"]');

    await click(".mc-cal-job-block");
    await click('[aria-label="Run job now"]');
    await act(async () => {
      await Promise.resolve();
    });
    expect(onRunCalendarJobNow).toHaveBeenCalledTimes(1);
  });

  it("keeps every Calendar surface reachable and offers Pin to Office without new mutations", async () => {
    if (!container || !root) {
      throw new Error("test container was not initialized");
    }
    const onRunCalendarJobNow = vi.fn(async () => undefined);
    const onToggleCalendarJob = vi.fn(async () => undefined);
    const onCreateExecAssHeartbeatJob = vi.fn(async () => undefined);

    await act(async () => {
      root?.render(
        <CalendarPage
          calendarWeek={calendarWeek}
          calendarAlwaysRunning={[job]}
          calendarNextUp={[job]}
          calendarJobs={[job]}
          agents={[]}
          execAssAgentId={null}
          onRunCalendarJobNow={onRunCalendarJobNow}
          onToggleCalendarJob={onToggleCalendarJob}
          onLoadCalendarJobHistory={vi.fn(async () => [])}
          onCreateExecAssHeartbeatJob={onCreateExecAssHeartbeatJob}
          strategyReady={false}
          taskByJobId={new Map()}
          describeStrategyTask={() => null}
          onOpenStrategyTask={() => false}
          runbookEnabled={false}
          runbookByJobId={new Map()}
          onOpenJobRunbook={() => false}
        />
      );
    });

    // Parity anchors: the heartbeat panel and all three tabs stay present.
    expect(
      container.querySelector('[aria-label="ExecAss heartbeat setup"]')
    ).not.toBeNull();
    const tabLabels = Array.from(
      container.querySelectorAll('[role="tab"]')
    ).map((tab) => tab.textContent ?? "");
    expect(tabLabels.some((label) => label.includes("Week View"))).toBe(true);
    expect(tabLabels.some((label) => label.includes("Schedule"))).toBe(true);
    expect(tabLabels.some((label) => label.includes("Active Jobs"))).toBe(true);

    const scheduleTab = Array.from(
      container.querySelectorAll<HTMLButtonElement>('[role="tab"]')
    ).find((tab) => tab.textContent?.includes("Schedule"));
    await act(async () => scheduleTab?.click());
    expect(scheduleTab?.getAttribute("aria-selected")).toBe("true");
    expect(container.querySelector(".mc-table")).not.toBeNull();

    const activeTab = Array.from(
      container.querySelectorAll<HTMLButtonElement>('[role="tab"]')
    ).find((tab) => tab.textContent?.includes("Active Jobs"));
    await act(async () => activeTab?.click());
    expect(activeTab?.getAttribute("aria-selected")).toBe("true");
    expect(container.querySelector(".mc-cal-active")).not.toBeNull();

    // The pin affordance appears, and pressing it runs no job mutation.
    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) =>
        button.getAttribute("aria-label") === "Pin Calendar to Office"
    );
    expect(pin).toBeTruthy();
    await act(async () => {
      pin?.click();
    });
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas"
    );
    expect(onRunCalendarJobNow).not.toHaveBeenCalled();
    expect(onToggleCalendarJob).not.toHaveBeenCalled();
    expect(onCreateExecAssHeartbeatJob).not.toHaveBeenCalled();
  });
});
