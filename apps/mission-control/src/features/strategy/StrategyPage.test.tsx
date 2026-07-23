// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { buildAgentOrgModel } from "./strategyOrg";
import { StrategyPage } from "./StrategyPage";
import type { useStrategyController } from "./useStrategyController";

type StrategyController = ReturnType<typeof useStrategyController>;

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

function stubController(overrides?: Partial<Record<string, unknown>>) {
  const mutations = {
    createGoal: vi.fn(),
    updateGoal: vi.fn(),
    createProject: vi.fn(),
    updateProject: vi.fn(),
    createTask: vi.fn(),
    updateTask: vi.fn(),
    linkTaskBoardCard: vi.fn(),
    linkTaskJob: vi.fn(),
    clearTaskLinks: vi.fn(),
  };
  const controller = {
    enabled: true,
    availability: "ready",
    availabilityMessage: null,
    loading: false,
    isFilterTransitionPending: false,
    mutating: null,
    summary: null,
    goals: [],
    goalById: new Map(),
    selectedGoalId: "",
    setSelectedGoalId: vi.fn(),
    selectedGoal: null,
    projects: [],
    projectById: new Map(),
    projectsForSelectedGoal: [],
    selectedProjectId: "",
    setSelectedProjectId: vi.fn(),
    selectedProject: null,
    tasks: [],
    taskById: new Map(),
    taskByBoardCardId: new Map(),
    taskByJobId: new Map(),
    filteredTasks: [],
    selectedTaskId: "",
    setSelectedTaskId: vi.fn(),
    selectedTask: null,
    taskFilters: {
      query: "",
      status: "",
      owner_agent_id: "",
      blocked: false,
      stale: false,
      unassigned: false,
      hierarchy_root_agent_id: "",
      hierarchy_scope: "all",
      include_archived: false,
    },
    updateFilters: vi.fn(),
    resetFilters: vi.fn(),
    presets: [],
    org: buildAgentOrgModel([]),
    taskCountByProjectId: new Map(),
    suggestedOwnerAgentIds: [],
    approvalTaskByApprovalId: new Map(),
    describeTaskContext: vi.fn(() => null),
    queueRefresh: vi.fn(),
    loadStrategyData: vi.fn(),
    openTaskById: vi.fn(() => false),
    openBoardCardTask: vi.fn(() => false),
    openJobTask: vi.fn(() => false),
    applySummaryLens: vi.fn(),
    ...mutations,
    ...overrides,
  } as unknown as StrategyController;
  return { controller, mutations };
}

async function render(controller: StrategyController) {
  await act(async () => {
    root = createRoot(container);
    root.render(
      <StrategyPage
        controller={controller}
        agents={[]}
        runbookEnabled={false}
        selectedTaskRunbook={null}
        onOpenTaskRunbook={() => false}
      />,
    );
  });
}

describe("StrategyPage Trenches parity", () => {
  it("keeps all five Strategy sections reachable and offers Pin to Office without mutations", async () => {
    const { controller, mutations } = stubController();
    await render(controller);

    const sections = [
      "Overview",
      "Goals & Projects",
      "Tasks",
      "Task Detail",
      "Insights",
    ];
    for (const label of sections) {
      const tab = Array.from(
        container.querySelectorAll<HTMLButtonElement>('[role="tab"]'),
      ).find((candidate) => candidate.textContent === label);
      expect(tab, `missing section tab ${label}`).toBeTruthy();
      await act(async () => tab!.click());
      expect(tab!.getAttribute("aria-selected")).toBe("true");
      expect(
        container.querySelector('[role="tabpanel"]'),
        `section ${label} rendered no panel`,
      ).not.toBeNull();
    }

    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("aria-label") === "Pin Plan to Office",
    );
    expect(pin).toBeTruthy();
    await act(async () => pin!.click());
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas",
    );
    for (const mutation of Object.values(mutations)) {
      expect(mutation).not.toHaveBeenCalled();
    }
  });

  it("keeps the honest disabled state when the Strategy hub is off", async () => {
    const { controller } = stubController({
      enabled: false,
      availability: "disabled",
    });
    await render(controller);
    expect(container.textContent).toContain("Strategy hub is disabled");
    expect(
      Array.from(container.querySelectorAll("button")).find(
        (button) => button.getAttribute("aria-label") === "Pin Plan to Office",
      ),
    ).toBeUndefined();
  });
});
