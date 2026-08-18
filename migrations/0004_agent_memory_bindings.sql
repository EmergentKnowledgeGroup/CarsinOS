ALTER TABLE agents
  ADD COLUMN memory_binding_id TEXT;

ALTER TABLE agents
  ADD COLUMN memory_provider_kind TEXT;

ALTER TABLE agents
  ADD COLUMN memory_base_url TEXT;

ALTER TABLE agents
  ADD COLUMN memory_auth_mode TEXT;

ALTER TABLE agents
  ADD COLUMN memory_auth_secret_ref TEXT;

ALTER TABLE agents
  ADD COLUMN memory_principal_id TEXT;

ALTER TABLE agents
  ADD COLUMN memory_principal_display_name TEXT;

ALTER TABLE agents
  ADD COLUMN memory_enabled INTEGER NOT NULL DEFAULT 0;

ALTER TABLE agents
  ADD COLUMN memory_trusted_local_operator_actions INTEGER NOT NULL DEFAULT 0;
