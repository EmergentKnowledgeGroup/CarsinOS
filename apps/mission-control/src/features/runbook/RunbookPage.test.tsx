// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  Agent,
  RunbookDetailResponse,
  RunbookEntityRefResponse,
  RunbookSummaryItemResponse,
} from "../../types";
import { RunbookPage } from "./RunbookPage";
import type { useRunbookController } from "./useRunbookController";

type RunbookController = ReturnType<typeof useRunbookController>;

const NOW = Date.now();

const SESSION_ENTITY: RunbookEntityRefResponse = {
  entity_kind: "session",
  entity_id: "sess-001",
  display_label: "Recovery session",
  deep_link: {
    tab: "assistant",
    target_kind: "session",
    target_id: "sess-001",
    context: null,
  },
};

const APPROVAL_ENTITY: RunbookEntityRefResponse = {
  entity_kind: "approval",
  entity_id: "appr-001",
  display_label: "Shell approval",
  deep_link: {
    tab: "focus",
    target_kind: "approval",
    target_id: "appr-001",
    context: null,
  },
};

function summaryFixture(
  overrides?: Partial<RunbookSummaryItemResponse>,
): RunbookSummaryItemResponse {
  return {
    runbook_id: "assistant_session_run:run-001",
    runbook_kind: "assistant_session_run",
    anchor_kind: "run",
    anchor_id: "run-001",
    title: "Approval gate for incident recovery session",
    status: "waiting",
    status_reason: "Shell command approval is still pending.",
    owner_agent_id: "agent-root",
    owner_agent_label: "Root",
    primary_entity_label: "Recovery session",
    updated_at_ms: NOW - 60_000,
    current_step_label: "Await approval",
    warning_count: 1,
    linked_entities: [SESSION_ENTITY],
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

function detailFixture(): RunbookDetailResponse {
  return {
    runbook_id: "assistant_session_run:run-001",
    runbook_kind: "assistant_session_run",
    template_id: "assistant-session-v1",
    template_version: "1",
    anchor_kind: "run",
    anchor_id: "run-001",
    title: "Approval gate for incident recovery session",
    status: "waiting",
    status_reason: "Shell command approval is still pending.",
    generated_at_ms: NOW - 30_000,
    selected_execution_ref: {
      entity_kind: "run_record",
      entity_id: "run-001",
      created_at_ms: NOW - 600_000,
      started_at_ms: NOW - 500_000,
      waiting_since_ms: NOW - 120_000,
      finished_at_ms: null,
    },
    active_step_id: "await-approval",
    next_step_ids: ["execute-run"],
    linked_entities: [SESSION_ENTITY, APPROVAL_ENTITY],
    steps: [
      {
        step_id: "receive-request",
        label: "Receive operator request",
        kind: "intake",
        state: "completed",
        state_reason: null,
        started_at_ms: NOW - 600_000,
        finished_at_ms: NOW - 590_000,
        waiting_since_ms: null,
        linked_entity_refs: [],
        action_refs: [],
        template_index: 0,
      },
      {
        step_id: "await-approval",
        label: "Await approval",
        kind: "approval_gate",
        state: "waiting",
        state_reason: "Shell command approval is still pending.",
        started_at_ms: NOW - 500_000,
        finished_at_ms: null,
        waiting_since_ms: NOW - 120_000,
        linked_entity_refs: [APPROVAL_ENTITY],
        action_refs: ["review-approval"],
        template_index: 1,
      },
      {
        step_id: "execute-run",
        label: "Execute run",
        kind: "execution",
        state: "pending",
        state_reason: null,
        started_at_ms: null,
        finished_at_ms: null,
        waiting_since_ms: null,
        linked_entity_refs: [],
        action_refs: [],
        template_index: 2,
      },
    ],
    history: [
      {
        history_id: "hist-1",
        event_kind: "created",
        label: "Session created",
        detail: null,
        occurred_at_ms: NOW - 600_000,
        step_id: "receive-request",
        entity_refs: [],
      },
      {
        history_id: "hist-2",
        event_kind: "approval_requested",
        label: "Approval requested",
        detail: "Waiting on the operator to approve the shell command.",
        occurred_at_ms: NOW - 120_000,
        step_id: "await-approval",
        entity_refs: [APPROVAL_ENTITY],
      },
    ],
    actions: [
      {
        action_id: "open-session",
        action_kind: "open_session",
        label: "Open session",
        availability: "enabled",
        disabled_reason: null,
        target_entity_ref: SESSION_ENTITY,
      },
      {
        action_id: "review-approval",
        action_kind: "review_approval",
        label: "Review approval",
        availability: "enabled",
        disabled_reason: null,
        target_entity_ref: APPROVAL_ENTITY,
      },
    ],
    source_facts: [
      {
        fact_id: "fact-run",
        fact_kind: "run_record",
        entity_ref: SESSION_ENTITY,
        occurred_at_ms: NOW - 600_000,
        partial: false,
      },
      {
        fact_id: "fact-approval",
        fact_kind: "approval_record",
        entity_ref: null,
        occurred_at_ms: NOW - 120_000,
        partial: true,
      },
    ],
    availability: {
      is_limited: false,
      is_stale: false,
      last_refresh_at_ms: NOW,
      missing_source_kinds: [],
      stale_reason: null,
    },
    warnings: [
      {
        warning_id: "warn-approval",
        warning_kind: "approval_pending",
        message: "Operator approval is still pending for the next command.",
      },
    ],
    owner_agent_id: "agent-root",
    owner_agent_label: "Root",
  };
}

function stubController(overrides?: Partial<Record<string, unknown>>) {
  const controller = {
    enabled: true,
    availability: "ready",
    availabilityMessage: null,
    filters: { kind: "all", status: "all", owner_agent_id: "", query: "" },
    setFilters: vi.fn(),
    resetFilters: vi.fn(),
    items: [summaryFixture()],
    allItems: [summaryFixture()],
    countsByStatus: {
      pending: 1,
      active: 2,
      waiting: 3,
      blocked: 4,
      failed: 0,
      completed: 5,
      limited: 0,
    },
    generatedAtMs: NOW - 30_000,
    lastRefreshAtMs: NOW,
    isStale: false,
    selectedRunbookKind: "assistant_session_run",
    selectedAnchorId: "run-001",
    selectedRunbookId: "assistant_session_run:run-001",
    openRequestVersion: 0,
    detail: detailFixture(),
    detailError: null,
    detailLoading: false,
    selectRunbook: vi.fn(),
    queueRefresh: vi.fn(),
    loadRunbookData: vi.fn(),
    loadRunbookDetail: vi.fn(),
    openRunbook: vi.fn(),
    ...overrides,
  } as unknown as RunbookController;
  return controller;
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
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
});

const ROOT_AGENT: Agent = {
  agent_id: "agent-root",
  name: "Root",
  model_provider: "openai",
  model_id: "gpt-test",
};

async function render(controller: RunbookController, agents: Agent[] = []) {
  await act(async () => {
    root = createRoot(container);
    root.render(
      <RunbookPage
        controller={controller}
        agents={agents}
        onOpenDeepLink={vi.fn()}
      />,
    );
  });
}

async function renderWithDeepLink(controller: RunbookController) {
  const onOpenDeepLink = vi.fn();
  await act(async () => {
    root = createRoot(container);
    root.render(
      <RunbookPage
        controller={controller}
        agents={[]}
        onOpenDeepLink={onOpenDeepLink}
      />,
    );
  });
  return onOpenDeepLink;
}

async function clickButton(label: string) {
  const button = Array.from(container.querySelectorAll("button")).find(
    (candidate) => candidate.textContent?.includes(label),
  );
  expect(button, `missing button ${label}`).toBeTruthy();
  await act(async () => button!.click());
}

async function changeControl(label: string, value: string) {
  const fieldLabel = Array.from(container.querySelectorAll("label")).find(
    (candidate) => candidate.textContent?.startsWith(label),
  );
  const control = fieldLabel?.querySelector("input, select") as
    | HTMLInputElement
    | HTMLSelectElement
    | null;
  expect(control, `missing control ${label}`).toBeTruthy();
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(
      control instanceof HTMLSelectElement
        ? HTMLSelectElement.prototype
        : HTMLInputElement.prototype,
      "value",
    )?.set;
    setter?.call(control, value);
    control!.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

function pinButton(): HTMLButtonElement | undefined {
  return Array.from(container.querySelectorAll("button")).find(
    (button) =>
      button.getAttribute("aria-label") === "Pin History & Receipts to Office",
  );
}

describe("RunbookPage Trenches parity", () => {
  it("keeps every honest state and exposes no dead pin on any of them", async () => {
    const states: Array<[Partial<Record<string, unknown>>, string]> = [
      [
        { enabled: false, availability: "disabled" },
        "Runbook hub is disabled",
      ],
      [
        {
          availability: "unsupported",
          availabilityMessage:
            "The connected gateway does not expose the Runbook surface yet.",
        },
        "Runbook surface unavailable",
      ],
      [
        { availability: "error", availabilityMessage: "boom" },
        "Runbook failed to load",
      ],
      [{ availability: "loading", items: [], detail: null }, "Loading Runbook"],
    ];
    for (const [overrides, title] of states) {
      await render(stubController(overrides));
      expect(container.textContent).toContain(title);
      expect(pinButton()).toBeUndefined();
      await act(async () => root?.unmount());
      root = null;
      container.replaceChildren();
    }
  });

  it("keeps browse parity: lenses, filters, refresh, and a pin without mutations", async () => {
    const controller = stubController();
    await render(controller, [ROOT_AGENT]);

    // Status summary lenses stay live filters.
    expect(container.textContent).toContain("Blocked");
    await clickButton("Waiting");
    expect(controller.setFilters).toHaveBeenCalledWith({ status: "waiting" });

    // Browse filters and actions stay wired to the controller.
    await changeControl("Query", "incident");
    expect(controller.setFilters).toHaveBeenCalledWith({ query: "incident" });
    await changeControl("Kind", "scheduled_job_run");
    expect(controller.setFilters).toHaveBeenCalledWith({
      kind: "scheduled_job_run",
    });
    await changeControl("Status", "blocked");
    expect(controller.setFilters).toHaveBeenCalledWith({ status: "blocked" });
    await changeControl("Owner", "agent-root");
    expect(controller.setFilters).toHaveBeenCalledWith({
      owner_agent_id: "agent-root",
    });
    await clickButton("Reset filters");
    expect(controller.resetFilters).toHaveBeenCalled();
    await clickButton("Refresh");
    expect(controller.queueRefresh).toHaveBeenCalled();

    // The list keeps its real item content.
    expect(container.textContent).toContain(
      "Approval gate for incident recovery session",
    );
    expect(container.textContent).toContain(
      "Shell command approval is still pending.",
    );
    expect(container.textContent).toContain("Await approval");

    // The registry-backed pin adds the shortcut without touching Runbook data.
    const pin = pinButton();
    expect(pin).toBeTruthy();
    const mutationCallsBefore = [
      controller.selectRunbook,
      controller.setFilters,
      controller.resetFilters,
      controller.queueRefresh,
    ].map((fn) => (fn as ReturnType<typeof vi.fn>).mock.calls.length);
    await act(async () => pin!.click());
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas",
    );
    expect(
      [
        controller.selectRunbook,
        controller.setFilters,
        controller.resetFilters,
        controller.queueRefresh,
      ].map((fn) => (fn as ReturnType<typeof vi.fn>).mock.calls.length),
    ).toEqual(mutationCallsBefore);
  });

  it("paginates the browse list past six runbooks", async () => {
    const items = Array.from({ length: 7 }, (_, index) =>
      summaryFixture({
        runbook_id: `assistant_session_run:run-00${index + 1}`,
        anchor_id: `run-00${index + 1}`,
        title: `Runbook number ${index + 1}`,
      }),
    );
    await render(stubController({ items }));
    expect(container.textContent).toContain("Runbook number 1");
    expect(container.textContent).not.toContain("Runbook number 7");
    expect(container.textContent).toContain("1 / 2");
    const previous = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("Previous"),
    );
    expect(previous?.disabled).toBe(true);
    await clickButton("Next");
    expect(container.textContent).toContain("Runbook number 7");
    const next = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("Next"),
    );
    expect(next?.disabled).toBe(true);
    await clickButton("Previous");
    expect(container.textContent).toContain("Runbook number 1");
  });

  it("enters a real detail and proves Overview, Flow, Artifacts, and History", async () => {
    const controller = stubController();
    const onOpenDeepLink = await renderWithDeepLink(controller);

    await clickButton("Approval gate for incident recovery session");
    expect(controller.selectRunbook).toHaveBeenCalledWith(
      "assistant_session_run",
      "run-001",
    );

    // Detail hero, authoritative facts, and the warning strip.
    expect(container.textContent).toContain("Back to list");
    expect(container.textContent).toContain("Assistant runs");
    expect(container.textContent).toContain("await-approval");
    expect(container.textContent).toContain("execute-run");
    expect(container.textContent).toContain("run_record");
    expect(container.textContent).toContain(
      "Operator approval is still pending for the next command.",
    );

    // Overview: actions, linked artifacts, and source facts with exact deep links.
    expect(container.textContent).toContain("Actions");
    expect(container.textContent).toContain("Linked artifacts");
    expect(container.textContent).toContain("2 fact(s)");
    expect(container.textContent).toContain("approval_record");
    expect(container.textContent).toContain("partial data");
    await clickButton("Review approval");
    expect(onOpenDeepLink).toHaveBeenCalledWith(APPROVAL_ENTITY.deep_link);
    await clickButton("Recovery session");
    expect(onOpenDeepLink).toHaveBeenCalledWith(SESSION_ENTITY.deep_link);

    // Flow: every step with its state.
    await clickButton("Flow (3)");
    expect(container.textContent).toContain("Receive operator request");
    expect(container.textContent).toContain("Await approval");
    expect(container.textContent).toContain("Execute run");
    expect(container.textContent).toContain("approval gate");

    // Artifacts: linked entities plus the previewed source facts.
    await clickButton("Artifacts");
    expect(container.textContent).toContain("2 item(s)");
    expect(container.textContent).toContain("Shell approval");
    expect(container.textContent).toContain("run_record");

    // History: newest first with detail copy.
    await clickButton("History (2)");
    expect(container.textContent).toContain("Approval requested");
    expect(container.textContent).toContain(
      "Waiting on the operator to approve the shell command.",
    );
    expect(container.textContent).toContain("Session created");

    // Back to list restores browse without losing the surface.
    await clickButton("Back to list");
    expect(container.textContent).toContain("Browse Runbooks");
  });

  it("keeps the honest in-detail loading and error copy", async () => {
    const controller = stubController({ detailLoading: true, detail: null });
    await render(controller);
    await clickButton("Approval gate for incident recovery session");
    expect(container.textContent).toContain("Loading selected runbook…");

    await act(async () => root?.unmount());
    root = null;
    container.replaceChildren();

    const errored = stubController({
      detail: null,
      detailError: "runbook not found",
    });
    await render(errored);
    await clickButton("Approval gate for incident recovery session");
    expect(container.textContent).toContain("runbook not found");
  });

  it("paginates long Flow and History tabs without losing their boundary items", async () => {
    const detail = detailFixture();
    detail.steps = [
      ...detail.steps,
      {
        ...detail.steps[2],
        step_id: "archive-receipt",
        label: "Archive receipt",
        template_index: 3,
      },
      {
        ...detail.steps[2],
        step_id: "notify-operator",
        label: "Notify operator",
        template_index: 4,
      },
    ];
    detail.history = [
      ...detail.history,
      ...Array.from({ length: 4 }, (_, index) => ({
        ...detail.history[0],
        history_id: `hist-extra-${index + 1}`,
        label: `History event ${index + 1}`,
        occurred_at_ms: NOW - (index + 3) * 60_000,
      })),
    ];
    await render(stubController({ detail }));
    await clickButton("Approval gate for incident recovery session");

    await clickButton("Flow (5)");
    expect(container.querySelector(".mc-runbook-flow")?.textContent).not.toContain(
      "Notify operator",
    );
    await clickButton("Next");
    expect(container.querySelector(".mc-runbook-flow")?.textContent).toContain(
      "Notify operator",
    );
    await clickButton("Previous");
    expect(container.querySelector(".mc-runbook-flow")?.textContent).toContain(
      "Receive operator request",
    );

    await clickButton("History (6)");
    expect(container.querySelector(".mc-runbook-history")?.textContent).not.toContain(
      "Session created",
    );
    await clickButton("Next");
    expect(container.querySelector(".mc-runbook-history")?.textContent).toContain(
      "Session created",
    );
    await clickButton("Previous");
    expect(container.querySelector(".mc-runbook-history")?.textContent).toContain(
      "History event 4",
    );
  });
});
