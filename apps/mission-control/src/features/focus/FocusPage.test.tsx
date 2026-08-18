// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  ChannelRuntimeAdapterStatusResponse,
  CircuitBreakerStateResponse,
  JobStatusResponse,
  MissionControlCalendarJob,
  MissionControlFocusItem,
  PluginRuntimeStatusResponse,
  RunbookSummaryItemResponse,
  TaskResponse,
} from "../../types";
import type { StrategyTaskContextSnapshot } from "../strategy/useStrategyController";
import { FocusPage } from "./FocusPage";

const NOW = Date.UTC(2026, 6, 25, 12, 0, 0);

function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function focusItem(
  overrides: Partial<MissionControlFocusItem> & { item_id: string },
): MissionControlFocusItem {
  return {
    category: "approval",
    severity: "high",
    title: "Approval waiting",
    detail: "An agent asked to run a tool.",
    primary_action: "resolve_approval",
    action_payload: { approval_id: "appr-1" },
    created_at: NOW - 3 * 60_000,
    ...overrides,
  };
}

function channelStatus(
  overrides: Partial<ChannelRuntimeAdapterStatusResponse> & { provider: string },
): ChannelRuntimeAdapterStatusResponse {
  return {
    lifecycle_state: "running",
    healthy: true,
    session_state: "active",
    proof_state: "proven",
    detail: null,
    proof_detail: null,
    last_error: null,
    last_inbound_at: null,
    last_outbound_at: null,
    last_proven_at: null,
    reconnect_attempts: 0,
    updated_at: NOW - 60_000,
    ...overrides,
  };
}

function taskFixture(
  overrides: Partial<TaskResponse> & { task_id: string },
): TaskResponse {
  return {
    project_id: "proj-1",
    parent_task_id: null,
    title: "Linked strategy task",
    detail: "Task detail",
    status: "in_progress",
    priority: "high",
    owner_agent_id: null,
    due_at: null,
    blocked_reason: null,
    linked_board_card_id: null,
    linked_job_id: null,
    latest_run_id: null,
    latest_session_id: null,
    created_at: NOW - 60 * 60_000,
    updated_at: NOW - 10 * 60_000,
    ...overrides,
  };
}

function runbookFixture(
  overrides: Partial<RunbookSummaryItemResponse> & { runbook_id: string },
): RunbookSummaryItemResponse {
  return {
    runbook_kind: "scheduled_job_run",
    anchor_kind: "job",
    anchor_id: "job-1",
    title: "Job runbook",
    status: "running",
    status_reason: null,
    owner_agent_id: null,
    owner_agent_label: null,
    primary_entity_label: "Nightly digest",
    updated_at_ms: NOW - 5 * 60_000,
    current_step_label: null,
    warning_count: 0,
    linked_entities: [],
    availability: {
      is_limited: false,
      is_stale: false,
      last_refresh_at_ms: NOW,
      missing_source_kinds: [],
      stale_reason: null,
    },
    ...overrides,
  };
}

function breakerFixture(
  overrides?: Partial<CircuitBreakerStateResponse>,
): CircuitBreakerStateResponse {
  return {
    scope: "provider",
    target_id: "openai",
    state: "open",
    consecutive_failures: 4,
    cooldown_until: NOW + 5 * 60_000,
    last_error_code: "timeout",
    updated_at: NOW - 2 * 60_000,
    ...overrides,
  };
}

function pluginBreakerFixture(
  overrides?: Partial<PluginRuntimeStatusResponse>,
): PluginRuntimeStatusResponse {
  return {
    plugin_id: "webhook-bridge",
    enabled: true,
    faulted: true,
    disabled_until_ms: NOW + 10 * 60_000,
    consecutive_failures: 3,
    last_error_code: "spawn_failed",
    last_error: "binary exited with code 1",
    last_success_ms: NOW - 60 * 60_000,
    last_invoked_ms: NOW - 4 * 60_000,
    ...overrides,
  };
}

function jobsStatusFixture(
  overrides?: Partial<JobStatusResponse>,
): JobStatusResponse {
  return {
    scheduler_running: true,
    scheduler_lock: {
      enabled: true,
      lock_path: "/var/lib/carsinos/scheduler.lock",
      owner: "gateway-1",
      detail: null,
    },
    jobs_total: 5,
    jobs_enabled: 3,
    jobs_due: 1,
    open_circuit_breakers: 0,
    circuit_breakers: [],
    top_stop_reasons: [],
    now_utc: "2026-07-25T12:00:00Z",
    ...overrides,
  };
}

function calendarJobFixture(
  overrides: Partial<MissionControlCalendarJob> & { job_id: string },
): MissionControlCalendarJob {
  return {
    name: "Nightly digest",
    agent_id: "agent-exec",
    enabled: true,
    schedule_kind: "cron",
    interval_seconds: null,
    cron_expr: "0 3 * * *",
    next_run_at: NOW + 60 * 60_000,
    last_run_at: NOW - 23 * 60 * 60_000,
    last_error: null,
    lane: "default",
    primary_action: "pause",
    ...overrides,
  };
}

