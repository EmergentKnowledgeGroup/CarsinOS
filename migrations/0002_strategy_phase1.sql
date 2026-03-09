CREATE TABLE IF NOT EXISTS goals (
  goal_id TEXT PRIMARY KEY,
  slug TEXT NOT NULL,
  title TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  owner_agent_id TEXT REFERENCES agents(agent_id),
  target_date INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_goals_slug_ci
ON goals(LOWER(slug));

CREATE INDEX IF NOT EXISTS idx_goals_updated
ON goals(updated_at DESC, goal_id ASC);

CREATE TABLE IF NOT EXISTS projects (
  project_id TEXT PRIMARY KEY,
  goal_id TEXT NOT NULL REFERENCES goals(goal_id),
  slug TEXT NOT NULL,
  name TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  owner_agent_id TEXT REFERENCES agents(agent_id),
  workspace_root TEXT,
  budget_month_usd REAL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_goal_updated
ON projects(goal_id, updated_at DESC, project_id ASC);

CREATE INDEX IF NOT EXISTS idx_projects_updated
ON projects(updated_at DESC, project_id ASC);

CREATE TABLE IF NOT EXISTS tasks (
  task_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(project_id),
  parent_task_id TEXT REFERENCES tasks(task_id),
  title TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  priority TEXT NOT NULL,
  owner_agent_id TEXT REFERENCES agents(agent_id),
  due_at INTEGER,
  blocked_reason TEXT,
  linked_board_card_id TEXT REFERENCES board_cards(card_id),
  linked_job_id TEXT REFERENCES jobs(job_id),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_project_updated
ON tasks(project_id, updated_at DESC, task_id ASC);

CREATE INDEX IF NOT EXISTS idx_tasks_status_updated
ON tasks(status, updated_at DESC, task_id ASC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_linked_board_card_unique
ON tasks(linked_board_card_id)
WHERE linked_board_card_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_linked_job_unique
ON tasks(linked_job_id)
WHERE linked_job_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS bootstrap_presets (
  preset_key TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  role_label TEXT NOT NULL,
  provider_path TEXT NOT NULL,
  default_model_provider TEXT,
  default_model_id TEXT,
  default_tool_profile TEXT,
  default_workspace_root TEXT,
  default_reports_to_agent_id TEXT REFERENCES agents(agent_id),
  setup_notes TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_bootstrap_presets_updated
ON bootstrap_presets(updated_at DESC, preset_key ASC);
