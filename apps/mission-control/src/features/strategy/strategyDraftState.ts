interface GoalDraftLike {
  slug: string;
  title: string;
  summary: string;
  status: string;
  owner_agent_id: string;
  target_date: string;
}

interface ProjectDraftLike {
  goal_id: string;
  slug: string;
  name: string;
  summary: string;
  status: string;
  owner_agent_id: string;
  workspace_root: string;
  budget_month_usd: string;
}

interface TaskDraftLike {
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

function areFieldsEqual<T extends object>(
  current: T,
  baseline: T,
  fields: readonly (keyof T)[]
): boolean {
  return fields.every((field) => current[field] === baseline[field]);
}

const GOAL_DRAFT_FIELDS = [
  "slug",
  "title",
  "summary",
  "status",
  "owner_agent_id",
  "target_date",
] as const satisfies readonly (keyof GoalDraftLike)[];

const PROJECT_DRAFT_FIELDS = [
  "goal_id",
  "slug",
  "name",
  "summary",
  "status",
  "owner_agent_id",
  "workspace_root",
  "budget_month_usd",
] as const satisfies readonly (keyof ProjectDraftLike)[];

const TASK_DRAFT_FIELDS = [
  "task_id",
  "project_id",
  "parent_task_id",
  "title",
  "detail",
  "status",
  "priority",
  "owner_agent_id",
  "due_at",
  "blocked_reason",
  "linked_board_card_id",
  "linked_job_id",
] as const satisfies readonly (keyof TaskDraftLike)[];

export function isGoalDraftDirty(
  current: GoalDraftLike,
  baseline: GoalDraftLike
): boolean {
  return !areFieldsEqual(current, baseline, GOAL_DRAFT_FIELDS);
}

export function isProjectDraftDirty(
  current: ProjectDraftLike,
  baseline: ProjectDraftLike
): boolean {
  return !areFieldsEqual(current, baseline, PROJECT_DRAFT_FIELDS);
}

export function isTaskDraftDirty(
  current: TaskDraftLike,
  baseline: TaskDraftLike
): boolean {
  return !areFieldsEqual(current, baseline, TASK_DRAFT_FIELDS);
}
