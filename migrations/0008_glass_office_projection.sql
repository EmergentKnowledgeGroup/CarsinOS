-- Additive Glass Office projections. These tables are intentionally outside
-- the ExecAss authority namespace: they are a privacy-safe renderer, not a
-- new control plane or event contract.
CREATE TABLE office_chatter_workstreams (
  delegation_id TEXT PRIMARY KEY REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  thread_id TEXT NOT NULL UNIQUE REFERENCES agent_mail_threads(thread_id) ON DELETE RESTRICT,
  safe_label TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE office_chatter_messages (
  message_id TEXT PRIMARY KEY REFERENCES agent_mail_messages(message_id) ON DELETE RESTRICT,
  thread_id TEXT NOT NULL REFERENCES agent_mail_threads(thread_id) ON DELETE RESTRICT,
  source_event_id TEXT UNIQUE REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  source_kind TEXT NOT NULL CHECK (source_kind IN ('execass_event', 'owner_message')),
  delegation_id TEXT REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  event_name TEXT,
  revision INTEGER,
  created_at INTEGER NOT NULL,
  CHECK (
    (source_kind = 'execass_event' AND source_event_id IS NOT NULL AND delegation_id IS NOT NULL
      AND event_name IS NOT NULL AND revision IS NOT NULL)
    OR (source_kind = 'owner_message' AND source_event_id IS NULL AND delegation_id IS NOT NULL
      AND event_name IS NULL AND revision IS NULL)
  )
);

CREATE INDEX idx_office_chatter_messages_thread_created
ON office_chatter_messages(thread_id, created_at, message_id);

CREATE TABLE office_chatter_producer_cursor (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  last_global_sequence INTEGER NOT NULL CHECK (last_global_sequence >= 0),
  updated_at INTEGER NOT NULL
);

INSERT INTO office_chatter_producer_cursor (singleton, last_global_sequence, updated_at)
SELECT 1, COALESCE(MAX(global_sequence), 0), strftime('%s', 'now') * 1000
FROM execass_outbox_events;

-- v8 preserves the authority contract while making the exact schema identity
-- explicit. Rebuild this table because v7 constrained schema_version to 7.
ALTER TABLE execass_schema_metadata RENAME TO execass_schema_metadata_v7;
CREATE TABLE execass_schema_metadata (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  application_id INTEGER NOT NULL CHECK (application_id = 1163411761),
  schema_version INTEGER NOT NULL CHECK (schema_version = 8),
  contract_id TEXT NOT NULL CHECK (contract_id = 'carsinos.execass.contract'),
  contract_version TEXT NOT NULL CHECK (contract_version = 'v1'),
  installed_at INTEGER NOT NULL
);
INSERT INTO execass_schema_metadata (
  singleton, application_id, schema_version, contract_id, contract_version, installed_at
)
SELECT singleton, application_id, 8, contract_id, contract_version, installed_at
FROM execass_schema_metadata_v7;
DROP TABLE execass_schema_metadata_v7;
PRAGMA user_version = 8;
