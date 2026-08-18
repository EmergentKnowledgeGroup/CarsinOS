CREATE TABLE IF NOT EXISTS connector_sources (
  connector_id TEXT PRIMARY KEY,
  slug TEXT NOT NULL,
  display_name TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  origin_kind TEXT NOT NULL,
  catalog_item_id TEXT,
  current_version_id TEXT,
  latest_imported_version_id TEXT,
  status TEXT NOT NULL,
  trust_state TEXT NOT NULL,
  last_conversion_at INTEGER,
  last_review_at INTEGER,
  last_enabled_at INTEGER,
  last_disabled_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_connector_sources_slug_ci
ON connector_sources(LOWER(slug));

CREATE INDEX IF NOT EXISTS idx_connector_sources_updated
ON connector_sources(updated_at DESC, connector_id ASC);

CREATE TABLE IF NOT EXISTS connector_versions (
  version_id TEXT PRIMARY KEY,
  connector_id TEXT NOT NULL REFERENCES connector_sources(connector_id),
  version_label TEXT NOT NULL,
  source_digest TEXT NOT NULL,
  raw_source_location TEXT,
  import_metadata_json TEXT NOT NULL,
  schema_summary_json TEXT NOT NULL,
  latest_conversion_id TEXT,
  external_reference_policy TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connector_versions_connector_created
ON connector_versions(connector_id, created_at DESC, version_id ASC);

CREATE TABLE IF NOT EXISTS connector_conversions (
  conversion_id TEXT PRIMARY KEY,
  connector_id TEXT NOT NULL REFERENCES connector_sources(connector_id),
  version_id TEXT NOT NULL REFERENCES connector_versions(version_id),
  status TEXT NOT NULL,
  warnings_json TEXT NOT NULL,
  proposed_tools_json TEXT NOT NULL,
  write_capable_tools INTEGER NOT NULL,
  unsupported_operations_json TEXT NOT NULL,
  normalization_notes_json TEXT NOT NULL,
  diff_from_previous_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connector_conversions_version_created
ON connector_conversions(version_id, created_at DESC, conversion_id ASC);

CREATE TABLE IF NOT EXISTS connector_published_tools (
  published_tool_id TEXT PRIMARY KEY,
  connector_id TEXT NOT NULL REFERENCES connector_sources(connector_id),
  version_id TEXT NOT NULL REFERENCES connector_versions(version_id),
  conversion_id TEXT NOT NULL REFERENCES connector_conversions(conversion_id),
  tool_name TEXT NOT NULL,
  display_name TEXT NOT NULL,
  tool_schema_json TEXT NOT NULL,
  origin_metadata_json TEXT NOT NULL,
  write_classification TEXT NOT NULL,
  published_at INTEGER NOT NULL,
  unpublished_at INTEGER,
  superseded_by_published_tool_id TEXT,
  deprecation_state TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connector_published_tools_connector_live
ON connector_published_tools(connector_id, version_id, published_at DESC, published_tool_id ASC);

CREATE INDEX IF NOT EXISTS idx_connector_published_tools_tool_name
ON connector_published_tools(tool_name, published_at DESC, published_tool_id ASC);

CREATE TABLE IF NOT EXISTS connector_assignments (
  assignment_id TEXT PRIMARY KEY,
  connector_id TEXT NOT NULL REFERENCES connector_sources(connector_id),
  agent_id TEXT NOT NULL REFERENCES agents(agent_id),
  enabled INTEGER NOT NULL,
  auth_mode TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_connector_assignments_connector_agent
ON connector_assignments(connector_id, agent_id);

CREATE TABLE IF NOT EXISTS connector_auth_bindings (
  auth_binding_id TEXT PRIMARY KEY,
  connector_id TEXT NOT NULL REFERENCES connector_sources(connector_id),
  agent_id TEXT REFERENCES agents(agent_id),
  auth_kind TEXT NOT NULL,
  secret_ref TEXT,
  oauth_session_id TEXT,
  status TEXT NOT NULL,
  auth_metadata_json TEXT NOT NULL,
  last_success_at INTEGER,
  last_error TEXT,
  last_rotated_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connector_auth_bindings_connector
ON connector_auth_bindings(connector_id, updated_at DESC, auth_binding_id ASC);

CREATE TABLE IF NOT EXISTS connector_interactions (
  interaction_id TEXT PRIMARY KEY,
  connector_id TEXT NOT NULL REFERENCES connector_sources(connector_id),
  agent_id TEXT REFERENCES agents(agent_id),
  interaction_kind TEXT NOT NULL,
  status TEXT NOT NULL,
  prompt_summary TEXT NOT NULL,
  resume_token TEXT,
  expires_at INTEGER,
  consumed_at INTEGER,
  detail_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_connector_interactions_connector
ON connector_interactions(connector_id, updated_at DESC, interaction_id ASC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_connector_interactions_resume_token
ON connector_interactions(resume_token)
WHERE resume_token IS NOT NULL;
