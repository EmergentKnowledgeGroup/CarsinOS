import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
} from "react";
import {
  clearTaskLinks,
  createBootstrapPreset,
  createGoal,
  createProject,
  createTask,
  exportBootstrapPreset,
  getStrategySummary,
  importBootstrapPreset,
  linkTaskBoardCard,
  linkTaskJob,
  listBootstrapPresets,
  listGoals,
  listProjects,
  listTasks,
  updateBootstrapPreset,
  updateGoal,
  updateProject,
  updateTask,
} from "../../lib/api";
import { resolveOperatorTimezone, resolveTzOffsetMinutes } from "../../lib/operatorTime";
import type { NotifyFn } from "../../app/useAppController";
import type {
  Agent,
  BootstrapPresetResponse,
  GoalResponse,
  ProjectResponse,
  RuntimeConnectionSettings,
  StrategySummaryResponse,
  TaskResponse,
} from "../../types";
import {
  STRATEGY_FETCH_PAGE_LIMIT,
  STRATEGY_OWNER_SUGGESTION_LIMIT,
  STRATEGY_UNSUPPORTED_STATUS_FRAGMENTS,
} from "./strategyConfig";
import { buildAgentOrgModel } from "./strategyOrg";

type StrategyAvailability = "disabled" | "loading" | "ready" | "unsupported" | "error";

export interface StrategyTaskFilters {
  query: string;
  status: string;
  owner_agent_id: string;
  blocked: boolean;
  stale: boolean;
  unassigned: boolean;
  hierarchy_root_agent_id: string;
  hierarchy_scope: "all" | "subtree";
  include_archived: boolean;
}

export interface StrategyTaskContextSnapshot {
  task: TaskResponse;
  project: ProjectResponse | null;
  goal: GoalResponse | null;
  owner: Agent | null;
  managerChain: Agent[];
}

interface UseStrategyControllerOptions {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  enabled: boolean;
  setNotice: NotifyFn;
}

const DEFAULT_TASK_FILTERS: StrategyTaskFilters = {
  query: "",
  status: "all",
  owner_agent_id: "",
  blocked: false,
  stale: false,
  unassigned: false,
  hierarchy_root_agent_id: "",
  hierarchy_scope: "subtree",
  include_archived: false,
};

function normalizeErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isUnsupportedError(error: unknown): boolean {
  const message = normalizeErrorMessage(error).toLowerCase();
  return STRATEGY_UNSUPPORTED_STATUS_FRAGMENTS.some((fragment) =>
    message.includes(fragment)
  );
}

async function fetchAllPages<T>(
  loader: (cursor?: string) => Promise<{ items: T[]; next_cursor: string | null }>
): Promise<T[]> {
  const items: T[] = [];
  let cursor: string | undefined;
  do {
    const response = await loader(cursor);
    items.push(...response.items);
    cursor = response.next_cursor ?? undefined;
  } while (cursor);
  return items;
}

function sortGoals(goals: GoalResponse[]): GoalResponse[] {
  return [...goals].sort((left, right) => right.updated_at - left.updated_at);
}

function sortProjects(projects: ProjectResponse[]): ProjectResponse[] {
  return [...projects].sort((left, right) => right.updated_at - left.updated_at);
}

function sortTasks(tasks: TaskResponse[]): TaskResponse[] {
  return [...tasks].sort((left, right) => {
    const leftDue = left.due_at ?? Number.MAX_SAFE_INTEGER;
    const rightDue = right.due_at ?? Number.MAX_SAFE_INTEGER;
    if (leftDue !== rightDue) {
      return leftDue - rightDue;
    }
    return right.updated_at - left.updated_at;
  });
}