type FocusPageProps = Parameters<typeof FocusPage>[0];

function baseProps(overrides?: Partial<FocusPageProps>): FocusPageProps {
  return {
    activeRoomId: null,
    focusItems: [],
    approvalsCount: 0,
    channelStatuses: [],
    onResolveFocusApproval: vi.fn(async () => {}),
    onRunCalendarJobNow: vi.fn(async () => {}),
    onReconnectFocusChannel: vi.fn(async () => {}),
    strategyReady: false,
    approvalTaskByApprovalId: new Map<string, TaskResponse>(),
    taskById: new Map<string, TaskResponse>(),
    taskByJobId: new Map<string, TaskResponse>(),
    describeStrategyTask: () => null,
    onOpenStrategyTask: vi.fn(() => true),
    runbookEnabled: false,
    getRunbookForFocusItem: () => null,
    onOpenRunbookForFocusItem: vi.fn(() => true),
    jobsStatus: null,
    openBreakers: [],
    openPluginBreakers: [],
    calendarJobs: [],
    onToggleCalendarJob: vi.fn(async () => {}),
    queueIntent: { nonce: 0, targetKind: null, targetId: null },
    ...overrides,
  };
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  vi.useFakeTimers({ now: NOW });
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
  localStorage.clear();
  vi.useRealTimers();
});

async function render(props: FocusPageProps) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(<FocusPage {...props} />);
  });
}

function items(): HTMLElement[] {
  return Array.from(container.querySelectorAll(".mc-focus-item"));
}

function subTab(label: string): HTMLButtonElement | undefined {
  return Array.from(
    container.querySelectorAll<HTMLButtonElement>(".mc-sub-tab"),
  ).find((button) => button.textContent?.startsWith(label));
}

function buttonIn(
  scope: ParentNode,
  label: string,
): HTMLButtonElement | undefined {
  return Array.from(scope.querySelectorAll<HTMLButtonElement>("button")).find(
    (button) => button.textContent === label,
  );
}

function paginationInfo(): string | null {
  return container.querySelector(".mc-pagination-info")?.textContent ?? null;
}

async function click(element: Element | undefined | null) {
  expect(element, "expected element to click").toBeTruthy();
  await act(async () => (element as HTMLElement).click());
}

