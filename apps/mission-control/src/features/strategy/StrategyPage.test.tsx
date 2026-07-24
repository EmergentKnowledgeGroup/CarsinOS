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

async function clickButton(label: string) {
  const button = Array.from(container.querySelectorAll("button")).find(
    (candidate) => candidate.textContent?.includes(label),
  );
  expect(button, `missing button ${label}`).toBeTruthy();
  await act(async () => button!.click());
}

async function setLabeledInput(labelText: string, value: string) {
  const label = Array.from(container.querySelectorAll("label")).find(
    (candidate) => candidate.textContent?.trim().startsWith(labelText),
  );
  const input = label?.querySelector("input");
  expect(input, `missing input ${labelText}`).toBeTruthy();
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(input),
    "value",
  )?.set;
  setter?.call(input, value);
  await act(async () => {
    input!.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("StrategyPage Trenches parity", () => {
  it("renders distinct content for all five Strategy sections and pins without mutations", async () => {
    const { controller, mutations } = stubController();
    await render(controller);

    const sections = [
      ["Overview", "strategy-panel-overview", "Blocked Work"],
      ["Goals & Projects", "strategy-panel-plan", "Goals + Projects"],
      ["Tasks", "strategy-panel-tasks", "No tasks match"],
      ["Task Detail", "strategy-panel-detail", "Select a project"],
      ["Insights", "strategy-panel-insights", "Spend by Agent"],
    ];
    for (const [label, panelId, expectedContent] of sections) {
      const tab = Array.from(
        container.querySelectorAll<HTMLButtonElement>('[role="tab"]'),
      ).find((candidate) => candidate.textContent === label);
      expect(tab, `missing section tab ${label}`).toBeTruthy();
      await act(async () => tab!.click());
      expect(tab!.getAttribute("aria-selected")).toBe("true");
      const panel = container.querySelector(`#${panelId}`);
      expect(panel, `section ${label} rendered no exact panel`).not.toBeNull();
      expect(panel?.textContent).toContain(expectedContent);
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

  it("preserves the overview summary lens and goal creation mutation", async () => {
    const { controller, mutations } = stubController();
    await render(controller);

    await clickButton("Overview");
    await clickButton("Blocked Work");
    expect(controller.applySummaryLens).toHaveBeenCalledWith("blocked");
    expect(container.querySelector("#strategy-panel-tasks")).not.toBeNull();

    await clickButton("Goals & Projects");
    await clickButton("New Goal");
    await setLabeledInput("Slug", "owner-goal");
    await setLabeledInput("Title", "Owner Goal");
    await clickButton("Save Goal");
    expect(mutations.createGoal).toHaveBeenCalledWith(
      expect.objectContaining({ slug: "owner-goal", title: "Owner Goal" }),
    );
  });

  it("preserves project and task creation through their real forms", async () => {
    const goal = {
      goal_id: "goal-1",
      slug: "owner-goal",
      title: "Owner Goal",
      summary: "",
      status: "active",
      owner_agent_id: null,
      progress_pct: 0,
    };
    const project = {
      project_id: "project-1",
      goal_id: goal.goal_id,
      slug: "office",
      name: "Glass Office",
      summary: "",
      status: "active",
      owner_agent_id: null,
      budget_month_usd: null,
    };
    const { controller, mutations } = stubController({
      goals: [goal],
      goalById: new Map([[goal.goal_id, goal]]),
      selectedGoalId: goal.goal_id,
      selectedGoal: goal,
      projects: [project],
      projectById: new Map([[project.project_id, project]]),
      projectsForSelectedGoal: [project],
      selectedProjectId: project.project_id,
      selectedProject: project,
    });
    mutations.createTask.mockResolvedValue({
      task: { task_id: "task-new" },
    });
    await render(controller);

    await clickButton("Goals & Projects");
    await clickButton("New Project");
    await setLabeledInput("Slug", "review");
    await setLabeledInput("Name", "Review Throughput");
    await clickButton("Save Project");
    expect(mutations.createProject).toHaveBeenCalledWith(
      expect.objectContaining({
        goal_id: goal.goal_id,
        slug: "review",
        name: "Review Throughput",
      }),
    );

    await clickButton("Tasks");
    await clickButton("New Task");
    await setLabeledInput("Title", "Verify the room");
    await clickButton("Create Task");
    expect(mutations.createTask).toHaveBeenCalledWith(
      expect.objectContaining({
        project_id: project.project_id,
        title: "Verify the room",
      }),
    );
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