export function useStrategyController(options: UseStrategyControllerOptions) {
  const { settings, agents, enabled, setNotice } = options;
  const [availability, setAvailability] = useState<StrategyAvailability>(
    enabled ? "loading" : "disabled"
  );
  const [availabilityMessage, setAvailabilityMessage] = useState<string | null>(null);
  const [summary, setSummary] = useState<StrategySummaryResponse | null>(null);
  const [goals, setGoals] = useState<GoalResponse[]>([]);
  const [projects, setProjects] = useState<ProjectResponse[]>([]);
  const [tasks, setTasks] = useState<TaskResponse[]>([]);
  const [presets, setPresets] = useState<BootstrapPresetResponse[]>([]);
  const [taskFilters, setTaskFilters] = useState<StrategyTaskFilters>(DEFAULT_TASK_FILTERS);
  const [selectedGoalId, setSelectedGoalId] = useState("");
  const [selectedProjectId, setSelectedProjectId] = useState("");
  const [selectedTaskId, setSelectedTaskId] = useState("");
  const [mutating, setMutating] = useState<string | null>(null);
  const refreshTimerRef = useRef<number | null>(null);
  const [isFilterTransitionPending, startFilterTransition] = useTransition();

  const org = useMemo(() => buildAgentOrgModel(agents), [agents]);
  const deferredQuery = useDeferredValue(taskFilters.query.trim().toLowerCase());

  const goalById = useMemo(
    () => new Map(goals.map((goal) => [goal.goal_id, goal] as const)),
    [goals]
  );
  const projectById = useMemo(
    () => new Map(projects.map((project) => [project.project_id, project] as const)),
    [projects]
  );
  const taskById = useMemo(
    () => new Map(tasks.map((task) => [task.task_id, task] as const)),
    [tasks]
  );
  const taskByBoardCardId = useMemo(() => {
    const map = new Map<string, TaskResponse>();
    for (const task of tasks) {
      if (task.linked_board_card_id) {
        map.set(task.linked_board_card_id, task);
      }
    }
    return map;
  }, [tasks]);
  const taskByJobId = useMemo(() => {
    const map = new Map<string, TaskResponse>();
    for (const task of tasks) {
      if (task.linked_job_id) {
        map.set(task.linked_job_id, task);
      }
    }
    return map;
  }, [tasks]);
  const approvalTaskByApprovalId = useMemo(() => {
    const map = new Map<string, TaskResponse>();
    for (const item of summary?.critical_approval_backlog ?? []) {
      if (!item.linked_task_id) {
        continue;
      }
      const task = taskById.get(item.linked_task_id);
      if (task) {
        map.set(item.approval_id, task);
      }
    }
    return map;
  }, [summary, taskById]);

  const selectedGoal = selectedGoalId ? goalById.get(selectedGoalId) ?? null : null;
  const projectsForSelectedGoal = useMemo(() => {
    if (!selectedGoalId) {
      return projects;
    }
    return projects.filter((project) => project.goal_id === selectedGoalId);
  }, [projects, selectedGoalId]);
  const selectedProject = selectedProjectId
    ? projectById.get(selectedProjectId) ?? null
    : null;

  const filteredTasks = useMemo(() => {
    const selectedHierarchyIds = taskFilters.hierarchy_root_agent_id
      ? new Set(
          taskFilters.hierarchy_scope === "subtree"
            ? org.subtreeIdsByAgentId.get(taskFilters.hierarchy_root_agent_id) ?? [
                taskFilters.hierarchy_root_agent_id,
              ]
            : [taskFilters.hierarchy_root_agent_id]
        )
      : null;

    const visible = tasks.filter((task) => {
      if (!taskFilters.include_archived && task.status === "archived") {
        return false;
      }
      if (selectedGoalId) {
        const project = projectById.get(task.project_id);
        if (!project || project.goal_id !== selectedGoalId) {
          return false;
        }
      }
      if (selectedProjectId && task.project_id !== selectedProjectId) {
        return false;
      }
      if (taskFilters.status !== "all" && task.status !== taskFilters.status) {
        return false;
      }
      if (taskFilters.owner_agent_id && task.owner_agent_id !== taskFilters.owner_agent_id) {
        return false;
      }
      if (taskFilters.blocked && task.status !== "blocked") {
        return false;
      }
      if (taskFilters.stale) {
        const taskSummary = summary?.stale_tasks.find((item) => item.task_id === task.task_id);
        if (!taskSummary) {
          return false;
        }
      }
      if (taskFilters.unassigned && Boolean(task.owner_agent_id)) {
        return false;
      }
      if (
        selectedHierarchyIds &&
        (!task.owner_agent_id || !selectedHierarchyIds.has(task.owner_agent_id))
      ) {
        return false;
      }
      if (!deferredQuery) {
        return true;
      }
      const project = projectById.get(task.project_id);
      const goal = project ? goalById.get(project.goal_id) : null;
      const searchHaystack = [
        task.title,
        task.detail,
        task.status,
        task.priority,
        project?.name,
        goal?.title,
        task.owner_agent_id ? org.agentsById.get(task.owner_agent_id)?.name : null,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return searchHaystack.includes(deferredQuery);
    });
    return sortTasks(visible);
  }, [
    deferredQuery,
    goalById,
    org.agentsById,
    org.subtreeIdsByAgentId,
    projectById,
    selectedGoalId,
    selectedProjectId,
    summary?.stale_tasks,
    taskFilters,
    tasks,
  ]);

  const selectedTask = useMemo(() => {
    if (selectedTaskId) {
      return taskById.get(selectedTaskId) ?? null;
    }
    return filteredTasks[0] ?? null;
  }, [filteredTasks, selectedTaskId, taskById]);

  const taskCountByProjectId = useMemo(() => {
    const counts = new Map<string, number>();
    for (const task of tasks) {
      counts.set(task.project_id, (counts.get(task.project_id) ?? 0) + 1);
    }
    return counts;
  }, [tasks]);

  const suggestedOwnerAgentIds = useMemo(() => {
    const rootAgentId =
      taskFilters.hierarchy_root_agent_id ||
      selectedProject?.owner_agent_id ||
      selectedGoal?.owner_agent_id ||
      "";
    if (!rootAgentId) {
      return [];
    }
    const subtree = org.subtreeIdsByAgentId.get(rootAgentId) ?? [rootAgentId];
    return subtree.slice(0, STRATEGY_OWNER_SUGGESTION_LIMIT);
  }, [
    org.subtreeIdsByAgentId,
    selectedGoal?.owner_agent_id,
    selectedProject?.owner_agent_id,
    taskFilters.hierarchy_root_agent_id,
  ]);

  const loadStrategyData = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (!enabled) {
        setAvailability("disabled");
        setAvailabilityMessage("Strategy hub is disabled in Config.");
        setSummary(null);
        setGoals([]);
        setProjects([]);
        setTasks([]);
        setPresets([]);
        return;
      }

      setAvailability((current) => (current === "ready" ? current : "loading"));
      setAvailabilityMessage(null);
      const timezone = resolveOperatorTimezone();
      const tzOffsetMinutes = resolveTzOffsetMinutes();

      const results = await Promise.allSettled([
        getStrategySummary(runtimeSettings, {
          timezone,
          tz_offset_minutes: tzOffsetMinutes,
        }),
        fetchAllPages((cursor) =>
          listGoals(runtimeSettings, {
            limit: STRATEGY_FETCH_PAGE_LIMIT,
            cursor,
          })
        ),
        fetchAllPages((cursor) =>
          listProjects(runtimeSettings, {
            limit: STRATEGY_FETCH_PAGE_LIMIT,
            cursor,
          })
        ),
        fetchAllPages((cursor) =>
          listTasks(runtimeSettings, {
            limit: STRATEGY_FETCH_PAGE_LIMIT,
            cursor,
          })
        ),
        fetchAllPages((cursor) =>
          listBootstrapPresets(runtimeSettings, {
            limit: STRATEGY_FETCH_PAGE_LIMIT,
            cursor,
          })
        ),
      ]);

      const [summaryResult, goalsResult, projectsResult, tasksResult, presetsResult] = results;
      const coreResults = [summaryResult, goalsResult, projectsResult, tasksResult];

      const unsupportedResult = coreResults.find(
        (result) => result.status === "rejected" && isUnsupportedError(result.reason)
      );
      if (unsupportedResult && unsupportedResult.status === "rejected") {
        setAvailability("unsupported");
        setAvailabilityMessage(
          "The connected gateway does not expose the Strategy management surface yet."
        );
        setSummary(null);
        setGoals([]);
        setProjects([]);
        setTasks([]);
        setPresets([]);
        return;
      }

      const rejectedCore = coreResults.find((result) => result.status === "rejected");
      if (rejectedCore && rejectedCore.status === "rejected") {
        setAvailability("error");
        setAvailabilityMessage(normalizeErrorMessage(rejectedCore.reason));
        return;
      }

      setSummary(
        summaryResult.status === "fulfilled" ? summaryResult.value : null
      );
      setGoals(
        goalsResult.status === "fulfilled" ? sortGoals(goalsResult.value) : []
      );
      setProjects(
        projectsResult.status === "fulfilled" ? sortProjects(projectsResult.value) : []
      );
      setTasks(tasksResult.status === "fulfilled" ? sortTasks(tasksResult.value) : []);
      setPresets(
        presetsResult.status === "fulfilled" ? presetsResult.value : []
      );
      setAvailability("ready");
      if (presetsResult.status === "rejected" && !isUnsupportedError(presetsResult.reason)) {
        setNotice({
          tone: "error",
          message: `Bootstrap preset load failed: ${normalizeErrorMessage(
            presetsResult.reason
          )}`,
        });
      }
    },
    [enabled, setNotice, settings]
  );

  const queueRefresh = useCallback(
    (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (refreshTimerRef.current) {
        window.clearTimeout(refreshTimerRef.current);
      }
      refreshTimerRef.current = window.setTimeout(() => {
        void loadStrategyData(runtimeSettings).catch((error: unknown) => {
          setAvailability("error");
          setAvailabilityMessage(normalizeErrorMessage(error));
        });
      }, 250);
    },
    [loadStrategyData, settings]
  );

  useEffect(() => {
    void loadStrategyData(settings).catch((error: unknown) => {
      setAvailability("error");
      setAvailabilityMessage(normalizeErrorMessage(error));
    });
  }, [enabled, loadStrategyData, settings]);

  useEffect(() => {
    return () => {
      if (refreshTimerRef.current) {
        window.clearTimeout(refreshTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (goals.length === 0) {
      setSelectedGoalId("");
      return;
    }
    if (!selectedGoalId || !goalById.has(selectedGoalId)) {
      setSelectedGoalId(goals[0].goal_id);
    }
  }, [goalById, goals, selectedGoalId]);

  useEffect(() => {
    if (projectsForSelectedGoal.length === 0) {
      setSelectedProjectId("");
      return;
    }
    if (
      !selectedProjectId ||
      !projectsForSelectedGoal.some((project) => project.project_id === selectedProjectId)
    ) {
      setSelectedProjectId(projectsForSelectedGoal[0].project_id);
    }
  }, [projectsForSelectedGoal, selectedProjectId]);

  useEffect(() => {
    if (filteredTasks.length === 0) {
      setSelectedTaskId("");
      return;
    }
    if (
      !selectedTaskId ||
      !filteredTasks.some((task) => task.task_id === selectedTaskId)
    ) {
      setSelectedTaskId(filteredTasks[0].task_id);
    }
  }, [filteredTasks, selectedTaskId]);

  const updateFilters = useCallback((patch: Partial<StrategyTaskFilters>) => {
    startFilterTransition(() => {
      setTaskFilters((current) => ({ ...current, ...patch }));
    });
  }, []);

  const resetFilters = useCallback(() => {
    startFilterTransition(() => {
      setTaskFilters(DEFAULT_TASK_FILTERS);
    });
  }, []);

  const openTaskById = useCallback(
    (taskId: string): boolean => {
      const task = taskById.get(taskId);
      if (!task) {
        return false;
      }
      const project = projectById.get(task.project_id);
      startFilterTransition(() => {
        if (project) {
          setSelectedGoalId(project.goal_id);
          setSelectedProjectId(project.project_id);
        }
        setSelectedTaskId(task.task_id);
      });
      return true;
    },
    [projectById, taskById]
  );

  const openBoardCardTask = useCallback(
    (boardCardId: string): boolean => {
      const task = taskByBoardCardId.get(boardCardId);
      return task ? openTaskById(task.task_id) : false;
    },
    [openTaskById, taskByBoardCardId]
  );

  const openJobTask = useCallback(
    (jobId: string): boolean => {
      const task = taskByJobId.get(jobId);
      return task ? openTaskById(task.task_id) : false;
    },
    [openTaskById, taskByJobId]
  );

  const applySummaryLens = useCallback(
    (lens: "blocked" | "stale" | "approvals" | "all") => {
      startFilterTransition(() => {
        if (lens === "all") {
          setTaskFilters(DEFAULT_TASK_FILTERS);
          return;
        }
        setTaskFilters((current) => ({
          ...DEFAULT_TASK_FILTERS,
          blocked: lens === "blocked",
          stale: lens === "stale",
          query: lens === "approvals" ? current.query : "",
        }));
      });
    },
    []
  );

  const runMutation = useCallback(
    async <T,>(
      label: string,
      mutation: () => Promise<T>,
      afterSuccess?: (result: T) => void
    ): Promise<T> => {
      setMutating(label);
      try {
        const result = await mutation();
        afterSuccess?.(result);
        await loadStrategyData(settings);
        return result;
      } finally {
        setMutating(null);
      }
    },
    [loadStrategyData, settings]
  );

  const describeTaskContext = useCallback(
    (taskId: string): StrategyTaskContextSnapshot | null => {
      const task = taskById.get(taskId);
      if (!task) {
        return null;
      }
      const project = projectById.get(task.project_id) ?? null;
      const goal = project ? goalById.get(project.goal_id) ?? null : null;
      const owner = task.owner_agent_id
        ? org.agentsById.get(task.owner_agent_id) ?? null
        : null;
      const managerChain = task.owner_agent_id
        ? org.managerChainByAgentId.get(task.owner_agent_id) ?? []
        : [];
      return {
        task,
        project,
        goal,
        owner,
        managerChain,
      };
    },
    [goalById, org.agentsById, org.managerChainByAgentId, projectById, taskById]
  );

  return {
    enabled,
    availability,
    availabilityMessage,
    loading: availability === "loading",
    isFilterTransitionPending,
    mutating,
    summary,
    goals,
    goalById,
    selectedGoalId,
    setSelectedGoalId: (goalId: string) =>
      startFilterTransition(() => {
        setSelectedGoalId(goalId);
        setSelectedProjectId("");
        setSelectedTaskId("");
      }),
    selectedGoal,
    projects,
    projectById,
    projectsForSelectedGoal,
    selectedProjectId,
    setSelectedProjectId: (projectId: string) =>
      startFilterTransition(() => {
        setSelectedProjectId(projectId);
        setSelectedTaskId("");
      }),
    selectedProject,
    tasks,
    taskById,
    taskByBoardCardId,
    taskByJobId,
    filteredTasks,
    selectedTaskId,
    setSelectedTaskId,
    selectedTask,
    taskFilters,
    updateFilters,
    resetFilters,
    presets,
    org,
    taskCountByProjectId,
    suggestedOwnerAgentIds,
    approvalTaskByApprovalId,
    describeTaskContext,
    queueRefresh,
    loadStrategyData,
    openTaskById,
    openBoardCardTask,
    openJobTask,
    applySummaryLens,
    createGoal: (
      payload: Parameters<typeof createGoal>[1]
    ) =>
      runMutation("create-goal", () => createGoal(settings, payload), (result) => {
        setSelectedGoalId(result.goal.goal_id);
      }),
    updateGoal: (goalId: string, payload: Parameters<typeof updateGoal>[2]) =>
      runMutation("update-goal", () => updateGoal(settings, goalId, payload)),
    createProject: (
      payload: Parameters<typeof createProject>[1]
    ) =>
      runMutation("create-project", () => createProject(settings, payload), (result) => {
        setSelectedGoalId(result.project.goal_id);
        setSelectedProjectId(result.project.project_id);
      }),
    updateProject: (projectId: string, payload: Parameters<typeof updateProject>[2]) =>
      runMutation("update-project", () => updateProject(settings, projectId, payload)),
    createTask: (
      payload: Parameters<typeof createTask>[1]
    ) =>
      runMutation("create-task", () => createTask(settings, payload), (result) => {
        const project = projectById.get(result.task.project_id);
        if (project) {
          setSelectedGoalId(project.goal_id);
          setSelectedProjectId(project.project_id);
        }
        setSelectedTaskId(result.task.task_id);
      }),
    updateTask: (taskId: string, payload: Parameters<typeof updateTask>[2]) =>
      runMutation("update-task", () => updateTask(settings, taskId, payload), (result) => {
        setSelectedTaskId(result.task.task_id);
      }),
    linkTaskBoardCard: (
      taskId: string,
      payload: Parameters<typeof linkTaskBoardCard>[2]
    ) =>
      runMutation("link-task-board-card", () => linkTaskBoardCard(settings, taskId, payload)),
    linkTaskJob: (taskId: string, payload: Parameters<typeof linkTaskJob>[2]) =>
      runMutation("link-task-job", () => linkTaskJob(settings, taskId, payload)),
    clearTaskLinks: (taskId: string, payload: Parameters<typeof clearTaskLinks>[2]) =>
      runMutation("clear-task-links", () => clearTaskLinks(settings, taskId, payload)),
    createBootstrapPreset: (
      payload: Parameters<typeof createBootstrapPreset>[1]
    ) =>
      runMutation("create-preset", () => createBootstrapPreset(settings, payload)),
    updateBootstrapPreset: (
      presetKey: string,
      payload: Parameters<typeof updateBootstrapPreset>[2]
    ) =>
      runMutation("update-preset", () => updateBootstrapPreset(settings, presetKey, payload)),
    exportBootstrapPreset: (presetKey: string) =>
      exportBootstrapPreset(settings, presetKey),
    importBootstrapPreset: (
      payload: Parameters<typeof importBootstrapPreset>[1]
    ) =>
      runMutation("import-preset", () => importBootstrapPreset(settings, payload)),
  };
}