describe("FocusPage queue parity", () => {
  it("shows severity, category, relative time, title, and detail for a queue item", async () => {
    await render(
      baseProps({
        focusItems: [
          focusItem({
            item_id: "item-1",
            severity: "critical",
            category: "approval",
            title: "Dangerous tool call",
            detail: "exec wants to run",
          }),
        ],
      }),
    );

    expect(subTab("Queue")?.getAttribute("aria-selected")).toBe("true");
    expect(subTab("Queue")?.querySelector(".mc-sub-tab-count")?.textContent).toBe("1");
    const item = items()[0];
    expect(item?.className).toContain("critical");
    expect(item?.textContent).toContain("critical");
    expect(item?.textContent).toContain("approval");
    expect(item?.textContent).toContain("3m ago");
    expect(item?.querySelector("h3")?.textContent).toBe("Dangerous tool call");
    expect(item?.textContent).toContain("exec wants to run");
    expect(container.textContent).toContain("1 open attention items");
  });

  it("approves through the real callback and locks out same-tick duplicate clicks", async () => {
    const gate = deferred();
    const onResolveFocusApproval = vi.fn(() => gate.promise);
    await render(
      baseProps({
        focusItems: [focusItem({ item_id: "item-1" })],
        onResolveFocusApproval,
      }),
    );

    const approve = buttonIn(items()[0]!, "Approve");
    await act(async () => {
      approve!.click();
      approve!.click();
    });
    expect(onResolveFocusApproval).toHaveBeenCalledTimes(1);
    expect(onResolveFocusApproval).toHaveBeenCalledWith("appr-1", "approve");

    // Busy state is honest while the request is in flight.
    const busyButtons = Array.from(
      items()[0]!.querySelectorAll<HTMLButtonElement>("button"),
    ).filter((button) => button.textContent === "Working...");
    expect(busyButtons.length).toBeGreaterThan(0);
    expect(busyButtons.every((button) => button.disabled)).toBe(true);

    await act(async () => {
      gate.resolve();
    });
    expect(buttonIn(items()[0]!, "Approve")).toBeTruthy();
    expect(buttonIn(items()[0]!, "Approve")?.disabled).toBe(false);
  });

  it("denies through the same seam", async () => {
    const onResolveFocusApproval = vi.fn(async () => {});
    await render(
      baseProps({
        focusItems: [focusItem({ item_id: "item-1" })],
        onResolveFocusApproval,
      }),
    );

    await click(buttonIn(items()[0]!, "Deny"));
    expect(onResolveFocusApproval).toHaveBeenCalledWith("appr-1", "deny");
  });

  it("disables approval actions honestly when no approval ID is linked", async () => {
    await render(
      baseProps({
        focusItems: [
          focusItem({ item_id: "item-1", action_payload: {} }),
        ],
      }),
    );

    const approve = buttonIn(items()[0]!, "Approve");
    const deny = buttonIn(items()[0]!, "Deny");
    expect(approve?.disabled).toBe(true);
    expect(approve?.getAttribute("title")).toBe("No approval ID linked");
    expect(deny?.disabled).toBe(true);
  });

  it("keeps action failures on the item and clears them on retry", async () => {
    const onResolveFocusApproval = vi
      .fn<(approvalId: string, decision: "approve" | "deny") => Promise<void>>()
      .mockRejectedValueOnce(new Error("gateway said no"))
      .mockResolvedValueOnce(undefined);
    await render(
      baseProps({
        focusItems: [focusItem({ item_id: "item-1" })],
        onResolveFocusApproval,
      }),
    );

    await click(buttonIn(items()[0]!, "Approve"));
    const alert = items()[0]!.querySelector('[role="alert"]');
    expect(alert?.textContent).toContain("Action failed: gateway said no");
    // The item stays in the queue with its actions re-enabled.
    expect(buttonIn(items()[0]!, "Approve")?.disabled).toBe(false);

    await click(buttonIn(items()[0]!, "Approve"));
    expect(items()[0]!.querySelector('[role="alert"]')).toBeNull();
    expect(onResolveFocusApproval).toHaveBeenCalledTimes(2);
  });

  it("retries a failed job through the run-now seam", async () => {
    const onRunCalendarJobNow = vi.fn(async () => {});
    await render(
      baseProps({
        focusItems: [
          focusItem({
            item_id: "item-job",
            category: "run_failure",
            title: "Job failed",
            action_payload: { job_id: "job-9" },
          }),
        ],
        onRunCalendarJobNow,
      }),
    );

    await click(buttonIn(items()[0]!, "Retry Job"));
    expect(onRunCalendarJobNow).toHaveBeenCalledWith("job-9");
  });

  it("reconnects a channel from a channel_health item", async () => {
    const onReconnectFocusChannel = vi.fn(async () => {});
    await render(
      baseProps({
        focusItems: [
          focusItem({
            item_id: "item-chan",
            category: "channel_health",
            title: "Discord degraded",
            action_payload: { provider: "discord" },
          }),
        ],
        onReconnectFocusChannel,
      }),
    );

    await click(buttonIn(items()[0]!, "Reconnect Channel"));
    expect(onReconnectFocusChannel).toHaveBeenCalledWith("discord");
  });

  it("expands details with recursive secret redaction across the rendered surface", async () => {
    await render(
      baseProps({
        focusItems: [
          focusItem({
            item_id: "item-1",
            action_payload: {
              approval_id: "appr-1",
              tool_name: "exec",
              tool_input: {
                api_key: "sk-live-very-secret",
                nested: { client_secret: "nested-secret" },
                argv: ["ls", "-la"],
              },
              request_summary: "Authorization: Bearer abc.def.ghi",
              custom_note: "plain public fact",
            },
          }),
        ],
      }),
    );

    const toggle = buttonIn(items()[0]!, "Show details");
    expect(items()[0]!.querySelector(".mc-focus-context")).toBeNull();
    await click(toggle);

    const context = items()[0]!.querySelector(".mc-focus-context");
    expect(context).toBeTruthy();
    expect(context?.textContent).toContain("Tool");
    expect(context?.textContent).toContain("exec");
    expect(context?.textContent).toContain("Arguments");
    expect(context?.textContent).toContain("custom_note");
    expect(context?.textContent).toContain("plain public fact");
    // Redaction must hold for the whole rendered surface, not one field.
    expect(container.textContent).toContain("[REDACTED]");
    expect(container.textContent).not.toContain("sk-live-very-secret");
    expect(container.textContent).not.toContain("nested-secret");
    expect(container.textContent).not.toContain("abc.def.ghi");
    // Multi-line JSON context renders preformatted.
    expect(context?.querySelector("pre")).toBeTruthy();

    await click(buttonIn(items()[0]!, "Hide details"));
    expect(items()[0]!.querySelector(".mc-focus-context")).toBeNull();
  });

  it("offers no details toggle when the payload has nothing beyond routing IDs", async () => {
    await render(
      baseProps({
        focusItems: [
          focusItem({
            item_id: "item-1",
            action_payload: { approval_id: "appr-1" },
          }),
        ],
      }),
    );

    expect(buttonIn(items()[0]!, "Show details")).toBeUndefined();
  });

  it("links Strategy tasks by payload task_id, approval, and job in that order", async () => {
    const byTaskId = taskFixture({ task_id: "task-direct", title: "Direct task" });
    const byApproval = taskFixture({ task_id: "task-approval", title: "Approval task" });
    const byJob = taskFixture({ task_id: "task-job", title: "Job task" });
    const onOpenStrategyTask = vi.fn(() => true);
    await render(
      baseProps({
        strategyReady: true,
        focusItems: [
          focusItem({
            item_id: "item-direct",
            action_payload: { approval_id: "appr-1", task_id: "task-direct" },
          }),
          focusItem({
            item_id: "item-approval",
            action_payload: { approval_id: "appr-2" },
          }),
          focusItem({
            item_id: "item-job",
            category: "run_failure",
            action_payload: { job_id: "job-9" },
          }),
        ],
        taskById: new Map([[byTaskId.task_id, byTaskId]]),
        approvalTaskByApprovalId: new Map([["appr-2", byApproval]]),
        taskByJobId: new Map([["job-9", byJob]]),
        describeStrategyTask: (taskId: string): StrategyTaskContextSnapshot | null =>
          taskId === "task-direct"
            ? {
                task: byTaskId,
                project: null,
                goal: null,
                owner: null,
                managerChain: [],
              }
            : null,
        onOpenStrategyTask,
      }),
    );

    const [direct, viaApproval, viaJob] = items();
    expect(direct?.textContent).toContain("Direct task");
    expect(viaApproval?.textContent).toContain("Approval task");
    expect(viaJob?.textContent).toContain("Job task");

    await click(buttonIn(direct!, "Open task"));
    expect(onOpenStrategyTask).toHaveBeenCalledWith("task-direct");
  });

  it("renders no Strategy panel when strategy is not ready", async () => {
    const byTaskId = taskFixture({ task_id: "task-direct", title: "Direct task" });
    await render(
      baseProps({
        strategyReady: false,
        focusItems: [
          focusItem({
            item_id: "item-direct",
            action_payload: { approval_id: "appr-1", task_id: "task-direct" },
          }),
        ],
        taskById: new Map([[byTaskId.task_id, byTaskId]]),
      }),
    );

    expect(container.querySelector(".mc-focus-strategy-panel")).toBeNull();
    expect(container.textContent).not.toContain("Direct task");
  });

  it("links the Runbook panel and Open Runbook action through the real seams", async () => {
    const summary = runbookFixture({ runbook_id: "rb-1" });
    const item = focusItem({
      item_id: "item-job",
      category: "run_failure",
      action_payload: { job_id: "job-1" },
    });
    const getRunbookForFocusItem = vi.fn(() => summary);
    const onOpenRunbookForFocusItem = vi.fn<
      (item: MissionControlFocusItem) => boolean
    >(() => true);
    await render(
      baseProps({
        runbookEnabled: true,
        focusItems: [item],
        getRunbookForFocusItem,
        onOpenRunbookForFocusItem,
      }),
    );

    expect(container.querySelector(".mc-focus-runbook-panel")).toBeTruthy();
    expect(container.textContent).toContain("Nightly digest");
    await click(buttonIn(items()[0]!, "Open Runbook"));
    expect(onOpenRunbookForFocusItem).toHaveBeenCalledTimes(1);
    expect(onOpenRunbookForFocusItem.mock.calls[0]?.[0]).toMatchObject({
      item_id: "item-job",
    });
  });

  it("hides Runbook affordances when the Runbook feature is disabled", async () => {
    const summary = runbookFixture({ runbook_id: "rb-1" });
    await render(
      baseProps({
        runbookEnabled: false,
        focusItems: [
          focusItem({
            item_id: "item-job",
            category: "run_failure",
            action_payload: { job_id: "job-1" },
          }),
        ],
        getRunbookForFocusItem: () => summary,
      }),
    );

    expect(container.querySelector(".mc-focus-runbook-panel")).toBeNull();
    expect(buttonIn(items()[0]!, "Open Runbook")).toBeUndefined();
  });

  it("shows the all-clear empty state when the queue is empty", async () => {
    await render(baseProps());
    expect(container.textContent).toContain("No focus items — all clear.");
    expect(container.querySelector(".mc-pagination")).toBeNull();
  });

  it("paginates at exactly six items with honest boundaries", async () => {
    const focusItems = Array.from({ length: 13 }, (_, index) =>
      focusItem({
        item_id: `item-${String(index + 1).padStart(2, "0")}`,
        title: `Attention item ${index + 1}`,
        action_payload: { approval_id: `appr-${index + 1}` },
      }),
    );
    await render(baseProps({ focusItems }));

    expect(items()).toHaveLength(6);
    expect(items()[0]?.textContent).toContain("Attention item 1");
    expect(items()[5]?.textContent).toContain("Attention item 6");
    expect(paginationInfo()).toBe("1 / 3");
    expect(buttonIn(container, "Prev")?.disabled).toBe(true);

    await click(buttonIn(container, "Next"));
    expect(paginationInfo()).toBe("2 / 3");
    expect(items()[0]?.textContent).toContain("Attention item 7");
    expect(items()[5]?.textContent).toContain("Attention item 12");

    await click(buttonIn(container, "Next"));
    expect(paginationInfo()).toBe("3 / 3");
    expect(items()).toHaveLength(1);
    expect(items()[0]?.textContent).toContain("Attention item 13");
    expect(buttonIn(container, "Next")?.disabled).toBe(true);
  });

  it("keeps pagination state honest through live shrink, shrink, and regrowth", async () => {
    const focusItems = Array.from({ length: 25 }, (_, index) =>
      focusItem({
        item_id: `item-${String(index + 1).padStart(2, "0")}`,
        title: `Attention item ${index + 1}`,
        action_payload: { approval_id: `appr-${index + 1}` },
      }),
    );
    await render(baseProps({ focusItems }));

    for (let step = 0; step < 4; step += 1) {
      await click(buttonIn(container, "Next"));
    }
    expect(paginationInfo()).toBe("5 / 5");
    expect(items()[0]?.textContent).toContain("Attention item 25");

    // Live resolution shrinks the queue under the current page: the visible
    // rows and the stored page must both clamp.
    await render(baseProps({ focusItems: focusItems.slice(0, 13) }));
    expect(paginationInfo()).toBe("3 / 3");
    expect(items()).toHaveLength(1);
    expect(items()[0]?.textContent).toContain("Attention item 13");

    await render(baseProps({ focusItems: focusItems.slice(0, 5) }));
    expect(items()).toHaveLength(5);
    expect(container.querySelector(".mc-pagination")).toBeNull();

    // Regrowth must stay on page one instead of resurrecting the stale page.
    await render(baseProps({ focusItems: focusItems.slice(0, 13) }));
    expect(paginationInfo()).toBe("1 / 3");
    expect(items()[0]?.textContent).toContain("Attention item 1");
    await render(baseProps({ focusItems }));
    expect(paginationInfo()).toBe("1 / 5");
    expect(items()[0]?.textContent).toContain("Attention item 1");
  });
});

