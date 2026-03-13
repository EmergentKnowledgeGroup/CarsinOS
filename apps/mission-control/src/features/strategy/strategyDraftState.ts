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

function serializeRecord<T extends object>(record: T): string {
  return JSON.stringify(record);
}

export function isGoalDraftDirty(
  current: GoalDraftLike,
  baseline: GoalDraftLike
): boolean {
  return serializeRecord(current) !== serializeRecord(baseline);
}

export function isProjectDraftDirty(
  current: ProjectDraftLike,
  baseline: ProjectDraftLike
): boolean {
  return serializeRecord(current) !== serializeRecord(baseline);
}

export function isTaskDraftDirty(
  current: TaskDraftLike,
  baseline: TaskDraftLike
): boolean {
  return serializeRecord(current) !== serializeRecord(baseline);
}
