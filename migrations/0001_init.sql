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

CREATE TABLE IF NOT EXISTS boards (
  board_id TEXT PRIMARY KEY,
  board_key TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  board_type TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  archived_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_boards_type_updated
ON boards(board_type, updated_at DESC);

CREATE TABLE IF NOT EXISTS board_columns (
  column_id TEXT PRIMARY KEY,
  board_id TEXT NOT NULL REFERENCES boards(board_id),
  column_key TEXT NOT NULL,
  name TEXT NOT NULL,
  position INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  archived_at INTEGER,
  UNIQUE(board_id, column_key),
  UNIQUE(board_id, column_id)
);

CREATE INDEX IF NOT EXISTS idx_board_columns_position
ON board_columns(board_id, position, updated_at DESC);

CREATE TABLE IF NOT EXISTS board_cards (
  card_id TEXT PRIMARY KEY,
  board_id TEXT NOT NULL REFERENCES boards(board_id),
  column_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  owner_kind TEXT NOT NULL,
  owner_agent_id TEXT,
  owner_human_id TEXT,
  due_at INTEGER,
  tags_json TEXT,
  script_markdown TEXT,
  linked_session_id TEXT,
  latest_run_id TEXT,
  position INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  archived_at INTEGER,
  FOREIGN KEY (board_id, column_id) REFERENCES board_columns(board_id, column_id)
);

CREATE INDEX IF NOT EXISTS idx_board_cards_column_position
ON board_cards(column_id, position, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_board_cards_board_updated
ON board_cards(board_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_board_cards_board_column_position
ON board_cards(board_id, column_id, position, updated_at DESC);

CREATE TABLE IF NOT EXISTS board_card_assets (
  card_asset_id TEXT PRIMARY KEY,
  card_id TEXT NOT NULL REFERENCES board_cards(card_id),
  filename TEXT NOT NULL,
  mime TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  local_path TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_board_card_assets_card
ON board_card_assets(card_id, created_at DESC);

CREATE TABLE IF NOT EXISTS agent_mail_threads (
  thread_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  subject TEXT NOT NULL,
  created_by_principal TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  archived_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_agent_mail_threads_kind_updated
ON agent_mail_threads(kind, updated_at DESC);

CREATE TABLE IF NOT EXISTS agent_mail_thread_participants (
  thread_id TEXT NOT NULL REFERENCES agent_mail_threads(thread_id),
  principal_id TEXT NOT NULL,
  role TEXT NOT NULL,
  joined_at INTEGER NOT NULL,
  last_read_at INTEGER,
  muted INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (thread_id, principal_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_mail_participants_principal
ON agent_mail_thread_participants(principal_id, thread_id);

CREATE TABLE IF NOT EXISTS agent_mail_messages (
  message_id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL REFERENCES agent_mail_threads(thread_id),
  sender_principal TEXT NOT NULL,
  sender_kind TEXT NOT NULL,
  body_text TEXT NOT NULL,
  metadata_json TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_mail_messages_thread_time
ON agent_mail_messages(thread_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_agent_mail_messages_sender_time
ON agent_mail_messages(sender_principal, created_at DESC);

CREATE TABLE IF NOT EXISTS agent_mail_message_recipients (
  message_id TEXT NOT NULL REFERENCES agent_mail_messages(message_id),
  recipient_principal TEXT NOT NULL,
  delivered_at INTEGER NOT NULL,
  acked_at INTEGER,
  PRIMARY KEY (message_id, recipient_principal)
);

CREATE INDEX IF NOT EXISTS idx_agent_mail_recipients_principal_time
ON agent_mail_message_recipients(recipient_principal, delivered_at DESC);

CREATE TABLE IF NOT EXISTS agent_mail_attachments (
  attachment_id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES agent_mail_messages(message_id),
  filename TEXT NOT NULL,
  mime TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  local_path TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_mail_attachments_message
ON agent_mail_attachments(message_id, created_at DESC);

CREATE VIRTUAL TABLE IF NOT EXISTS agent_mail_messages_fts
USING fts5(
  thread_id UNINDEXED,
  sender_principal UNINDEXED,
  body_text,
  content='agent_mail_messages',
  content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS agent_mail_messages_ai AFTER INSERT ON agent_mail_messages BEGIN
  INSERT INTO agent_mail_messages_fts(rowid, thread_id, sender_principal, body_text)
  VALUES (new.rowid, new.thread_id, new.sender_principal, new.body_text);
END;

CREATE TRIGGER IF NOT EXISTS agent_mail_messages_ad AFTER DELETE ON agent_mail_messages BEGIN
  INSERT INTO agent_mail_messages_fts(agent_mail_messages_fts, rowid, thread_id, sender_principal, body_text)
  VALUES ('delete', old.rowid, old.thread_id, old.sender_principal, old.body_text);
END;

CREATE TRIGGER IF NOT EXISTS agent_mail_messages_au AFTER UPDATE ON agent_mail_messages BEGIN
  INSERT INTO agent_mail_messages_fts(agent_mail_messages_fts, rowid, thread_id, sender_principal, body_text)
  VALUES ('delete', old.rowid, old.thread_id, old.sender_principal, old.body_text);
  INSERT INTO agent_mail_messages_fts(rowid, thread_id, sender_principal, body_text)
  VALUES (new.rowid, new.thread_id, new.sender_principal, new.body_text);
END;

CREATE TABLE IF NOT EXISTS agent_mail_file_leases (
  lease_id TEXT PRIMARY KEY,
  holder_principal TEXT NOT NULL,
  glob_pattern TEXT NOT NULL,
  exclusive INTEGER NOT NULL,
  ttl_ms INTEGER NOT NULL,
  note TEXT,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  released_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_agent_mail_file_leases_active
ON agent_mail_file_leases(released_at, expires_at, holder_principal);
