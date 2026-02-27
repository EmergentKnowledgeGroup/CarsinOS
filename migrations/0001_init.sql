PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_kv (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agents (
  agent_id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  workspace_root TEXT NOT NULL,
  model_provider TEXT NOT NULL,
  model_id TEXT NOT NULL,
  tool_profile TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS auth_profiles (
  auth_profile_id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  display_name TEXT NOT NULL,
  auth_mode TEXT NOT NULL,
  risk_level TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  kill_switch_scope TEXT NOT NULL,
  api_base_url TEXT,
  credentials_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_profiles_provider_name
ON auth_profiles(provider, display_name);

CREATE TABLE IF NOT EXISTS agent_provider_profile_order (
  agent_id TEXT NOT NULL REFERENCES agents(agent_id),
  provider TEXT NOT NULL,
  auth_profile_id TEXT NOT NULL REFERENCES auth_profiles(auth_profile_id),
  priority INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (agent_id, provider, auth_profile_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_provider_profile_order_priority
ON agent_provider_profile_order(agent_id, provider, priority);

CREATE TABLE IF NOT EXISTS routing_rules (
  rule_id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL,
  channel TEXT NOT NULL,
  channel_account TEXT,
  peer_id TEXT,
  conversation_id TEXT,
  agent_id TEXT NOT NULL REFERENCES agents(agent_id),
  session_scope TEXT NOT NULL,
  require_mention INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
  session_id TEXT PRIMARY KEY,
  session_key TEXT NOT NULL UNIQUE,
  agent_id TEXT NOT NULL REFERENCES agents(agent_id),
  title TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  closed_at INTEGER
);

CREATE TABLE IF NOT EXISTS messages (
  message_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(session_id),
  source_channel TEXT NOT NULL,
  source_peer_id TEXT,
  source_message_id TEXT,
  role TEXT NOT NULL,
  content_text TEXT NOT NULL,
  content_format TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_session_time
ON messages(session_id, created_at);

CREATE TABLE IF NOT EXISTS attachments (
  attachment_id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES messages(message_id),
  kind TEXT NOT NULL,
  mime TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  local_path TEXT NOT NULL,
  width INTEGER,
  height INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
  run_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(session_id),
  status TEXT NOT NULL,
  model_provider TEXT NOT NULL,
  model_id TEXT NOT NULL,
  started_at INTEGER,
  ended_at INTEGER,
  error_text TEXT,
  usage_json TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_session_time
ON runs(session_id, created_at);

CREATE TABLE IF NOT EXISTS tool_calls (
  tool_call_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(run_id),
  tool_name TEXT NOT NULL,
  args_json TEXT NOT NULL,
  started_at INTEGER,
  ended_at INTEGER,
  status TEXT NOT NULL,
  result_json TEXT,
  error_text TEXT
);

CREATE TABLE IF NOT EXISTS approvals (
  approval_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(run_id),
  tool_call_id TEXT NOT NULL REFERENCES tool_calls(tool_call_id),
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  request_summary TEXT NOT NULL,
  request_json TEXT NOT NULL,
  requested_at INTEGER NOT NULL,
  decided_at INTEGER,
  decided_via TEXT,
  decided_by_peer_id TEXT
);

CREATE TABLE IF NOT EXISTS security_audit_events (
  event_id TEXT PRIMARY KEY,
  request_id TEXT NOT NULL,
  correlation_id TEXT NOT NULL,
  principal TEXT NOT NULL,
  action TEXT NOT NULL,
  resource TEXT NOT NULL,
  decision TEXT NOT NULL,
  reason TEXT,
  transport TEXT NOT NULL,
  status TEXT NOT NULL,
  error_code TEXT,
  session_id TEXT,
  run_id TEXT,
  metadata_json TEXT,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS security_audit_events_archive (
  event_id TEXT PRIMARY KEY,
  request_id TEXT NOT NULL,
  correlation_id TEXT NOT NULL,
  principal TEXT NOT NULL,
  action TEXT NOT NULL,
  resource TEXT NOT NULL,
  decision TEXT NOT NULL,
  reason TEXT,
  transport TEXT NOT NULL,
  status TEXT NOT NULL,
  error_code TEXT,
  session_id TEXT,
  run_id TEXT,
  metadata_json TEXT,
  created_at INTEGER NOT NULL,
  archived_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_security_audit_events_created
ON security_audit_events(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_security_audit_events_request
ON security_audit_events(request_id);

CREATE INDEX IF NOT EXISTS idx_security_audit_events_principal_action
ON security_audit_events(principal, action, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_security_audit_events_archive_created
ON security_audit_events_archive(created_at DESC);

CREATE TABLE IF NOT EXISTS jobs (
  job_id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES agents(agent_id),
  name TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  schedule_kind TEXT NOT NULL,
  interval_seconds INTEGER,
  run_at_ms INTEGER,
  next_run_at INTEGER,
  payload_json TEXT NOT NULL,
  max_retries INTEGER NOT NULL,
  retry_backoff_ms INTEGER NOT NULL,
  timeout_ms INTEGER NOT NULL,
  lease_owner TEXT,
  lease_expires_at INTEGER,
  last_run_at INTEGER,
  last_error TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_jobs_due
ON jobs(enabled, next_run_at, lease_expires_at);

CREATE INDEX IF NOT EXISTS idx_jobs_agent
ON jobs(agent_id, updated_at);

CREATE TABLE IF NOT EXISTS job_runs (
  job_run_id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES jobs(job_id),
  trigger_kind TEXT NOT NULL,
  status TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  started_at INTEGER,
  ended_at INTEGER,
  error_text TEXT,
  output_json TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_job_runs_job_time
ON job_runs(job_id, created_at);

CREATE TABLE IF NOT EXISTS notes (
  note_id TEXT PRIMARY KEY,
  title TEXT,
  body TEXT NOT NULL,
  tags_json TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS embeddings (
  embedding_id TEXT PRIMARY KEY,
  source_kind TEXT NOT NULL,
  source_id TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  model TEXT NOT NULL,
  dims INTEGER NOT NULL,
  vec BLOB NOT NULL,
  text TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_embeddings_source
ON embeddings(source_kind, source_id);

CREATE TABLE IF NOT EXISTS daily_auth_profile_usage (
  usage_day_utc TEXT NOT NULL,
  auth_profile_id TEXT NOT NULL REFERENCES auth_profiles(auth_profile_id),
  provider TEXT NOT NULL,
  input_chars INTEGER NOT NULL,
  output_chars INTEGER NOT NULL,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  total_tokens INTEGER NOT NULL,
  estimated_cost_usd REAL NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (usage_day_utc, auth_profile_id)
);

CREATE INDEX IF NOT EXISTS idx_daily_auth_profile_usage_provider_day
ON daily_auth_profile_usage(provider, usage_day_utc, updated_at DESC);

CREATE TABLE IF NOT EXISTS circuit_breaker_states (
  breaker_key TEXT PRIMARY KEY,
  scope TEXT NOT NULL,
  target_id TEXT NOT NULL,
  state TEXT NOT NULL,
  consecutive_failures INTEGER NOT NULL,
  opened_at INTEGER,
  cooldown_until INTEGER,
  last_error_code TEXT,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_circuit_breaker_scope_target
ON circuit_breaker_states(scope, target_id, updated_at DESC);
