ALTER TABLE agents ADD COLUMN reports_to_agent_id TEXT REFERENCES agents(agent_id);

ALTER TABLE agents ADD COLUMN role_label TEXT;

CREATE TABLE bootstrap_presets_rebuild (
  preset_key TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  role_label TEXT NOT NULL,
  provider_path TEXT NOT NULL,
  default_model_provider TEXT,
  default_model_id TEXT,
  default_tool_profile TEXT,
  default_workspace_root TEXT,
  default_reports_to_agent_id TEXT,
  setup_notes TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

INSERT INTO bootstrap_presets_rebuild (
  preset_key,
  display_name,
  description,
  role_label,
  provider_path,
  default_model_provider,
  default_model_id,
  default_tool_profile,
  default_workspace_root,
  default_reports_to_agent_id,
  setup_notes,
  created_at,
  updated_at
)
SELECT
  preset_key,
  display_name,
  description,
  role_label,
  provider_path,
  default_model_provider,
  default_model_id,
  default_tool_profile,
  default_workspace_root,
  default_reports_to_agent_id,
  setup_notes,
  created_at,
  updated_at
FROM bootstrap_presets;

DROP TABLE bootstrap_presets;

ALTER TABLE bootstrap_presets_rebuild RENAME TO bootstrap_presets;

CREATE INDEX IF NOT EXISTS idx_bootstrap_presets_updated
ON bootstrap_presets(updated_at DESC, preset_key ASC);