describe("FocusPage system status parity", () => {
  it("reports counts, channel posture, and degraded badge honestly", async () => {
    const channels = [
      channelStatus({ provider: "discord" }),
      channelStatus({
        provider: "telegram",
        healthy: false,
        lifecycle_state: "errored",
        last_error: "socket closed",
      }),
      channelStatus({
        provider: "slack",
        healthy: true,
        lifecycle_state: "starting",
        detail: "booting adapter",
      }),
    ];
    await render(
      baseProps({
        approvalsCount: 4,
        channelStatuses: channels,
      }),
    );

    // Two channels are degraded: unhealthy telegram plus non-running slack.
    expect(subTab("System Status")?.querySelector(".mc-sub-tab-count")?.textContent).toBe("2");
    await click(subTab("System Status"));
    expect(subTab("System Status")?.getAttribute("aria-selected")).toBe("true");

    const stats = container.querySelector(".mc-stat-list");
    expect(stats?.textContent).toContain("Pending approvals");
    expect(stats?.textContent).toContain("4");
    expect(stats?.textContent).toContain("Channel adapters");
    expect(stats?.textContent).toContain("3");
    expect(stats?.textContent).toContain("Degraded channels");
    expect(stats?.textContent).toContain("2");

    const cards = Array.from(container.querySelectorAll(".mc-channel-card"));
    expect(cards).toHaveLength(3);
    const discord = cards.find((card) => card.textContent?.includes("discord"));
    expect(discord?.textContent).toContain("healthy");
    expect(discord?.textContent).toContain("all systems go");
    const telegram = cards.find((card) => card.textContent?.includes("telegram"));
    expect(telegram?.textContent).toContain("degraded");
    expect(telegram?.textContent).toContain("socket closed");
    const slack = cards.find((card) => card.textContent?.includes("slack"));
    expect(slack?.textContent).toContain("booting adapter");
  });

  it("reconnects a channel card with duplicate-click lock and honest failure", async () => {
    const gate = deferred();
    const onReconnectFocusChannel = vi
      .fn<(provider: string) => Promise<void>>()
      .mockImplementationOnce(() => gate.promise)
      .mockRejectedValueOnce(new Error("adapter refused"));
    await render(
      baseProps({
        channelStatuses: [channelStatus({ provider: "discord" })],
        onReconnectFocusChannel,
      }),
    );
    await click(subTab("System Status"));

    const card = container.querySelector(".mc-channel-card")!;
    const reconnect = buttonIn(card, "Reconnect");
    await act(async () => {
      reconnect!.click();
      reconnect!.click();
    });
    expect(onReconnectFocusChannel).toHaveBeenCalledTimes(1);
    expect(onReconnectFocusChannel).toHaveBeenCalledWith("discord");
    expect(buttonIn(card, "Working...")?.disabled).toBe(true);
    await act(async () => {
      gate.resolve();
    });

    await click(buttonIn(container.querySelector(".mc-channel-card")!, "Reconnect"));
    expect(
      container.querySelector(".mc-channel-card [role=\"alert\"]")?.textContent,
    ).toContain("Action failed: adapter refused");
  });

  it("shows the empty adapters state", async () => {
    await render(baseProps());
    await click(subTab("System Status"));
    expect(container.textContent).toContain("No channel adapters registered.");
  });
});

