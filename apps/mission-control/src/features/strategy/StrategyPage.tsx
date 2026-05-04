import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { Compass, Link2, Milestone, ShieldAlert, TimerReset } from "lucide-react";
import { Modal } from "../../ui/Modal";
import { Surface } from "../../ui/Surface";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import type {
  Agent,
  GoalResponse,
  ProjectResponse,
  RunbookSummaryItemResponse,
  TaskResponse,
} from "../../types";
import { formatRelative, fromInputDateTimeValue, toInputDateTimeValue } from "../../utils/datetime";
import { RunbookLinkPanel } from "../runbook/RunbookLinkPanel";
import {
  isGoalDraftDirty,
  isProjectDraftDirty,
  isTaskDraftDirty,
} from "./strategyDraftState";
import type { useStrategyController } from "./useStrategyController";

interface StrategyPageProps {
  controller: ReturnType<typeof useStrategyController>;
  agents: Agent[];
  runbookEnabled: boolean;
  selectedTaskRunbook: RunbookSummaryItemResponse | null;
  onOpenTaskRunbook: (taskId: string) => boolean;
}

interface GoalFormState {
  slug: string;
  title: string;
  summary: string;
  status: string;
  owner_agent_id: string;
  target_date: string;
}

interface ProjectFormState {
  goal_id: string;
  slug: string;
  name: string;
  summary: string;
  status: string;
  owner_agent_id: string;
  workspace_root: string;
  budget_month_usd: string;
}

interface TaskFormState {
  task_id: string;
  project_id: string;
  parent_task_id: string;
  title: string;
  detail: string;
  status: string;
  priority: string;
  owner_agent_id: string;
  due_at: string;
  blocked_reason: string;
  linked_board_card_id: string;
  linked_job_id: string;
}

const GOAL_STATUS_OPTIONS = ["active", "at_risk", "completed", "archived"];
const PROJECT_STATUS_OPTIONS = ["active", "blocked", "completed", "archived"];
const TASK_STATUS_OPTIONS = ["todo", "in_progress", "blocked", "done", "archived"];
const TASK_PRIORITY_OPTIONS = ["low", "normal", "high", "critical"];

const EMPTY_GOAL_FORM: GoalFormState = {
  slug: "",
  title: "",
  summary: "",
  status: "active",
  owner_agent_id: "",
  target_date: "",
};

const EMPTY_PROJECT_FORM: ProjectFormState = {
  goal_id: "",
  slug: "",
  name: "",
  summary: "",
  status: "active",
  owner_agent_id: "",
  workspace_root: ".",
  budget_month_usd: "",
};

const EMPTY_TASK_FORM: TaskFormState = {
  task_id: "",
  project_id: "",
  parent_task_id: "",
  title: "",
  detail: "",
  status: "todo",
  priority: "normal",
  owner_agent_id: "",
  due_at: "",
  blocked_reason: "",
  linked_board_card_id: "",
  linked_job_id: "",
};

const MONEY_FORMATTER = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 2,
});

function formatMoney(value: number): string {
  return MONEY_FORMATTER.format(value);
}

function toneForStatus(status: string): "up" | "down" | "warning" | "checking" | "" {
  if (status === "done" || status === "completed") {
    return "up";
  }
  if (status === "blocked" || status === "archived") {
    return "down";
  }
  if (status === "at_risk" || status === "critical" || status === "high") {
    return "warning";
  }
  if (status === "in_progress" || status === "active") {
    return "checking";
  }
  return "";
}

function hydrateGoalForm(goal: GoalResponse | null): GoalFormState {
  if (!goal) {
    return EMPTY_GOAL_FORM;
  }
  return {
    slug: goal.slug,
    title: goal.title,
    summary: goal.summary,
    status: goal.status,
    owner_agent_id: goal.owner_agent_id ?? "",
    target_date: toInputDateTimeValue(goal.target_date),
  };
}

function hydrateProjectForm(project: ProjectResponse | null, fallbackGoalId: string): ProjectFormState {
  if (!project) {
    return {
      ...EMPTY_PROJECT_FORM,
      goal_id: fallbackGoalId,
    };
  }
  return {
    goal_id: project.goal_id,
    slug: project.slug,
    name: project.name,
    summary: project.summary,
    status: project.status,
    owner_agent_id: project.owner_agent_id ?? "",
    workspace_root: project.workspace_root ?? ".",
    budget_month_usd:
      typeof project.budget_month_usd === "number" ? String(project.budget_month_usd) : "",
  };
}

function hydrateTaskForm(task: TaskResponse | null, fallbackProjectId: string): TaskFormState {
  if (!task) {
    return {
      ...EMPTY_TASK_FORM,
      project_id: fallbackProjectId,
    };
  }
  return {
    task_id: task.task_id,
    project_id: task.project_id,
    parent_task_id: task.parent_task_id ?? "",
    title: task.title,
    detail: task.detail,
    status: task.status,
    priority: task.priority,
    owner_agent_id: task.owner_agent_id ?? "",
    due_at: toInputDateTimeValue(task.due_at),
    blocked_reason: task.blocked_reason ?? "",
    linked_board_card_id: task.linked_board_card_id ?? "",
    linked_job_id: task.linked_job_id ?? "",
  };
}

function StrategyStatePanel({
  title,
  detail,
}: {
  title: string;
  detail: string;
}) {
  return (
    <section className="mc-strategy-page">
      <Surface
        className="mc-strategy-state"
        title={title}
        subtitle={detail}
      >
        <EmptyState message={detail} />
      </Surface>
    </section>
  );
}

function SummaryCard({
  icon,
  label,
  value,
  detail,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  detail: string;
  onClick?: () => void;
}) {
  const Tag = onClick ? "button" : "div";
  return (
    <Tag
      type={onClick ? "button" : undefined}
      className={`mc-strategy-summary-card${onClick ? " mc-strategy-summary-card-action" : ""}`}
      onClick={onClick}
    >
      <div className="mc-strategy-summary-kicker">
        {icon}
        <span>{label}</span>
      </div>
      <strong>{value}</strong>
      <p>{detail}</p>
    </Tag>
  );
}