describe("FocusPage breakers section", () => {
  it("lands on Breakers when arriving in the breakers room, with Queue one tap away", async () => {
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
        focusItems: [focusItem({ item_id: "item-1" })],
      }),
    );

    expect(subTab("Breakers")?.getAttribute("aria-selected")).toBe("true");
    expect(container.textContent).toContain("Circuit breakers");

    await click(subTab("Queue"));
    expect(container.textContent).toContain("Operator Focus Queue");
  });

  it("a queue deep-link intent wins over the room landing", async () => {
    await render(baseProps({ activeRoomId: null }));
    expect(subTab("Queue")?.getAttribute("aria-selected")).toBe("true");

    // A deep link that targets queue content (e.g. Review approval from
    // Runbook) must outrank the breakers room landing in the same render.
    await render(
      baseProps({
        activeRoomId: "breakers",
        queueIntent: { nonce: 1, targetKind: null, targetId: null },
      }),
    );
    expect(subTab("Queue")?.getAttribute("aria-selected")).toBe("true");
    expect(container.textContent).toContain("Operator Focus Queue");

    // An unrelated rerender with the same nonce keeps the boss's choice.
    await click(subTab("Scheduler"));
    await render(
      baseProps({
        activeRoomId: "breakers",
        queueIntent: { nonce: 1, targetKind: null, targetId: null },
        approvalsCount: 3,
      }),
    );
    expect(subTab("Scheduler")?.getAttribute("aria-selected")).toBe("true");
  });

  it("opens the exact deep-linked queue entity on its page and expands its proof", async () => {
    const focusItems = Array.from({ length: 8 }, (_, index) =>
      focusItem({
        item_id: `item-${index + 1}`,
        title: `Approval waiting ${index + 1}`,
        action_payload: {
          approval_id: `appr-${index + 1}`,
          request_summary: `Approval proof ${index + 1}`,
        },
      }),
    );
    await render(
      baseProps({
        activeRoomId: null,
        focusItems,
      }),
    );
    expect(container.textContent).toContain("Approval waiting 1");
    expect(container.textContent).not.toContain("Approval waiting 8");

    await render(
      baseProps({
        activeRoomId: "breakers",
        focusItems,
        queueIntent: {
          nonce: 1,
          targetKind: "approval",
          targetId: "appr-8",
        },
      }),
    );

    expect(subTab("Queue")?.getAttribute("aria-selected")).toBe("true");
    expect(paginationInfo()).toBe("2 / 2");
    const target = Array.from(container.querySelectorAll(".mc-focus-item")).find(
      (item) => item.textContent?.includes("Approval waiting 8"),
    );
    expect(target).toBeTruthy();
    expect(target?.textContent).toContain("Approval");
    expect(target?.textContent).toContain("Approval proof 8");
    expect(target?.textContent).toContain("Hide details");
  });

  it("only a real room change forces the landing section", async () => {
    await render(baseProps({ activeRoomId: null }));
    expect(subTab("Queue")?.getAttribute("aria-selected")).toBe("true");

    // Entering the breakers room lands on the operations section.
    await render(baseProps({ activeRoomId: "breakers" }));
    expect(subTab("Breakers")?.getAttribute("aria-selected")).toBe("true");

    // The operator picks another section; unrelated rerenders keep it.
    await click(subTab("Scheduler"));
    await render(
      baseProps({ activeRoomId: "breakers", approvalsCount: 2 }),
    );
    expect(subTab("Scheduler")?.getAttribute("aria-selected")).toBe("true");
  });

  it("shows an honest waiting state before any breaker status has loaded", async () => {
    await render(baseProps({ activeRoomId: "breakers" }));

    expect(subTab("Breakers")?.querySelector(".mc-sub-tab-count")).toBeNull();
    expect(container.textContent).toContain(
      "Breaker status hasn't loaded yet",
    );
    expect(container.textContent).not.toContain("No open core breakers.");
    expect(container.textContent).not.toContain("No faulted plugin runtimes.");
    expect(container.textContent?.toLowerCase()).not.toContain("healthy");
  });

  it("presents core and plugin breaker facts distinctly and honestly", async () => {
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
        openBreakers: [breakerFixture()],
        openPluginBreakers: [pluginBreakerFixture()],
      }),
    );

    expect(subTab("Breakers")?.querySelector(".mc-sub-tab-count")?.textContent).toBe("2");

    const core = container.querySelector('[aria-label="Core breakers"]');
    expect(core?.textContent).toContain("provider");
    expect(core?.textContent).toContain("openai");
    expect(core?.textContent).toContain("open");
    expect(core?.textContent).toContain("4 consecutive failures");
    expect(core?.textContent).toContain("timeout");
    expect(core?.textContent).toContain("Cooldown until");

    const plugins = container.querySelector('[aria-label="Plugin runtimes"]');
    expect(plugins?.textContent).toContain("webhook-bridge");
    expect(plugins?.textContent).toContain("faulted");
    expect(plugins?.textContent).toContain("3 consecutive failures");
    expect(plugins?.textContent).toContain("binary exited with code 1");
    expect(plugins?.textContent).toContain("Disabled until");
  });

  it("renders loaded-empty separately from loading and never invents a reset", async () => {
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
      }),
    );

    expect(container.textContent).toContain("No open core breakers.");
    expect(container.textContent).toContain("No faulted plugin runtimes.");
    expect(container.textContent).not.toContain("hasn't loaded yet");
    // Recovery is automatic; a manual reset control must not exist.
    const resetControls = Array.from(
      container.querySelectorAll("button"),
    ).filter((button) => /reset/i.test(button.textContent ?? ""));
    expect(resetControls).toHaveLength(0);
    expect(container.textContent).toContain("no manual reset");
  });
});

describe("FocusPage scheduler section", () => {
  it("shows an honest waiting state before jobsStatus exists", async () => {
    await render(baseProps({ activeRoomId: "breakers" }));
    await click(subTab("Scheduler"));

    expect(container.textContent).toContain(
      "Scheduler status hasn't loaded yet",
    );
    expect(container.textContent).not.toContain("running");
    expect(container.textContent).not.toContain("stopped");
  });

  it("presents scheduler, lock, counts, and stop reasons from jobsStatus only", async () => {
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture({
          scheduler_running: true,
          scheduler_lock: {
            enabled: true,
            lock_path: "/var/lib/carsinos/scheduler.lock",
            owner: "gateway-1",
            detail: "held since boot",
          },
          jobs_total: 7,
          jobs_enabled: 4,
          jobs_due: 2,
          top_stop_reasons: [
            { code: "budget_exhausted", count: 3 },
            { code: "circuit_open", count: 1 },
          ],
        }),
      }),
    );
    await click(subTab("Scheduler"));

    const posture = container.querySelector(".mc-sched-posture");
    expect(posture?.textContent).toContain("running");
    expect(posture?.textContent).toContain("gateway-1");
    expect(posture?.textContent).toContain("held since boot");
    // Raw lock paths stay in the later File-lock machinery room.
    expect(container.textContent).not.toContain("/var/lib/carsinos/scheduler.lock");

    const stats = container.querySelector(".mc-sched-stats");
    expect(stats?.textContent).toContain("Jobs total");
    expect(stats?.textContent).toContain("7");
    expect(stats?.textContent).toContain("Enabled");
    expect(stats?.textContent).toContain("4");
    expect(stats?.textContent).toContain("Due now");
    expect(stats?.textContent).toContain("2");

    expect(container.textContent).toContain("budget_exhausted");
    expect(container.textContent).toContain("×3");
    expect(container.textContent).toContain("circuit_open");
    expect(container.textContent).toContain("×1");
  });

  it("shows stopped honestly and an empty stop-reason state", async () => {
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture({
          scheduler_running: false,
          scheduler_lock: {
            enabled: false,
            lock_path: "/tmp/lock",
            owner: "",
            detail: null,
          },
        }),
      }),
    );
    await click(subTab("Scheduler"));

    expect(container.querySelector(".mc-sched-posture")?.textContent).toContain(
      "stopped",
    );
    expect(container.textContent).toContain("Lock disabled");
    expect(container.textContent).toContain("No recorded stop reasons.");
  });

  it("runs a scheduled job with duplicate-click lock and honest failure copy", async () => {
    const gate = deferred();
    const onRunCalendarJobNow = vi
      .fn<(jobId: string) => Promise<void>>()
      .mockImplementationOnce(() => gate.promise)
      .mockRejectedValueOnce(new Error("scheduler refused"));
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
        calendarJobs: [calendarJobFixture({ job_id: "job-1" })],
        onRunCalendarJobNow,
      }),
    );
    await click(subTab("Scheduler"));

    const row = container.querySelector(".mc-sched-job")!;
    const run = buttonIn(row, "Run now");
    await act(async () => {
      run!.click();
      run!.click();
    });
    expect(onRunCalendarJobNow).toHaveBeenCalledTimes(1);
    expect(onRunCalendarJobNow).toHaveBeenCalledWith("job-1");
    expect(buttonIn(row, "Working...")?.disabled).toBe(true);
    await act(async () => {
      gate.resolve();
    });

    await click(buttonIn(container.querySelector(".mc-sched-job")!, "Run now"));
    expect(
      container.querySelector('.mc-sched-job [role="alert"]')?.textContent,
    ).toContain("Action failed: scheduler refused");
  });

  it("serializes Run and Pause for the same job with one synchronous lock", async () => {
    const runGate = deferred();
    const toggleGate = deferred();
    const onRunCalendarJobNow = vi.fn(() => runGate.promise);
    const onToggleCalendarJob = vi.fn(() => toggleGate.promise);
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
        calendarJobs: [calendarJobFixture({ job_id: "job-1", enabled: true })],
        onRunCalendarJobNow,
        onToggleCalendarJob,
      }),
    );
    await click(subTab("Scheduler"));

    const row = container.querySelector(".mc-sched-job")!;
    const run = buttonIn(row, "Run now")!;
    const pause = buttonIn(row, "Pause")!;
    await act(async () => {
      run.click();
      pause.click();
    });
    expect(onRunCalendarJobNow).toHaveBeenCalledTimes(1);
    expect(onToggleCalendarJob).not.toHaveBeenCalled();
    expect(Array.from(row.querySelectorAll("button")).every((button) => button.disabled)).toBe(
      true,
    );
    await act(async () => {
      runGate.resolve();
    });

    const refreshedRow = container.querySelector(".mc-sched-job")!;
    await act(async () => {
      buttonIn(refreshedRow, "Pause")!.click();
      buttonIn(refreshedRow, "Run now")!.click();
    });
    expect(onToggleCalendarJob).toHaveBeenCalledTimes(1);
    expect(onRunCalendarJobNow).toHaveBeenCalledTimes(1);
    await act(async () => {
      toggleGate.resolve();
    });
  });

  it("pauses and resumes through the existing toggle mutation", async () => {
    const onToggleCalendarJob = vi.fn(async () => {});
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
        calendarJobs: [
          calendarJobFixture({ job_id: "job-on", enabled: true }),
          calendarJobFixture({
            job_id: "job-off",
            name: "Weekly report",
            enabled: false,
            primary_action: "resume",
          }),
        ],
        onToggleCalendarJob,
      }),
    );
    await click(subTab("Scheduler"));

    const rows = Array.from(container.querySelectorAll(".mc-sched-job"));
    await click(buttonIn(rows[0]!, "Pause"));
    expect(onToggleCalendarJob).toHaveBeenCalledWith("job-on", false);

    await click(buttonIn(rows[1]!, "Resume"));
    expect(onToggleCalendarJob).toHaveBeenCalledWith("job-off", true);
  });

  it("paginates scheduled jobs at six with state-correct shrink and regrowth", async () => {
    const jobs = Array.from({ length: 13 }, (_, index) =>
      calendarJobFixture({
        job_id: `job-${String(index + 1).padStart(2, "0")}`,
        name: `Scheduled job ${index + 1}`,
      }),
    );
    const props = () =>
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
      });
    await render({ ...props(), calendarJobs: jobs });
    await click(subTab("Scheduler"));

    const jobRows = () => Array.from(container.querySelectorAll(".mc-sched-job"));
    expect(jobRows()).toHaveLength(6);
    expect(jobRows()[0]?.textContent).toContain("Scheduled job 1");
    expect(paginationInfo()).toBe("1 / 3");

    await click(buttonIn(container, "Next"));
    await click(buttonIn(container, "Next"));
    expect(paginationInfo()).toBe("3 / 3");
    expect(jobRows()).toHaveLength(1);
    expect(jobRows()[0]?.textContent).toContain("Scheduled job 13");
    expect(buttonIn(container, "Next")?.disabled).toBe(true);

    // Live shrink under the current page clamps stored state, and regrowth
    // stays on the clamped page instead of resurrecting page three.
    await render({ ...props(), calendarJobs: jobs.slice(0, 5) });
    expect(jobRows()).toHaveLength(5);
    expect(container.querySelector(".mc-pagination")).toBeNull();
    await render({ ...props(), calendarJobs: jobs });
    expect(paginationInfo()).toBe("1 / 3");
    expect(jobRows()[0]?.textContent).toContain("Scheduled job 1");
  });

  it("shows honest empty copy for jobs before and after load", async () => {
    await render(baseProps({ activeRoomId: "breakers" }));
    await click(subTab("Scheduler"));
    expect(container.textContent).toContain(
      "Scheduled jobs haven't loaded yet",
    );

    await render(
      baseProps({ activeRoomId: "breakers", jobsStatus: jobsStatusFixture() }),
    );
    expect(container.textContent).toContain("No scheduled jobs.");
  });
});