export function StrategyPage({
  controller,
  agents,
  runbookEnabled,
  selectedTaskRunbook,
  onOpenTaskRunbook,
}: StrategyPageProps) {
  const [goalModalMode, setGoalModalMode] = useState<"create" | "edit" | null>(null);
  const [goalForm, setGoalForm] = useState<GoalFormState>(EMPTY_GOAL_FORM);
  const [goalError, setGoalError] = useState<string | null>(null);
  const [projectModalMode, setProjectModalMode] = useState<"create" | "edit" | null>(null);
  const [projectForm, setProjectForm] = useState<ProjectFormState>(EMPTY_PROJECT_FORM);
  const [projectError, setProjectError] = useState<string | null>(null);
  const [taskMode, setTaskMode] = useState<"create" | "edit">("edit");
  const [taskForm, setTaskForm] = useState<TaskFormState>(EMPTY_TASK_FORM);
  const [taskError, setTaskError] = useState<string | null>(null);
  const [activeSection, setActiveSection] = useState<
    "overview" | "plan" | "tasks" | "detail" | "insights"
  >("tasks");
  const [detailSection, setDetailSection] = useState<"basics" | "links">(
    "basics"
  );
  const [forceBoardReassign, setForceBoardReassign] = useState(false);
  const [forceJobReassign, setForceJobReassign] = useState(false);
  const selectedTaskId =
    taskMode === "edit" ? controller.selectedTask?.task_id ?? null : null;
  const goalBaseline =
    goalModalMode === "edit" ? hydrateGoalForm(controller.selectedGoal) : EMPTY_GOAL_FORM;
  const projectBaseline =
    projectModalMode === "edit"
      ? hydrateProjectForm(controller.selectedProject, controller.selectedGoalId)
      : hydrateProjectForm(null, controller.selectedGoalId);
  const taskBaseline =
    taskMode === "create"
      ? hydrateTaskForm(null, controller.selectedProjectId)
      : hydrateTaskForm(
          taskForm.task_id
            ? controller.taskById.get(taskForm.task_id) ?? null
            : controller.selectedTask,
          taskForm.project_id || controller.selectedProjectId
        );
  const goalDirty =
    goalModalMode !== null && isGoalDraftDirty(goalForm, goalBaseline);
  const projectDirty =
    projectModalMode !== null && isProjectDraftDirty(projectForm, projectBaseline);
  const taskDirty =
    taskMode === "create"
      ? isTaskDraftDirty(taskForm, taskBaseline)
      : taskForm.task_id !== "" && isTaskDraftDirty(taskForm, taskBaseline);
  const hasUnsavedStrategyDraft = goalDirty || projectDirty || taskDirty;
  const activeTaskForm =
    taskMode === "edit" &&
    controller.selectedTask &&
    taskForm.task_id !== controller.selectedTask.task_id &&
    !taskDirty
      ? hydrateTaskForm(controller.selectedTask, controller.selectedProjectId)
      : taskForm;

  useEffect(() => {
    if (!hasUnsavedStrategyDraft) {
      return;
    }
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, [hasUnsavedStrategyDraft]);

  const confirmDiscard = useCallback(
    (shouldGuard: boolean, message = "Discard unsaved Strategy changes?") => {
      if (!shouldGuard) {
        return true;
      }
      return window.confirm(message);
    },
    []
  );

  const discardTaskDraft = useCallback(() => {
    setTaskError(null);
    setTaskMode("edit");
    setTaskForm(hydrateTaskForm(controller.selectedTask, controller.selectedProjectId));
  }, [controller.selectedProjectId, controller.selectedTask]);

  const runWithTaskDraftGuard = useCallback(
    (action: () => void, message = "Discard the current Strategy task draft?") => {
      if (!confirmDiscard(taskDirty, message)) {
        return;
      }
      if (taskDirty) {
        discardTaskDraft();
      }
      action();
    },
    [confirmDiscard, discardTaskDraft, taskDirty]
  );

  const runWithAnyDraftGuard = useCallback(
    (action: () => void, message = "Discard unsaved Strategy changes?") => {
      if (!confirmDiscard(hasUnsavedStrategyDraft, message)) {
        return;
      }
      if (goalDirty) {
        setGoalError(null);
        setGoalForm(goalBaseline);
      }
      if (projectDirty) {
        setProjectError(null);
        setProjectForm(projectBaseline);
      }
      if (taskDirty) {
        discardTaskDraft();
      }
      action();
    },
    [
      confirmDiscard,
      discardTaskDraft,
      goalBaseline,
      goalDirty,
      hasUnsavedStrategyDraft,
      projectBaseline,
      projectDirty,
      taskDirty,
    ]
  );

  const updateTaskForm = (
    updater: (current: TaskFormState) => TaskFormState
  ) => {
    setTaskForm((current) => {
      const base =
        taskMode === "edit" &&
        controller.selectedTask &&
        current.task_id !== controller.selectedTask.task_id
          ? hydrateTaskForm(controller.selectedTask, controller.selectedProjectId)
          : current;
      return updater(base);
    });
  };

  const selectedProjectTasks = useMemo(
    () =>
      controller.tasks
        .filter((task) => task.project_id === activeTaskForm.project_id)
        .filter((task) => task.task_id !== activeTaskForm.task_id),
    [activeTaskForm.project_id, activeTaskForm.task_id, controller.tasks]
  );

  const topAgentSpend = controller.summary?.spend_by_agent[0] ?? null;
  const topProjectSpend = controller.summary?.spend_by_project[0] ?? null;
  const lowestProgressGoal = useMemo(() => {
    const items = controller.summary?.goal_progress ?? [];
    return [...items].sort((left, right) => left.progress_pct - right.progress_pct)[0] ?? null;
  }, [controller.summary?.goal_progress]);
  const totalGoalProgress = useMemo(() => {
    const items = controller.summary?.goal_progress ?? [];
    if (items.length === 0) {
      return 0;
    }
    return Math.round(
      items.reduce((sum, item) => sum + item.progress_pct, 0) / items.length
    );
  }, [controller.summary?.goal_progress]);

  if (!controller.enabled) {
    return (
      <StrategyStatePanel
        title="Strategy hub is disabled"
        detail="Enable Strategy hub in Config > Reliability + Rollout to expose goals, projects, tasks, and presets."
      />
    );
  }

  if (controller.availability === "unsupported") {
    return (
      <StrategyStatePanel
        title="Strategy surface unavailable"
        detail={
          controller.availabilityMessage ??
          "The connected gateway does not expose the Strategy management contracts yet."
        }
      />
    );
  }

  if (controller.availability === "error") {
    return (
      <StrategyStatePanel
        title="Strategy failed to load"
        detail={controller.availabilityMessage ?? "Strategy could not load."}
      />
    );
  }

  if (controller.loading && controller.goals.length === 0) {
    return (
      <StrategyStatePanel
        title="Loading Strategy"
        detail="Pulling goals, projects, tasks, and summary projections from the gateway."
      />
    );
  }

  const saveGoal = async () => {
    setGoalError(null);
    if (!goalForm.slug.trim() || !goalForm.title.trim()) {
      setGoalError("Goal slug and title are required.");
      return;
    }
    try {
      if (goalModalMode === "create") {
        await controller.createGoal({
          slug: goalForm.slug.trim(),
          title: goalForm.title.trim(),
          summary: goalForm.summary.trim() || null,
          status: goalForm.status,
          owner_agent_id: goalForm.owner_agent_id || null,
          target_date: fromInputDateTimeValue(goalForm.target_date),
        });
      } else if (controller.selectedGoal) {
        await controller.updateGoal(controller.selectedGoal.goal_id, {
          slug: goalForm.slug.trim(),
          title: goalForm.title.trim(),
          summary: goalForm.summary.trim(),
          status: goalForm.status,
          owner_agent_id: goalForm.owner_agent_id || null,
          target_date: fromInputDateTimeValue(goalForm.target_date),
        });
      }
      setGoalModalMode(null);
    } catch (error) {
      setGoalError(String(error));
    }
  };

  const saveProject = async () => {
    setProjectError(null);
    if (!projectForm.goal_id || !projectForm.slug.trim() || !projectForm.name.trim()) {
      setProjectError("Goal, project slug, and project name are required.");
      return;
    }
    try {
      if (projectModalMode === "create") {
        await controller.createProject({
          goal_id: projectForm.goal_id,
          slug: projectForm.slug.trim(),
          name: projectForm.name.trim(),
          summary: projectForm.summary.trim() || null,
          status: projectForm.status,
          owner_agent_id: projectForm.owner_agent_id || null,
          workspace_root: projectForm.workspace_root.trim() || null,
          budget_month_usd: projectForm.budget_month_usd.trim()
            ? Number(projectForm.budget_month_usd)
            : null,
        });
      } else if (controller.selectedProject) {
        await controller.updateProject(controller.selectedProject.project_id, {
          goal_id: projectForm.goal_id,
          slug: projectForm.slug.trim(),
          name: projectForm.name.trim(),
          summary: projectForm.summary.trim(),
          status: projectForm.status,
          owner_agent_id: projectForm.owner_agent_id || null,
          workspace_root: projectForm.workspace_root.trim() || null,
          budget_month_usd: projectForm.budget_month_usd.trim()
            ? Number(projectForm.budget_month_usd)
            : null,
        });
      }
      setProjectModalMode(null);
    } catch (error) {
      setProjectError(String(error));
    }
  };

  const saveTask = async () => {
    const workingTaskForm = activeTaskForm;
    setTaskError(null);
    if (!workingTaskForm.project_id || !workingTaskForm.title.trim()) {
      setTaskError("Project and task title are required.");
      return;
    }
    if (
      workingTaskForm.status === "blocked" &&
      !workingTaskForm.blocked_reason.trim()
    ) {
      setTaskError("Blocked tasks require a blocked reason.");
      return;
    }
    try {
      const taskPayload = {
        project_id: workingTaskForm.project_id,
        parent_task_id: workingTaskForm.parent_task_id.trim() || null,
        title: workingTaskForm.title.trim(),
        detail: workingTaskForm.detail.trim(),
        status: workingTaskForm.status,
        priority: workingTaskForm.priority,
        owner_agent_id: workingTaskForm.owner_agent_id || null,
        due_at: fromInputDateTimeValue(workingTaskForm.due_at),
        blocked_reason:
          workingTaskForm.status === "blocked"
            ? workingTaskForm.blocked_reason.trim()
            : null,
      };
      const response =
        taskMode === "create"
          ? await controller.createTask(taskPayload)
          : await controller.updateTask(workingTaskForm.task_id, taskPayload);

      const savedTask = response.task;
      if (workingTaskForm.linked_board_card_id.trim()) {
        await controller.linkTaskBoardCard(savedTask.task_id, {
          board_card_id: workingTaskForm.linked_board_card_id.trim(),
          force_reassign: forceBoardReassign,
        });
      } else if (
        taskMode === "edit" &&
        controller.selectedTask?.linked_board_card_id
      ) {
        await controller.clearTaskLinks(savedTask.task_id, {
          clear_board_card: true,
        });
      }

      if (workingTaskForm.linked_job_id.trim()) {
        await controller.linkTaskJob(savedTask.task_id, {
          job_id: workingTaskForm.linked_job_id.trim(),
          force_reassign: forceJobReassign,
        });
      } else if (taskMode === "edit" && controller.selectedTask?.linked_job_id) {
        await controller.clearTaskLinks(savedTask.task_id, {
          clear_job: true,
        });
      }
      setTaskMode("edit");
    } catch (error) {
      setTaskError(String(error));
    }
  };

  return (
    <section className="mc-strategy-page" data-testid="strategy-page">
      <div className="mc-page-section-tabs" aria-label="Strategy sections" role="tablist">
        <button
          type="button"
          id="strategy-tab-overview"
          role="tab"
          aria-selected={activeSection === "overview"}
          aria-controls="strategy-panel-overview"
          className={`mc-page-section-btn${activeSection === "overview" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActiveSection("overview")}
        >
          Overview
        </button>
        <button
          type="button"
          id="strategy-tab-plan"
          role="tab"
          aria-selected={activeSection === "plan"}
          aria-controls="strategy-panel-plan"
          className={`mc-page-section-btn${activeSection === "plan" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActiveSection("plan")}
        >
          Goals & Projects
        </button>
        <button
          type="button"
          id="strategy-tab-tasks"
          role="tab"
          aria-selected={activeSection === "tasks"}
          aria-controls="strategy-panel-tasks"
          className={`mc-page-section-btn${activeSection === "tasks" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActiveSection("tasks")}
        >
          Tasks
        </button>
        <button
          type="button"
          id="strategy-tab-detail"
          role="tab"
          aria-selected={activeSection === "detail"}
          aria-controls="strategy-panel-detail"
          className={`mc-page-section-btn${activeSection === "detail" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActiveSection("detail")}
        >
          Task Detail
        </button>
        <button
          type="button"
          id="strategy-tab-insights"
          role="tab"
          aria-selected={activeSection === "insights"}
          aria-controls="strategy-panel-insights"
          className={`mc-page-section-btn${activeSection === "insights" ? " mc-page-section-btn-active" : ""}`}
          onClick={() => setActiveSection("insights")}
        >
          Insights
        </button>
      </div>

      {activeSection === "overview" ? (
        <div
          className="mc-strategy-summary-strip"
          id="strategy-panel-overview"
          role="tabpanel"
          aria-labelledby="strategy-tab-overview"
        >
          <SummaryCard
            icon={<ShieldAlert size={14} />}
            label="Blocked Work"
            value={String(controller.summary?.blocked_task_count ?? 0)}
            detail={
              controller.summary?.blocked_tasks[0]?.title ??
              "No blocked tasks in the active management set."
            }
            onClick={() => {
              setActiveSection("tasks");
              runWithTaskDraftGuard(() => controller.applySummaryLens("blocked"));
            }}
          />
          <SummaryCard
            icon={<TimerReset size={14} />}
            label="Stale Work"
            value={String(controller.summary?.stale_task_count ?? 0)}
            detail={
              controller.summary?.stale_tasks[0]?.title ??
              "Nothing has crossed the stale threshold."
            }
            onClick={() => {
              setActiveSection("tasks");
              runWithTaskDraftGuard(() => controller.applySummaryLens("stale"));
            }}
          />
          <SummaryCard
            icon={<Compass size={14} />}
            label="Top Agent Spend"
            value={topAgentSpend ? formatMoney(topAgentSpend.estimated_cost_total) : formatMoney(0)}
            detail={
              topAgentSpend
                ? `${topAgentSpend.agent_name} · ${topAgentSpend.linked_task_count} linked tasks`
                : "No spend linked to managed work."
            }
            onClick={
              topAgentSpend
                ? () => {
                    setActiveSection("tasks");
                    runWithTaskDraftGuard(() =>
                      controller.updateFilters({
                        owner_agent_id: topAgentSpend.agent_id,
                        blocked: false,
                        stale: false,
                      })
                    );
                  }
                : undefined
            }
          />
          <SummaryCard
            icon={<Milestone size={14} />}
            label="Top Project Spend"
            value={
              topProjectSpend
                ? formatMoney(topProjectSpend.estimated_cost_total)
                : formatMoney(0)
            }
            detail={
              topProjectSpend
                ? `${topProjectSpend.project_name} · ${topProjectSpend.attributed_run_count} runs`
                : "No project spend attributed yet."
            }
            onClick={
              topProjectSpend
                ? () => {
                    setActiveSection("plan");
                    runWithTaskDraftGuard(() => {
                      controller.setSelectedGoalId(topProjectSpend.goal_id);
                      controller.setSelectedProjectId(topProjectSpend.project_id);
                    });
                  }
                : undefined
            }
          />
          <SummaryCard
            icon={<Milestone size={14} />}
            label="Goal Progress"
            value={`${totalGoalProgress}%`}
            detail={
              lowestProgressGoal
                ? `${lowestProgressGoal.title} is lowest at ${lowestProgressGoal.progress_pct}%`
                : "No goal progress tracked yet."
            }
            onClick={
              lowestProgressGoal
                ? () => {
                    setActiveSection("plan");
                    runWithTaskDraftGuard(() =>
                      controller.setSelectedGoalId(lowestProgressGoal.goal_id)
                    );
                  }
                : undefined
            }
          />
          <SummaryCard
            icon={<Link2 size={14} />}
            label="Approval Backlog"
            value={String(controller.summary?.critical_approval_backlog_count ?? 0)}
            detail={
              controller.summary?.critical_approval_backlog[0]?.summary ??
              `Unattributed spend: ${formatMoney(
                controller.summary?.unattributed_spend_total ?? 0
              )}`
            }
            onClick={() => {
              setActiveSection("tasks");
              const firstLinkedTask = controller.summary?.critical_approval_backlog.find(
                (item) => item.linked_task_id
              );
              const linkedTaskId = firstLinkedTask?.linked_task_id;
              if (linkedTaskId) {
                runWithTaskDraftGuard(() => {
                  controller.openTaskById(linkedTaskId);
                });
              }
            }}
          />
        </div>
      ) : null}

      {activeSection === "plan" ? (
        <div
          className="mc-page-section-shell"
          id="strategy-panel-plan"
          role="tabpanel"
          aria-labelledby="strategy-tab-plan"
        >
        <Surface
          className="mc-strategy-nav"
          title="Goals + Projects"
          subtitle="Management is top-down here. Boards stay execution-first."
          headerRight={
            <div className="mc-strategy-inline-actions">
              <button
                type="button"
                className="ghost"
                onClick={() => {
                  runWithAnyDraftGuard(() => {
                    setGoalError(null);
                    setGoalForm(EMPTY_GOAL_FORM);
                    setGoalModalMode("create");
                  });
                }}
              >
                New Goal
              </button>
              <button
                type="button"
                className="ghost"
                disabled={!controller.selectedGoalId}
                onClick={() => {
                  runWithAnyDraftGuard(() => {
                    setProjectError(null);
                    setProjectForm(
                      hydrateProjectForm(null, controller.selectedGoalId)
                    );
                    setProjectModalMode("create");
                  });
                }}
              >
                New Project
              </button>
            </div>
          }
        >
          <div className="mc-strategy-goal-list">
            {controller.goals.map((goal) => (
              <button
                key={goal.goal_id}
                type="button"
                className={`mc-strategy-goal-card${
                  goal.goal_id === controller.selectedGoalId ? " is-active" : ""
                }`}
                onClick={() =>
                  runWithAnyDraftGuard(() => controller.setSelectedGoalId(goal.goal_id))
                }
              >
                <div className="mc-strategy-goal-head">
                  <strong>{goal.title}</strong>
                  <Chip label={`${goal.progress_pct}%`} tone="checking" />
                </div>
                <p>{goal.summary || "No goal summary yet."}</p>
                <div className="mc-strategy-goal-meta">
                  <Chip label={goal.status} tone={toneForStatus(goal.status)} />
                  {goal.owner_agent_id ? (
                    <span>
                      {controller.org.agentsById.get(goal.owner_agent_id)?.name ??
                        goal.owner_agent_id}
                    </span>
                  ) : (
                    <span>Unowned</span>
                  )}
                </div>
              </button>
            ))}
            {controller.goals.length === 0 ? (
              <EmptyState message="No goals yet. Create the first goal to expose the management graph." />
            ) : null}
          </div>

          {controller.selectedGoal ? (
            <div className="mc-strategy-project-stack">
              <div className="mc-strategy-subheader">
                <div>
                  <h3>{controller.selectedGoal.title}</h3>
                  <p>{controller.projectsForSelectedGoal.length} projects under this goal</p>
                </div>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    runWithAnyDraftGuard(() => {
                      setGoalError(null);
                      setGoalForm(hydrateGoalForm(controller.selectedGoal));
                      setGoalModalMode("edit");
                    });
                  }}
                >
                  Edit Goal
                </button>
              </div>
              {controller.projectsForSelectedGoal.map((project) => (
                <button
                  key={project.project_id}
                  type="button"
                  className={`mc-strategy-project-card${
                    project.project_id === controller.selectedProjectId ? " is-active" : ""
                  }`}
                  onClick={() =>
                    runWithAnyDraftGuard(() =>
                      controller.setSelectedProjectId(project.project_id)
                    )
                  }
                >
                  <div className="mc-strategy-project-head">
                    <strong>{project.name}</strong>
                    <Chip
                      label={`${controller.taskCountByProjectId.get(project.project_id) ?? 0} tasks`}
                      tone=""
                    />
                  </div>
                  <p>{project.summary || "No project summary yet."}</p>
                  <div className="mc-strategy-goal-meta">
                    <Chip label={project.status} tone={toneForStatus(project.status)} />
                    {typeof project.budget_month_usd === "number" ? (
                      <span>{formatMoney(project.budget_month_usd)}/mo</span>
                    ) : (
                      <span>No budget</span>
                    )}
                  </div>
                </button>
              ))}
            </div>
          ) : null}
        </Surface>
        </div>
      ) : null}

      {activeSection === "tasks" ? (
        <div
          className="mc-page-section-shell"
          id="strategy-panel-tasks"
          role="tabpanel"
          aria-labelledby="strategy-tab-tasks"
        >
        <Surface
          className="mc-strategy-list"
          title="Tasks"
          subtitle={`${controller.filteredTasks.length} tasks in the active management slice`}
          headerRight={
            <div className="mc-strategy-inline-actions">
              <button
                type="button"
                className="ghost"
                onClick={() => runWithTaskDraftGuard(() => controller.applySummaryLens("all"))}
              >
                Clear lens
              </button>
              <button
                type="button"
                onClick={() => {
                  runWithTaskDraftGuard(() => {
                    setActiveSection("detail");
                    setDetailSection("basics");
                    setTaskError(null);
                    setTaskMode("create");
                    controller.setSelectedTaskId("");
                    setTaskForm(hydrateTaskForm(null, controller.selectedProjectId));
                  });
                }}
                disabled={!controller.selectedProjectId}
              >
                New Task
              </button>
            </div>
          }
        >
          <div className="mc-strategy-filter-bar">
            <label>
              Search
              <input
                value={controller.taskFilters.query}
                onChange={(event) =>
                  runWithTaskDraftGuard(() =>
                    controller.updateFilters({ query: event.target.value })
                  )
                }
                placeholder="Search tasks, projects, goals, owners"
              />
            </label>
            <label>
              Status
              <select
                value={controller.taskFilters.status}
                onChange={(event) =>
                  runWithTaskDraftGuard(() =>
                    controller.updateFilters({ status: event.target.value })
                  )
                }
              >
                <option value="all">All</option>
                {TASK_STATUS_OPTIONS.map((status) => (
                  <option key={status} value={status}>
                    {status}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Owner
              <select
                value={controller.taskFilters.owner_agent_id}
                onChange={(event) =>
                  runWithTaskDraftGuard(() =>
                    controller.updateFilters({ owner_agent_id: event.target.value })
                  )
                }
              >
                <option value="">All owners</option>
                {agents.map((agent) => (
                  <option key={agent.agent_id} value={agent.agent_id}>
                    {agent.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Hierarchy Root
              <select
                value={controller.taskFilters.hierarchy_root_agent_id}
                onChange={(event) =>
                  runWithTaskDraftGuard(() =>
                    controller.updateFilters({
                      hierarchy_root_agent_id: event.target.value,
                    })
                  )
                }
              >
                <option value="">All orgs</option>
                {agents.map((agent) => (
                  <option key={agent.agent_id} value={agent.agent_id}>
                    {agent.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="mc-strategy-chip-row">
            <button
              type="button"
              className={`mc-strategy-filter-chip${
                controller.taskFilters.blocked ? " is-active" : ""
              }`}
              aria-pressed={controller.taskFilters.blocked}
              onClick={() =>
                runWithTaskDraftGuard(() =>
                  controller.updateFilters({ blocked: !controller.taskFilters.blocked })
                )
              }
            >
              Blocked
            </button>
            <button
              type="button"
              className={`mc-strategy-filter-chip${
                controller.taskFilters.stale ? " is-active" : ""
              }`}
              aria-pressed={controller.taskFilters.stale}
              onClick={() =>
                runWithTaskDraftGuard(() =>
                  controller.updateFilters({ stale: !controller.taskFilters.stale })
                )
              }
            >
              Stale
            </button>
            <button
              type="button"
              className={`mc-strategy-filter-chip${
                controller.taskFilters.unassigned ? " is-active" : ""
              }`}
              aria-pressed={controller.taskFilters.unassigned}
              onClick={() =>
                runWithTaskDraftGuard(() =>
                  controller.updateFilters({
                    unassigned: !controller.taskFilters.unassigned,
                  })
                )
              }
            >
              Unassigned
            </button>
            <button
              type="button"
              className={`mc-strategy-filter-chip${
                controller.taskFilters.include_archived ? " is-active" : ""
              }`}
              aria-pressed={controller.taskFilters.include_archived}
              onClick={() =>
                runWithTaskDraftGuard(() =>
                  controller.updateFilters({
                    include_archived: !controller.taskFilters.include_archived,
                  })
                )
              }
            >
              Include Archived
            </button>
            <button
              type="button"
              className="mc-strategy-filter-chip"
              onClick={() => runWithTaskDraftGuard(controller.resetFilters)}
            >
              Reset
            </button>
            {controller.isFilterTransitionPending ? (
              <span className="mc-strategy-filter-pending" aria-label="Filtering\u2026">Filtering\u2026</span>
            ) : null}
          </div>

          <div className="mc-strategy-task-list">
            {controller.filteredTasks.map((task) => {
              const project = controller.projectById.get(task.project_id);
              const goal = project ? controller.goalById.get(project.goal_id) : null;
              return (
                <button
                  key={task.task_id}
                  type="button"
                  className={`mc-strategy-task-row${
                    task.task_id === controller.selectedTaskId ? " is-active" : ""
                  }`}
                  onClick={() => {
                    runWithTaskDraftGuard(() => {
                      setActiveSection("detail");
                      setDetailSection("basics");
                      setTaskMode("edit");
                      controller.setSelectedTaskId(task.task_id);
                    });
                  }}
                >
                  <div className="mc-strategy-task-row-head">
                    <strong>{task.title}</strong>
                    <Chip label={task.priority} tone={toneForStatus(task.priority)} />
                  </div>
                  <div className="mc-strategy-task-row-meta">
                    <Chip label={task.status} tone={toneForStatus(task.status)} />
                    <span>{project?.name ?? "Unknown project"}</span>
                    <span>{goal?.title ?? "Unknown goal"}</span>
                  </div>
                  <div className="mc-strategy-task-row-foot">
                    <span>{formatRelative(task.updated_at)}</span>
                    <span>
                      {task.owner_agent_id
                        ? controller.org.agentsById.get(task.owner_agent_id)?.name ??
                          task.owner_agent_id
                        : "Unassigned"}
                    </span>
                  </div>
                </button>
              );
            })}
            {controller.filteredTasks.length === 0 ? (
              <EmptyState message="No tasks match the current Strategy filters." />
            ) : null}
          </div>
        </Surface>
        </div>
      ) : null}

      {activeSection === "detail" ? (
        <div
          className="mc-page-section-shell"
          id="strategy-panel-detail"
          role="tabpanel"
          aria-labelledby="strategy-tab-detail"
        >
        <Surface
          className="mc-strategy-detail"
          title={
            taskMode === "create"
              ? "New Task Draft"
              : activeTaskForm.title || "Task Detail"
          }
          subtitle={
            taskMode === "create"
              ? "Create management work here; execution artifacts link in later."
              : activeTaskForm.task_id
          }
          headerRight={
            <div className="mc-strategy-inline-actions">
              {controller.selectedGoal ? (
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    runWithAnyDraftGuard(() => {
                      setGoalError(null);
                      setGoalForm(hydrateGoalForm(controller.selectedGoal));
                      setGoalModalMode("edit");
                    });
                  }}
                >
                  Edit Goal
                </button>
              ) : null}
              {controller.selectedProject ? (
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    runWithAnyDraftGuard(() => {
                      setProjectError(null);
                      setProjectForm(
                        hydrateProjectForm(controller.selectedProject, controller.selectedGoalId)
                      );
                      setProjectModalMode("edit");
                    });
                  }}
                >
                  Edit Project
                </button>
              ) : null}
              {taskDirty ? <Chip label="Unsaved draft" tone="warning" /> : null}
              <button type="button" onClick={() => void saveTask()}>
                {controller.mutating ? "Saving..." : taskMode === "create" ? "Create Task" : "Save Task"}
              </button>
            </div>
          }
        >
          {activeTaskForm.project_id ? (
            <>
              <div className="mc-page-section-tabs" aria-label="Task detail sections">
                <button
                  type="button"
                  className={`mc-page-section-btn${detailSection === "basics" ? " mc-page-section-btn-active" : ""}`}
                  onClick={() => setDetailSection("basics")}
                >
                  Basics
                </button>
                <button
                  type="button"
                  className={`mc-page-section-btn${detailSection === "links" ? " mc-page-section-btn-active" : ""}`}
                  onClick={() => setDetailSection("links")}
                >
                  Links & Runbook
                </button>
              </div>

              {detailSection === "basics" ? (
                <div className="mc-page-section-stack">
              <div className="mc-strategy-form-grid">
                <label>
                  Project
                  <select
                    value={activeTaskForm.project_id}
                    onChange={(event) =>
                      updateTaskForm((current) => ({
                        ...current,
                        project_id: event.target.value,
                      }))
                    }
                  >
                    {controller.projectsForSelectedGoal.map((project) => (
                      <option key={project.project_id} value={project.project_id}>
                        {project.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Parent Task
                  <select
                    value={activeTaskForm.parent_task_id}
                    onChange={(event) =>
                      updateTaskForm((current) => ({
                        ...current,
                        parent_task_id: event.target.value,
                      }))
                    }
                  >
                    <option value="">No parent</option>
                    {selectedProjectTasks.map((task) => (
                      <option key={task.task_id} value={task.task_id}>
                        {task.title}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Status
                  <select
                    value={activeTaskForm.status}
                    onChange={(event) =>
                      updateTaskForm((current) => ({
                        ...current,
                        status: event.target.value,
                        blocked_reason:
                          event.target.value === "blocked" ? current.blocked_reason : "",
                      }))
                    }
                  >
                    {TASK_STATUS_OPTIONS.map((status) => (
                      <option key={status} value={status}>
                        {status}
                      </option>
                    ))}
                  </select>
                  {activeTaskForm.status !== "blocked" ? (
                    <small className="mc-strategy-field-hint">Set to &ldquo;blocked&rdquo; to add a blocked reason.</small>
                  ) : null}
                </label>
                <label>
                  Priority
                  <select
                    value={activeTaskForm.priority}
                    onChange={(event) =>
                      updateTaskForm((current) => ({
                        ...current,
                        priority: event.target.value,
                      }))
                    }
                  >
                    {TASK_PRIORITY_OPTIONS.map((priority) => (
                      <option key={priority} value={priority}>
                        {priority}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              <label>
                Title
                <input
                  value={activeTaskForm.title}
                  onChange={(event) =>
                    updateTaskForm((current) => ({
                      ...current,
                      title: event.target.value,
                    }))
                  }
                  placeholder="Improve review throughput"
                />
              </label>
              <label>
                Detail
                <textarea
                  rows={6}
                  value={activeTaskForm.detail}
                  onChange={(event) =>
                    updateTaskForm((current) => ({
                      ...current,
                      detail: event.target.value,
                    }))
                  }
                  placeholder="Context, acceptance criteria, and management notes."
                />
              </label>

              <div className="mc-strategy-form-grid">
                <label>
                  Owner
                  <select
                    value={activeTaskForm.owner_agent_id}
                    onChange={(event) =>
                      updateTaskForm((current) => ({
                        ...current,
                        owner_agent_id: event.target.value,
                      }))
                    }
                  >
                    <option value="">Unassigned</option>
                    {agents.map((agent) => (
                      <option key={agent.agent_id} value={agent.agent_id}>
                        {agent.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Due At
                  <input
                    type="datetime-local"
                    value={activeTaskForm.due_at}
                    onChange={(event) =>
                      updateTaskForm((current) => ({
                        ...current,
                        due_at: event.target.value,
                      }))
                    }
                  />
                </label>
              </div>

              {controller.suggestedOwnerAgentIds.length > 0 ? (
                <div className="mc-strategy-suggestion-row">
                  <span>Suggested owners</span>
                  <div className="mc-strategy-chip-row">
                    {controller.suggestedOwnerAgentIds.map((agentId) => (
                      <button
                        key={agentId}
                        type="button"
                        className={`mc-strategy-filter-chip${
                          activeTaskForm.owner_agent_id === agentId ? " is-active" : ""
                        }`}
                        onClick={() =>
                          updateTaskForm((current) => ({
                            ...current,
                            owner_agent_id: agentId,
                          }))
                        }
                      >
                        {controller.org.agentsById.get(agentId)?.name ?? agentId}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}

              {activeTaskForm.status === "blocked" ? (
                <label>
                  Blocked Reason
                  <textarea
                    rows={3}
                    value={activeTaskForm.blocked_reason}
                    onChange={(event) =>
                      updateTaskForm((current) => ({
                        ...current,
                        blocked_reason: event.target.value,
                      }))
                    }
                    placeholder="What is stopping this task from moving?"
                  />
                </label>
              ) : null}
                </div>
              ) : null}

              {detailSection === "links" ? (
                <div className="mc-page-section-stack">
                  {runbookEnabled && taskMode === "edit" ? (
                    <RunbookLinkPanel
                      className="mc-strategy-runbook-panel"
                      summary={selectedTaskRunbook}
                      emptyMessage="Runbook appears here once this task has linked execution truth."
                      onOpen={
                        selectedTaskId
                          ? () => onOpenTaskRunbook(selectedTaskId)
                          : undefined
                      }
                    />
                  ) : null}

                  <div className="mc-strategy-link-card">
                    <div className="mc-strategy-subheader">
                      <div>
                        <h3>Execution Links</h3>
                        <p>Connect the management record to boards and jobs without rewriting runtime state.</p>
                      </div>
                    </div>
                    <div className="mc-strategy-form-grid">
                      <label>
                        Linked Board Card ID
                        <input
                          value={activeTaskForm.linked_board_card_id}
                          onChange={(event) =>
                            updateTaskForm((current) => ({
                              ...current,
                              linked_board_card_id: event.target.value,
                            }))
                          }
                          placeholder="card-ops-1"
                        />
                      </label>
                      <label>
                        Linked Job ID
                        <input
                          value={activeTaskForm.linked_job_id}
                          onChange={(event) =>
                            updateTaskForm((current) => ({
                              ...current,
                              linked_job_id: event.target.value,
                            }))
                          }
                          placeholder="job-heartbeat"
                        />
                      </label>
                    </div>
                    <div className="mc-strategy-form-grid">
                      <label className="mc-settings-toggle">
                        <input
                          type="checkbox"
                          checked={forceBoardReassign}
                          onChange={(event) => setForceBoardReassign(event.target.checked)}
                        />
                        <span>Force board-card reassignment if already linked</span>
                      </label>
                      <label className="mc-settings-toggle">
                        <input
                          type="checkbox"
                          checked={forceJobReassign}
                          onChange={(event) => setForceJobReassign(event.target.checked)}
                        />
                        <span>Force job reassignment if already linked</span>
                      </label>
                    </div>
                    {taskMode === "edit" ? (
                      <div className="mc-strategy-runtime-meta">
                        <span>Latest run: {controller.selectedTask?.latest_run_id ?? "n/a"}</span>
                        <span>Latest session: {controller.selectedTask?.latest_session_id ?? "n/a"}</span>
                      </div>
                    ) : null}
                  </div>
                </div>
              ) : null}

              {taskError ? <p className="mc-settings-inline-error">{taskError}</p> : null}
            </>
          ) : (
            <EmptyState message="Select a project to start a task draft." />
          )}
        </Surface>
        </div>
      ) : null}

      {activeSection === "insights" ? (
        <div
          className="mc-page-section-shell"
          id="strategy-panel-insights"
          role="tabpanel"
          aria-labelledby="strategy-tab-insights"
        >
      <div className="mc-strategy-insights-grid">
        <Surface title="Spend by Agent" subtitle="Execution cost attributed to managed work.">
          <div className="mc-strategy-metric-list">
            {(controller.summary?.spend_by_agent ?? []).map((item) => (
              <button
                key={item.agent_id}
                type="button"
                className="mc-strategy-metric-row"
                onClick={() =>
                  runWithTaskDraftGuard(() =>
                    controller.updateFilters({
                      owner_agent_id: item.agent_id,
                      blocked: false,
                      stale: false,
                    })
                  )
                }
              >
                <div>
                  <strong>{item.agent_name}</strong>
                  <p>{item.linked_task_count} linked tasks</p>
                </div>
                <span>{formatMoney(item.estimated_cost_total)}</span>
              </button>
            ))}
            {(controller.summary?.spend_by_agent ?? []).length === 0 ? (
              <EmptyState message="No managed spend attributed by agent yet." />
            ) : null}
          </div>
        </Surface>

        <Surface title="Spend by Project" subtitle="Project-level cost rollup from linked work.">
          <div className="mc-strategy-metric-list">
            {(controller.summary?.spend_by_project ?? []).map((item) => (
              <button
                key={item.project_id}
                type="button"
                className="mc-strategy-metric-row"
                onClick={() => {
                  runWithTaskDraftGuard(() => {
                    controller.setSelectedGoalId(item.goal_id);
                    controller.setSelectedProjectId(item.project_id);
                  });
                }}
              >
                <div>
                  <strong>{item.project_name}</strong>
                  <p>{item.goal_title}</p>
                </div>
                <span>{formatMoney(item.estimated_cost_total)}</span>
              </button>
            ))}
            {(controller.summary?.spend_by_project ?? []).length === 0 ? (
              <EmptyState message="No project spend rollup is available yet." />
            ) : null}
          </div>
        </Surface>

        <Surface title="Goal Progress" subtitle="Derived from non-archived leaf tasks only.">
          <div className="mc-strategy-metric-list">
            {(controller.summary?.goal_progress ?? []).map((item) => (
              <button
                key={item.goal_id}
                type="button"
                className="mc-strategy-metric-row"
                onClick={() =>
                  runWithTaskDraftGuard(() => controller.setSelectedGoalId(item.goal_id))
                }
              >
                <div>
                  <strong>{item.title}</strong>
                  <p>
                    {item.open_task_count} open · {item.blocked_task_count} blocked
                  </p>
                </div>
                <span>{item.progress_pct}%</span>
              </button>
            ))}
            {(controller.summary?.goal_progress ?? []).length === 0 ? (
              <EmptyState message="Goal progress appears here once work is linked." />
            ) : null}
          </div>
        </Surface>

        <Surface
          title="Critical Approval Backlog"
          subtitle={`Unattributed spend currently totals ${formatMoney(
            controller.summary?.unattributed_spend_total ?? 0
          )}.`}
        >
          <div className="mc-strategy-metric-list">
            {(controller.summary?.critical_approval_backlog ?? []).map((item) => (
              <button
                key={item.approval_id}
                type="button"
                className="mc-strategy-metric-row"
                onClick={() => {
                  const linkedTaskId = item.linked_task_id;
                  if (linkedTaskId) {
                    runWithTaskDraftGuard(() => {
                      setActiveSection("detail");
                      setDetailSection("links");
                      controller.openTaskById(linkedTaskId);
                    });
                  }
                }}
              >
                <div>
                  <strong>{item.summary}</strong>
                  <p>
                    {item.kind} · requested {formatRelative(item.requested_at)}
                  </p>
                </div>
                <span>{item.linked_task_id ? "Open task" : "No task"}</span>
              </button>
            ))}
            {(controller.summary?.critical_approval_backlog ?? []).length === 0 ? (
              <EmptyState message="No critical approvals are waiting on operator action." />
            ) : null}
          </div>
        </Surface>
      </div>
        </div>
      ) : null}

      <Modal
        open={goalModalMode !== null}
        onClose={() => {
          if (!confirmDiscard(goalDirty)) {
            return;
          }
          setGoalModalMode(null);
        }}
        title={goalModalMode === "create" ? "Create Goal" : "Edit Goal"}
        width="640px"
        footer={
          <>
            <button
              type="button"
              className="ghost"
              onClick={() => {
                if (!confirmDiscard(goalDirty)) {
                  return;
                }
                setGoalModalMode(null);
              }}
            >
              Cancel
            </button>
            <button type="button" onClick={() => void saveGoal()}>
              Save Goal
            </button>
          </>
        }
      >
        <div className="mc-strategy-form-grid">
          <label>
            Slug
            <input
              value={goalForm.slug}
              onChange={(event) =>
                setGoalForm((current) => ({ ...current, slug: event.target.value }))
              }
              placeholder="release-quality"
            />
          </label>
          <label>
            Status
            <select
              value={goalForm.status}
              onChange={(event) =>
                setGoalForm((current) => ({ ...current, status: event.target.value }))
              }
            >
              {GOAL_STATUS_OPTIONS.map((status) => (
                <option key={status} value={status}>
                  {status}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label>
          Title
          <input
            value={goalForm.title}
            onChange={(event) =>
              setGoalForm((current) => ({ ...current, title: event.target.value }))
            }
          />
        </label>
        <label>
          Summary
          <textarea
            rows={5}
            value={goalForm.summary}
            onChange={(event) =>
              setGoalForm((current) => ({ ...current, summary: event.target.value }))
            }
          />
        </label>
        <div className="mc-strategy-form-grid">
          <label>
            Owner
            <select
              value={goalForm.owner_agent_id}
              onChange={(event) =>
                setGoalForm((current) => ({
                  ...current,
                  owner_agent_id: event.target.value,
                }))
              }
            >
              <option value="">Unassigned</option>
              {agents.map((agent) => (
                <option key={agent.agent_id} value={agent.agent_id}>
                  {agent.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Target Date
            <input
              type="datetime-local"
              value={goalForm.target_date}
              onChange={(event) =>
                setGoalForm((current) => ({
                  ...current,
                  target_date: event.target.value,
                }))
              }
            />
          </label>
        </div>
        {goalError ? <p className="mc-settings-inline-error">{goalError}</p> : null}
      </Modal>

      <Modal
        open={projectModalMode !== null}
        onClose={() => {
          if (!confirmDiscard(projectDirty)) {
            return;
          }
          setProjectModalMode(null);
        }}
        title={projectModalMode === "create" ? "Create Project" : "Edit Project"}
        width="680px"
        footer={
          <>
            <button
              type="button"
              className="ghost"
              onClick={() => {
                if (!confirmDiscard(projectDirty)) {
                  return;
                }
                setProjectModalMode(null);
              }}
            >
              Cancel
            </button>
            <button type="button" onClick={() => void saveProject()}>
              Save Project
            </button>
          </>
        }
      >
        <div className="mc-strategy-form-grid">
          <label>
            Goal
            <select
              value={projectForm.goal_id}
              onChange={(event) =>
                setProjectForm((current) => ({ ...current, goal_id: event.target.value }))
              }
            >
              {controller.goals.map((goal) => (
                <option key={goal.goal_id} value={goal.goal_id}>
                  {goal.title}
                </option>
              ))}
            </select>
          </label>
          <label>
            Status
            <select
              value={projectForm.status}
              onChange={(event) =>
                setProjectForm((current) => ({ ...current, status: event.target.value }))
              }
            >
              {PROJECT_STATUS_OPTIONS.map((status) => (
                <option key={status} value={status}>
                  {status}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="mc-strategy-form-grid">
          <label>
            Slug
            <input
              value={projectForm.slug}
              onChange={(event) =>
                setProjectForm((current) => ({ ...current, slug: event.target.value }))
              }
            />
          </label>
          <label>
            Name
            <input
              value={projectForm.name}
              onChange={(event) =>
                setProjectForm((current) => ({ ...current, name: event.target.value }))
              }
            />
          </label>
        </div>
        <label>
          Summary
          <textarea
            rows={4}
            value={projectForm.summary}
            onChange={(event) =>
              setProjectForm((current) => ({ ...current, summary: event.target.value }))
            }
          />
        </label>
        <div className="mc-strategy-form-grid">
          <label>
            Owner
            <select
              value={projectForm.owner_agent_id}
              onChange={(event) =>
                setProjectForm((current) => ({
                  ...current,
                  owner_agent_id: event.target.value,
                }))
              }
            >
              <option value="">Unassigned</option>
              {agents.map((agent) => (
                <option key={agent.agent_id} value={agent.agent_id}>
                  {agent.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Workspace Root
            <input
              value={projectForm.workspace_root}
              onChange={(event) =>
                setProjectForm((current) => ({
                  ...current,
                  workspace_root: event.target.value,
                }))
              }
            />
          </label>
        </div>
        <label>
          Budget / Month (USD)
          <input
            value={projectForm.budget_month_usd}
            onChange={(event) =>
              setProjectForm((current) => ({
                ...current,
                budget_month_usd: event.target.value,
              }))
            }
            placeholder="2500"
          />
        </label>
        {projectError ? <p className="mc-settings-inline-error">{projectError}</p> : null}
      </Modal>
    </section>
  );
}