describe("FocusPage breakers pin", () => {
  it("offers the Breakers & Scheduler pin without any operational mutation", async () => {
    const onResolveFocusApproval = vi.fn(async () => {});
    const onRunCalendarJobNow = vi.fn(async () => {});
    const onReconnectFocusChannel = vi.fn(async () => {});
    const onToggleCalendarJob = vi.fn(async () => {});
    const onOpenStrategyTask = vi.fn(() => true);
    const onOpenRunbookForFocusItem = vi.fn(() => true);
    await render(
      baseProps({
        activeRoomId: "breakers",
        jobsStatus: jobsStatusFixture(),
        openBreakers: [breakerFixture()],
        calendarJobs: [calendarJobFixture({ job_id: "job-1" })],
        onResolveFocusApproval,
        onRunCalendarJobNow,
        onReconnectFocusChannel,
        onToggleCalendarJob,
        onOpenStrategyTask,
        onOpenRunbookForFocusItem,
      }),
    );

    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) =>
        button.getAttribute("aria-label") ===
        "Pin Breakers & Scheduler to Office",
    );
    expect(pin).toBeTruthy();
    await click(pin);
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas",
    );

    // Pinning is config-only: zero approval, breaker, scheduler, job,
    // channel, Strategy, or Runbook mutations, and the section stays put.
    expect(onResolveFocusApproval).not.toHaveBeenCalled();
    expect(onRunCalendarJobNow).not.toHaveBeenCalled();
    expect(onReconnectFocusChannel).not.toHaveBeenCalled();
    expect(onToggleCalendarJob).not.toHaveBeenCalled();
    expect(onOpenStrategyTask).not.toHaveBeenCalled();
    expect(onOpenRunbookForFocusItem).not.toHaveBeenCalled();
    expect(subTab("Breakers")?.getAttribute("aria-selected")).toBe("true");
  });
});
