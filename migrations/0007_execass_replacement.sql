-- ExecAss v1 is an incompatible clean-root schema. This migration is only
-- installed by init_execass_fresh_root; the legacy migration path must not
-- apply it to an existing CarsinOS database.
PRAGMA application_id = 1163411761; -- "EXA1"
PRAGMA user_version = 7;

-- Retire the old orchestration decision authority on the clean root. The
-- audit ledger remains authoritative and is referenced below, but no ExecAss
-- record can depend on the retired worker-coordination tables.
DROP TABLE assistant_task_links;
DROP TABLE assistant_workers;
DROP TABLE approvals;

CREATE TABLE execass_schema_metadata (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  application_id INTEGER NOT NULL CHECK (application_id = 1163411761),
  schema_version INTEGER NOT NULL CHECK (schema_version = 7),
  contract_id TEXT NOT NULL CHECK (contract_id = 'carsinos.execass.contract'),
  contract_version TEXT NOT NULL CHECK (contract_version = 'v1'),
  installed_at INTEGER NOT NULL
);

INSERT INTO execass_schema_metadata (
  singleton, application_id, schema_version, contract_id, contract_version, installed_at
) VALUES (1, 1163411761, 7, 'carsinos.execass.contract', 'v1', strftime('%s', 'now') * 1000);

CREATE TABLE execass_authority_provenance (
  authority_provenance_id TEXT PRIMARY KEY,
  actor_type TEXT NOT NULL CHECK (actor_type IN (
    'human_local', 'human_remote', 'runtime', 'worker', 'connector', 'model'
  )),
  credential_identity TEXT NOT NULL,
  authenticated_ingress TEXT NOT NULL,
  channel_assurance TEXT NOT NULL,
  source_correlation_id TEXT NOT NULL,
  source_message_id TEXT,
  authority_kind TEXT NOT NULL CHECK (authority_kind IN (
    'original_request', 'decision_resolution', 'action_specific_owner_amendment',
    'policy_snapshot', 'runtime_settings_snapshot', 'run_control_attestation', 'runtime_safety_state'
  )),
  normalized_scope_json TEXT NOT NULL CHECK (json_valid(normalized_scope_json)),
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  bound_decision_id TEXT,
  bound_decision_revision INTEGER,
  bound_manifest_digest TEXT,
  bound_challenge_nonce_digest TEXT,
  evidence_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER,
  CHECK (expires_at IS NULL OR expires_at > created_at),
  CHECK (
    (authority_kind = 'original_request' AND actor_type IN ('human_local', 'human_remote')) OR
    (authority_kind = 'decision_resolution' AND actor_type IN ('human_local', 'human_remote')) OR
    (authority_kind = 'action_specific_owner_amendment' AND actor_type IN ('human_local', 'human_remote')) OR
    (authority_kind = 'policy_snapshot' AND actor_type IN ('human_local', 'human_remote')) OR
    (authority_kind = 'runtime_settings_snapshot' AND actor_type IN ('human_local', 'human_remote')) OR
    (authority_kind = 'run_control_attestation' AND actor_type IN ('human_local', 'human_remote')) OR
    (authority_kind = 'runtime_safety_state' AND actor_type = 'runtime')
  ),
  CHECK (
    (authority_kind = 'decision_resolution' AND
     bound_decision_id IS NOT NULL AND bound_decision_revision IS NOT NULL AND
     bound_manifest_digest IS NOT NULL AND bound_challenge_nonce_digest IS NOT NULL) OR
    (authority_kind = 'action_specific_owner_amendment' AND (
      (bound_decision_id IS NOT NULL AND bound_decision_revision IS NOT NULL AND
       bound_manifest_digest IS NOT NULL AND bound_challenge_nonce_digest IS NOT NULL) OR
      (bound_decision_id IS NULL AND bound_decision_revision IS NULL AND
       bound_manifest_digest IS NULL AND bound_challenge_nonce_digest IS NULL AND
       COALESCE(json_type(normalized_scope_json, '$.delegation_id') = 'text', 0) AND
       COALESCE(length(trim(json_extract(normalized_scope_json, '$.delegation_id'))) > 0, 0) AND
       COALESCE(json_type(normalized_scope_json, '$.delegation_revision') = 'integer', 0) AND
       COALESCE(json_extract(normalized_scope_json, '$.delegation_revision') > 0, 0) AND
       COALESCE(json_type(normalized_scope_json, '$.plan_revision') = 'integer', 0) AND
       COALESCE(json_extract(normalized_scope_json, '$.plan_revision') > 0, 0))
    )) OR
    (authority_kind NOT IN ('decision_resolution', 'action_specific_owner_amendment') AND
     bound_decision_id IS NULL AND bound_decision_revision IS NULL AND
     bound_manifest_digest IS NULL AND bound_challenge_nonce_digest IS NULL)
  ),
  CHECK (bound_decision_revision IS NULL OR bound_decision_revision > 0),
  UNIQUE (authenticated_ingress, credential_identity, source_correlation_id, authority_kind),
  UNIQUE (authority_kind, bound_decision_id)
);

CREATE TABLE execass_delegations (
  delegation_id TEXT PRIMARY KEY,
  normalized_original_intent TEXT NOT NULL,
  intake_evidence_json TEXT NOT NULL CHECK (json_valid(intake_evidence_json)),
  ingress_source TEXT NOT NULL,
  ingress_credential_identity TEXT NOT NULL,
  source_message_id TEXT,
  source_correlation_id TEXT NOT NULL,
  ingress_idempotency_key TEXT NOT NULL,
  classifier_version TEXT NOT NULL,
  classifier_reasons_json TEXT NOT NULL CHECK (json_valid(classifier_reasons_json)),
  phase TEXT NOT NULL CHECK (phase IN (
    'accepted', 'planning', 'in_motion', 'waiting_for_user', 'waiting_external',
    'recovering', 'completed', 'partially_completed', 'failed'
  )),
  run_control TEXT NOT NULL CHECK (run_control IN ('running', 'stop_requested', 'stopped')),
  state_revision INTEGER NOT NULL CHECK (state_revision > 0),
  current_plan_revision INTEGER,
  current_criteria_revision INTEGER,
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  effective_authority_json TEXT NOT NULL CHECK (json_valid(effective_authority_json)),
  authority_provenance_id TEXT NOT NULL REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  pending_decision_id TEXT,
  external_wait_json TEXT CHECK (external_wait_json IS NULL OR json_valid(external_wait_json)),
  stop_epoch INTEGER NOT NULL DEFAULT 0 CHECK (stop_epoch >= 0),
  completion_assessment_json TEXT CHECK (
    completion_assessment_json IS NULL OR json_valid(completion_assessment_json)
  ),
  receipt_chain_count INTEGER NOT NULL DEFAULT 0 CHECK (receipt_chain_count >= 0),
  receipt_chain_head_digest TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  acknowledged_at INTEGER,
  terminal_at INTEGER,
  CHECK (updated_at >= created_at),
  CHECK ((phase IN ('completed', 'partially_completed', 'failed')) = (terminal_at IS NOT NULL)),
  CHECK (current_plan_revision IS NULL OR current_plan_revision > 0),
  CHECK (current_criteria_revision IS NULL OR current_criteria_revision > 0),
  UNIQUE (ingress_source, ingress_credential_identity, ingress_idempotency_key),
  UNIQUE (delegation_id, state_revision),
  FOREIGN KEY (delegation_id, current_plan_revision)
    REFERENCES execass_plans(delegation_id, plan_revision) ON DELETE RESTRICT,
  FOREIGN KEY (delegation_id, current_criteria_revision)
    REFERENCES execass_criteria_sets(delegation_id, criteria_revision) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (pending_decision_id)
    REFERENCES execass_decisions(decision_id) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_delegations_summary
ON execass_delegations(phase, run_control, updated_at DESC, delegation_id);

CREATE INDEX idx_execass_delegations_source_message
ON execass_delegations(ingress_source, source_message_id)
WHERE source_message_id IS NOT NULL;

-- Provider reply identifiers become attachment evidence only through this
-- append-only, owner-scoped binding. A reply boolean or message text can never
-- select a delegation. Provider, ingress, owner, conversation, and outbound
-- message identity together identify at most one delegation.
CREATE TABLE execass_channel_reply_bindings (
  binding_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  provider TEXT NOT NULL CHECK (provider IN ('telegram', 'discord')),
  authenticated_ingress TEXT NOT NULL CHECK (length(trim(authenticated_ingress)) > 0),
  owner_credential_identity TEXT NOT NULL CHECK (length(trim(owner_credential_identity)) > 0),
  conversation_id TEXT NOT NULL CHECK (length(trim(conversation_id)) > 0),
  outbound_message_id TEXT NOT NULL CHECK (length(trim(outbound_message_id)) > 0),
  created_at INTEGER NOT NULL CHECK (created_at > 0),
  UNIQUE (
    provider, authenticated_ingress, owner_credential_identity,
    conversation_id, outbound_message_id
  )
);

CREATE TABLE execass_plans (
  plan_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  plan_revision INTEGER NOT NULL CHECK (plan_revision > 0),
  based_on_delegation_revision INTEGER NOT NULL CHECK (based_on_delegation_revision > 0),
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  plan_summary TEXT NOT NULL,
  resolved_leaf_manifest_json TEXT NOT NULL CHECK (json_valid(resolved_leaf_manifest_json)),
  manifest_digest TEXT NOT NULL,
  created_by_authority_provenance_id TEXT NOT NULL REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  created_at INTEGER NOT NULL,
  UNIQUE (delegation_id, plan_revision),
  UNIQUE (delegation_id, manifest_digest)
);

CREATE TABLE execass_plan_amendments (
  amendment_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  amendment_revision INTEGER NOT NULL CHECK (amendment_revision > 0),
  superseded_plan_revision INTEGER NOT NULL CHECK (superseded_plan_revision > 0),
  resulting_plan_revision INTEGER NOT NULL CHECK (resulting_plan_revision > superseded_plan_revision),
  normalized_amendment TEXT NOT NULL,
  intake_evidence_json TEXT NOT NULL CHECK (json_valid(intake_evidence_json)),
  authority_provenance_id TEXT NOT NULL REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  created_at INTEGER NOT NULL,
  UNIQUE (delegation_id, amendment_revision),
  UNIQUE (delegation_id, resulting_plan_revision),
  FOREIGN KEY (delegation_id, superseded_plan_revision)
    REFERENCES execass_plans(delegation_id, plan_revision) ON DELETE RESTRICT,
  FOREIGN KEY (delegation_id, resulting_plan_revision)
    REFERENCES execass_plans(delegation_id, plan_revision) ON DELETE RESTRICT
);

CREATE TABLE execass_outcome_criteria (
  criterion_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  criteria_revision INTEGER NOT NULL CHECK (criteria_revision > 0),
  criterion_key TEXT NOT NULL,
  description TEXT NOT NULL,
  material INTEGER NOT NULL CHECK (material IN (0, 1)),
  verifier_type TEXT NOT NULL CHECK (verifier_type IN (
    'artifact', 'authoritative_state', 'provider_state', 'delivery', 'process_exit',
    'database_predicate', 'human_bound_supersession'
  )),
  expected_predicate_json TEXT NOT NULL CHECK (json_valid(expected_predicate_json)),
  authoritative_source_kind TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (delegation_id, criteria_revision, criterion_key),
  UNIQUE (delegation_id, criterion_id),
  FOREIGN KEY (delegation_id, criteria_revision)
    REFERENCES execass_criteria_sets(delegation_id, criteria_revision)
    ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE execass_verifier_results (
  verifier_result_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL,
  criterion_id TEXT NOT NULL,
  result_revision INTEGER NOT NULL CHECK (result_revision > 0),
  result TEXT NOT NULL CHECK (result IN ('pass', 'fail', 'unknown')),
  evidence_refs_json TEXT NOT NULL CHECK (json_valid(evidence_refs_json)),
  evidence_digest TEXT NOT NULL,
  verifier_identity TEXT NOT NULL,
  verified_at INTEGER NOT NULL,
  UNIQUE (criterion_id, result_revision),
  FOREIGN KEY (delegation_id, criterion_id)
    REFERENCES execass_outcome_criteria(delegation_id, criterion_id) ON DELETE RESTRICT
);

CREATE TABLE execass_criteria_sets (
  criteria_set_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  criteria_revision INTEGER NOT NULL CHECK (criteria_revision > 0),
  parent_criteria_revision INTEGER,
  disposition TEXT NOT NULL CHECK (disposition IN ('genesis','current','superseded')),
  created_at INTEGER NOT NULL,
  UNIQUE (delegation_id, criteria_revision),
  CHECK (parent_criteria_revision IS NOT NULL OR disposition IN ('genesis','superseded')),
  CHECK (parent_criteria_revision IS NULL OR parent_criteria_revision < criteria_revision),
  FOREIGN KEY (delegation_id, parent_criteria_revision)
    REFERENCES execass_criteria_sets(delegation_id, criteria_revision)
    ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
);

CREATE UNIQUE INDEX idx_execass_criteria_sets_one_current
ON execass_criteria_sets(delegation_id)
WHERE disposition = 'current';

CREATE TABLE execass_decisions (
  decision_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  decision_revision INTEGER NOT NULL CHECK (decision_revision > 0),
  delegation_revision INTEGER NOT NULL CHECK (delegation_revision > 0),
  plan_revision INTEGER NOT NULL CHECK (plan_revision > 0),
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  decision_kind TEXT NOT NULL CHECK (decision_kind IN (
    'clarification', 'dangerous_action_confirmation', 'owner_configured_checkpoint',
    'recovery_choice', 'duplicate_risk_retry', 'stop', 'policy_change'
  )),
  status TEXT NOT NULL CHECK (status IN ('pending', 'resolved', 'superseded', 'expired')),
  result TEXT CHECK (result IS NULL OR result IN (
    'confirm_and_continue', 'revise', 'decline', 'stop'
  )),
  exact_presented_action_json TEXT NOT NULL CHECK (json_valid(exact_presented_action_json)),
  confirmed_logical_action_identity TEXT NOT NULL,
  manifest_digest TEXT NOT NULL,
  payload_digest TEXT NOT NULL,
  payload_and_material_operands_json TEXT NOT NULL CHECK (json_valid(payload_and_material_operands_json)),
  target_audience_path_json TEXT NOT NULL CHECK (json_valid(target_audience_path_json)),
  connector_tool_identity TEXT,
  connector_tool_version TEXT,
  side_effect_envelope_json TEXT NOT NULL CHECK (json_valid(side_effect_envelope_json)),
  recommendation TEXT NOT NULL,
  consequence TEXT NOT NULL,
  alternatives_json TEXT NOT NULL CHECK (json_valid(alternatives_json)),
  idempotency_key TEXT NOT NULL UNIQUE,
  requested_at INTEGER NOT NULL,
  resolved_at INTEGER,
  resolved_by_authority_provenance_id TEXT REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  CHECK ((status = 'resolved') = (result IS NOT NULL AND resolved_at IS NOT NULL)),
  CHECK ((status = 'resolved') = (resolved_by_authority_provenance_id IS NOT NULL)),
  UNIQUE (delegation_id, decision_revision),
  UNIQUE (delegation_id, decision_id),
  FOREIGN KEY (delegation_id, plan_revision)
    REFERENCES execass_plans(delegation_id, plan_revision) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_decisions_pending
ON execass_decisions(status, decision_kind, delegation_id);

-- A challenge is one expiring, single-resolution presentation.  It is not an
-- authority grant and can never carry an accepted action beyond its resolution.
CREATE TABLE execass_confirmation_challenges (
  challenge_id TEXT PRIMARY KEY,
  decision_id TEXT NOT NULL UNIQUE REFERENCES execass_decisions(decision_id) ON DELETE RESTRICT,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  decision_revision INTEGER NOT NULL CHECK (decision_revision > 0),
  exact_presented_action_json TEXT NOT NULL CHECK (json_valid(exact_presented_action_json)),
  confirmed_logical_action_identity TEXT NOT NULL,
  manifest_digest TEXT NOT NULL,
  payload_digest TEXT NOT NULL,
  payload_and_material_operands_json TEXT NOT NULL CHECK (json_valid(payload_and_material_operands_json)),
  connector_tool_identity TEXT,
  connector_tool_version TEXT,
  canonical_action_envelope_or_selector_json TEXT NOT NULL CHECK (json_valid(canonical_action_envelope_or_selector_json)),
  declared_consequence TEXT NOT NULL,
  selected_logical_action_id TEXT,
  nonce_digest TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('pending', 'resolved', 'expired')),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  resolved_at INTEGER,
  CHECK (expires_at > created_at),
  CHECK ((status = 'resolved') = (resolved_at IS NOT NULL)),
  UNIQUE (delegation_id, decision_revision),
  FOREIGN KEY (delegation_id, decision_id) REFERENCES execass_decisions(delegation_id, decision_id) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_confirmation_challenges_pending
ON execass_confirmation_challenges(status, expires_at, delegation_id);

-- Every disclosed alternative has its own immutable, server-derived full
-- binding.  A combined question intentionally has no selected row while
-- pending; the owner selects exactly one row only when resolving it.
CREATE TABLE execass_confirmation_challenge_alternatives (
  challenge_id TEXT NOT NULL REFERENCES execass_confirmation_challenges(challenge_id) ON DELETE RESTRICT,
  logical_action_id TEXT NOT NULL,
  exact_presented_action_json TEXT NOT NULL CHECK (json_valid(exact_presented_action_json)),
  confirmed_logical_action_identity TEXT NOT NULL,
  manifest_digest TEXT NOT NULL,
  payload_digest TEXT NOT NULL,
  payload_and_material_operands_json TEXT NOT NULL CHECK (json_valid(payload_and_material_operands_json)),
  target_audience_path_json TEXT NOT NULL CHECK (json_valid(target_audience_path_json)),
  connector_tool_identity TEXT,
  connector_tool_version TEXT,
  canonical_action_envelope_or_selector_json TEXT NOT NULL CHECK (json_valid(canonical_action_envelope_or_selector_json)),
  declared_consequence TEXT NOT NULL,
  PRIMARY KEY (challenge_id, logical_action_id)
);

-- Verification authority is selected only from this canonical state root.
-- The attestation wire input never carries verifying-key material.
CREATE TABLE execass_confirmation_authority_keys (
  key_id TEXT PRIMARY KEY,
  key_generation INTEGER NOT NULL CHECK (key_generation > 0),
  verifying_key_hex TEXT NOT NULL UNIQUE CHECK (
    length(verifying_key_hex) = 64 AND verifying_key_hex NOT GLOB '*[^0-9a-f]*'
  ),
  verifying_key_digest TEXT NOT NULL UNIQUE CHECK (
    length(verifying_key_digest) = 64 AND verifying_key_digest NOT GLOB '*[^0-9a-f]*'
  ),
  canonical_root_identity TEXT NOT NULL CHECK (
    length(canonical_root_identity) = 71
    AND canonical_root_identity GLOB 'sha256:*'
    AND substr(canonical_root_identity, 8) NOT GLOB '*[^0-9a-f]*'
  ),
  installation_identity TEXT NOT NULL CHECK (length(trim(installation_identity)) > 0),
  os_user_identity_digest TEXT NOT NULL CHECK (
    length(os_user_identity_digest) = 64 AND os_user_identity_digest NOT GLOB '*[^0-9a-f]*'
  ),
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  status TEXT NOT NULL CHECK (status IN ('active', 'retired')),
  created_at INTEGER NOT NULL CHECK (created_at > 0),
  UNIQUE (key_id, key_generation)
);

CREATE UNIQUE INDEX idx_execass_confirmation_authority_one_active
ON execass_confirmation_authority_keys(status) WHERE status = 'active';

-- One local owner binding is mandatory at use time; one matching remote
-- binding is optional. There are deliberately no tenant, organization, role,
-- or second-owner concepts in this table.
CREATE TABLE execass_owner_ingress_bindings (
  binding_id TEXT PRIMARY KEY,
  actor_type TEXT NOT NULL CHECK (actor_type IN ('human_local', 'human_remote')),
  credential_identity TEXT NOT NULL CHECK (length(trim(credential_identity)) > 0),
  authenticated_ingress TEXT NOT NULL CHECK (length(trim(authenticated_ingress)) > 0),
  channel_assurance TEXT NOT NULL CHECK (length(trim(channel_assurance)) > 0),
  provider_event_required INTEGER NOT NULL CHECK (provider_event_required IN (0, 1)),
  status TEXT NOT NULL CHECK (status IN ('active', 'retired')),
  created_at INTEGER NOT NULL CHECK (created_at > 0),
  CHECK (
    (actor_type = 'human_local' AND provider_event_required = 0)
    OR (actor_type = 'human_remote' AND provider_event_required = 1)
  ),
  -- Re-enrolling a previously retired exact owner/adapter tuple is a new,
  -- append-only generation rather than an update of historical trust proof.
  UNIQUE (actor_type, credential_identity, authenticated_ingress, channel_assurance, created_at)
);

CREATE UNIQUE INDEX idx_execass_owner_ingress_one_active_local
ON execass_owner_ingress_bindings(actor_type)
WHERE status = 'active' AND actor_type = 'human_local';

-- This is the durable replay/provenance record created only after strict
-- signature and all database binding checks pass.
CREATE TABLE execass_confirmation_attestations (
  attestation_digest TEXT PRIMARY KEY CHECK (
    length(attestation_digest) = 64 AND attestation_digest NOT GLOB '*[^0-9a-f]*'
  ),
  decision_id TEXT NOT NULL UNIQUE REFERENCES execass_decisions(decision_id) ON DELETE RESTRICT,
  authority_provenance_id TEXT NOT NULL UNIQUE REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  pinned_key_id TEXT NOT NULL,
  pinned_key_generation INTEGER NOT NULL CHECK (pinned_key_generation > 0),
  actor_type TEXT NOT NULL CHECK (actor_type IN ('human_local', 'human_remote')),
  credential_identity TEXT NOT NULL,
  authenticated_ingress TEXT NOT NULL,
  channel_assurance TEXT NOT NULL,
  request_correlation_id TEXT NOT NULL,
  source_message_id TEXT,
  provider_event_id TEXT,
  selected_logical_action_id TEXT NOT NULL,
  signed_payload_json TEXT NOT NULL CHECK (json_valid(signed_payload_json)),
  signature_hex TEXT NOT NULL CHECK (
    length(signature_hex) = 128 AND signature_hex NOT GLOB '*[^0-9a-f]*'
  ),
  issued_at INTEGER NOT NULL CHECK (issued_at > 0),
  expires_at INTEGER NOT NULL CHECK (expires_at > issued_at),
  verified_at INTEGER NOT NULL CHECK (verified_at >= issued_at AND verified_at < expires_at),
  CHECK (
    (actor_type = 'human_local' AND source_message_id IS NULL AND provider_event_id IS NULL)
    OR (actor_type = 'human_remote' AND source_message_id IS NOT NULL AND provider_event_id IS NOT NULL)
  ),
  FOREIGN KEY (pinned_key_id, pinned_key_generation)
    REFERENCES execass_confirmation_authority_keys(key_id, key_generation) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX idx_execass_confirmation_attestations_provider_event
ON execass_confirmation_attestations(
  credential_identity, authenticated_ingress, provider_event_id
) WHERE provider_event_id IS NOT NULL;

-- Immutable replay and custody proof for one exact run-control transition.
-- No caller-selected generic authority field is accepted here: the authority
-- row is deterministically derived from and bound back to the signed bytes.
CREATE TABLE execass_run_control_attestations (
  attestation_digest TEXT PRIMARY KEY CHECK (
    length(attestation_digest) = 64 AND attestation_digest NOT GLOB '*[^0-9a-f]*'
  ),
  replay_identity TEXT NOT NULL UNIQUE CHECK (length(trim(replay_identity)) > 0),
  authority_provenance_id TEXT NOT NULL UNIQUE REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  pinned_key_id TEXT NOT NULL,
  pinned_key_generation INTEGER NOT NULL CHECK (pinned_key_generation > 0),
  actor_type TEXT NOT NULL CHECK (actor_type IN ('human_local', 'human_remote')),
  credential_identity TEXT NOT NULL CHECK (length(trim(credential_identity)) > 0),
  authenticated_ingress TEXT NOT NULL CHECK (length(trim(authenticated_ingress)) > 0),
  channel_assurance TEXT NOT NULL CHECK (length(trim(channel_assurance)) > 0),
  request_correlation_id TEXT NOT NULL CHECK (length(trim(request_correlation_id)) > 0),
  source_message_id TEXT,
  provider_event_id TEXT,
  operation TEXT NOT NULL CHECK (operation IN (
    'global_stop', 'global_resume', 'delegation_stop', 'delegation_resume'
  )),
  target_kind TEXT NOT NULL CHECK (target_kind IN ('global', 'delegation')),
  target_delegation_id TEXT,
  idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) > 0),
  stopped_epoch INTEGER NOT NULL CHECK (stopped_epoch >= 0),
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  unresolved_effect_disclosure_digest TEXT NOT NULL CHECK (
    length(unresolved_effect_disclosure_digest) = 71
    AND unresolved_effect_disclosure_digest GLOB 'sha256:*'
    AND substr(unresolved_effect_disclosure_digest, 8) NOT GLOB '*[^0-9a-f]*'
  ),
  delegation_state_revision INTEGER CHECK (delegation_state_revision > 0),
  current_plan_revision INTEGER CHECK (current_plan_revision > 0),
  canonical_root_identity TEXT NOT NULL,
  installation_identity TEXT NOT NULL,
  os_user_identity_digest TEXT NOT NULL,
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  normalized_scope_json TEXT NOT NULL CHECK (json_valid(normalized_scope_json)),
  signed_payload_json TEXT NOT NULL CHECK (json_valid(signed_payload_json)),
  signature_hex TEXT NOT NULL CHECK (
    length(signature_hex) = 128 AND signature_hex NOT GLOB '*[^0-9a-f]*'
  ),
  observed_at INTEGER NOT NULL CHECK (observed_at > 0),
  issued_at INTEGER NOT NULL CHECK (issued_at >= observed_at),
  verified_at INTEGER NOT NULL CHECK (verified_at > 0),
  receipt_id TEXT NOT NULL UNIQUE,
  outbox_event_id TEXT NOT NULL UNIQUE,
  receipt_command_digest TEXT NOT NULL CHECK (
    length(receipt_command_digest) = 64 AND receipt_command_digest NOT GLOB '*[^0-9a-f]*'
  ),
  outbox_event_digest TEXT NOT NULL CHECK (
    length(outbox_event_digest) = 64 AND outbox_event_digest NOT GLOB '*[^0-9a-f]*'
  ),
  CHECK (
    (actor_type = 'human_local' AND source_message_id IS NULL AND provider_event_id IS NULL)
    OR (actor_type = 'human_remote' AND source_message_id IS NOT NULL AND provider_event_id IS NOT NULL)
  ),
  CHECK (
    (target_kind = 'global' AND target_delegation_id IS NULL
      AND delegation_state_revision IS NULL AND current_plan_revision IS NULL
      AND operation IN ('global_stop', 'global_resume'))
    OR
    (target_kind = 'delegation' AND target_delegation_id IS NOT NULL
      AND delegation_state_revision IS NOT NULL
      AND operation IN ('delegation_stop', 'delegation_resume'))
  ),
  FOREIGN KEY (pinned_key_id, pinned_key_generation)
    REFERENCES execass_confirmation_authority_keys(key_id, key_generation) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX idx_execass_run_control_attestations_provider_event
ON execass_run_control_attestations(
  credential_identity, authenticated_ingress, provider_event_id
) WHERE provider_event_id IS NOT NULL;

-- An accepted confirmation is durable.  There is intentionally no expiry or
-- use counter: only material action drift or an explicit action-specific owner
-- amendment, revocation, or cancellation can invalidate it.
CREATE TABLE execass_accepted_confirmation_grants (
  grant_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  decision_id TEXT NOT NULL UNIQUE REFERENCES execass_decisions(decision_id) ON DELETE RESTRICT,
  confirmed_logical_action_identity TEXT NOT NULL,
  canonical_action_envelope_or_selector_json TEXT NOT NULL CHECK (json_valid(canonical_action_envelope_or_selector_json)),
  payload_and_material_operands_json TEXT NOT NULL CHECK (json_valid(payload_and_material_operands_json)),
  payload_and_material_operands_digest TEXT NOT NULL,
  connector_tool_identity TEXT,
  connector_tool_version TEXT,
  declared_consequence TEXT NOT NULL,
  accepted_by_authority_provenance_id TEXT NOT NULL REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  confirmation_attestation_digest TEXT NOT NULL UNIQUE REFERENCES execass_confirmation_attestations(attestation_digest) ON DELETE RESTRICT,
  accepted_at INTEGER NOT NULL,
  invalidated_at INTEGER,
  invalidation_reason TEXT CHECK (invalidation_reason IS NULL OR invalidation_reason IN (
    'material_target_drift', 'material_scope_drift', 'material_payload_drift',
    'material_tool_drift', 'material_consequence_drift',
    'explicit_action_specific_owner_amendment',
    'explicit_action_specific_owner_revocation',
    'explicit_action_specific_owner_cancellation'
  )),
  invalidated_by_authority_provenance_id TEXT REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  CHECK ((invalidated_at IS NULL) = (invalidation_reason IS NULL)),
  CHECK (
    invalidation_reason NOT IN (
      'explicit_action_specific_owner_amendment',
      'explicit_action_specific_owner_revocation',
      'explicit_action_specific_owner_cancellation'
    ) OR invalidated_by_authority_provenance_id IS NOT NULL
  ),
  CHECK (
    invalidation_reason IN (
      'explicit_action_specific_owner_amendment',
      'explicit_action_specific_owner_revocation',
      'explicit_action_specific_owner_cancellation'
    ) OR invalidated_by_authority_provenance_id IS NULL
  ),
  UNIQUE (delegation_id, confirmed_logical_action_identity)
);

CREATE TABLE execass_continuations (
  continuation_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  target_delegation_revision INTEGER NOT NULL CHECK (target_delegation_revision > 0),
  target_plan_revision INTEGER NOT NULL CHECK (target_plan_revision > 0),
  action_id TEXT NOT NULL,
  branch_kind TEXT NOT NULL CHECK (branch_kind IN ('ordinary', 'recovery')),
  causation_kind TEXT NOT NULL CHECK (causation_kind IN (
    'intake', 'plan', 'amendment', 'decision', 'action_result', 'recovery', 'resume', 'routine_occurrence'
  )),
  causation_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('runnable', 'executing', 'waiting', 'uncertain', 'terminal', 'superseded')),
  job_id TEXT REFERENCES jobs(job_id) ON DELETE RESTRICT,
  lease_owner TEXT,
  lease_expires_at INTEGER,
  fencing_token INTEGER NOT NULL DEFAULT 0 CHECK (fencing_token >= 0),
  host_generation INTEGER NOT NULL CHECK (host_generation > 0),
  stop_epoch INTEGER NOT NULL CHECK (stop_epoch >= 0),
  global_stop_epoch INTEGER NOT NULL CHECK (global_stop_epoch >= 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  completed_at INTEGER,
  CHECK (updated_at >= created_at),
  UNIQUE (causation_kind, causation_id),
  UNIQUE (delegation_id, continuation_id),
  FOREIGN KEY (delegation_id, target_plan_revision)
    REFERENCES execass_plans(delegation_id, plan_revision) ON DELETE RESTRICT,
  UNIQUE (action_id),
  FOREIGN KEY (delegation_id, action_id)
    REFERENCES execass_action_branches(delegation_id, action_id)
    ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX idx_execass_continuations_claim
ON execass_continuations(status, lease_expires_at, delegation_id);

CREATE UNIQUE INDEX idx_execass_continuations_one_current_per_revision
ON execass_continuations(delegation_id, target_delegation_revision)
WHERE status IN ('runnable', 'executing');

CREATE UNIQUE INDEX idx_execass_continuations_one_job
ON execass_continuations(job_id)
WHERE job_id IS NOT NULL;

CREATE TABLE execass_continuation_operation_history (
  event_id TEXT PRIMARY KEY REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  claim_event_id TEXT NOT NULL REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  claim_receipt_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK (operation IN ('claim', 'settle', 'expire', 'recover', 'reconcile')),
  result_status TEXT NOT NULL CHECK (result_status IN ('executing', 'waiting', 'uncertain', 'terminal', 'superseded')),
  continuation_id TEXT NOT NULL REFERENCES execass_continuations(continuation_id) ON DELETE RESTRICT,
  delegation_id TEXT NOT NULL,
  action_id TEXT NOT NULL,
  job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE RESTRICT,
  worker_id TEXT NOT NULL,
  job_lease_expires_at INTEGER NOT NULL,
  continuation_fencing_token INTEGER NOT NULL CHECK (continuation_fencing_token > 0),
  runtime_host_generation INTEGER NOT NULL CHECK (runtime_host_generation > 0),
  runtime_host_instance_id TEXT NOT NULL,
  runtime_fencing_token INTEGER NOT NULL CHECK (runtime_fencing_token > 0),
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  runtime_authority_provenance_id TEXT NOT NULL REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  runtime_actor_identity TEXT NOT NULL,
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  global_stop_epoch INTEGER NOT NULL CHECK (global_stop_epoch >= 0),
  technical_quota_policy_digest TEXT NOT NULL,
  technical_quota_snapshot_id TEXT,
  technical_resource_reservation_set_json TEXT NOT NULL CHECK (json_valid(technical_resource_reservation_set_json) AND json_type(technical_resource_reservation_set_json)='array'),
  technical_resource_reservation_set_digest TEXT NOT NULL,
  technical_resource_evidence_digest TEXT,
  recorded_at INTEGER NOT NULL,
  FOREIGN KEY (delegation_id, continuation_id)
    REFERENCES execass_continuations(delegation_id, continuation_id) ON DELETE RESTRICT,
  FOREIGN KEY (delegation_id, action_id)
    REFERENCES execass_action_branches(delegation_id, action_id) ON DELETE RESTRICT,
  FOREIGN KEY (runtime_host_generation, runtime_host_instance_id)
    REFERENCES execass_runtime_host_generations(generation, host_instance_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX idx_execass_continuation_one_nonreconcile_operation
ON execass_continuation_operation_history(continuation_id,continuation_fencing_token,operation)
WHERE operation <> 'reconcile';

CREATE TRIGGER execass_continuation_job_binding_one_way
BEFORE UPDATE OF job_id ON execass_continuations
WHEN OLD.job_id IS NOT NULL AND NEW.job_id IS NOT OLD.job_id
BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation job binding is immutable');
END;

CREATE TRIGGER execass_continuation_operation_history_immutable
BEFORE UPDATE ON execass_continuation_operation_history
BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation operation history is immutable');
END;

CREATE TRIGGER execass_continuation_operation_history_no_delete
BEFORE DELETE ON execass_continuation_operation_history
BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation operation history cannot be deleted');
END;

CREATE TRIGGER execass_continuation_claim_transition_requires_history
BEFORE UPDATE OF status,fencing_token,lease_owner,lease_expires_at,host_generation
ON execass_continuations
WHEN OLD.status='runnable' AND NEW.status='executing'
  AND NOT EXISTS (
    SELECT 1 FROM execass_continuation_operation_history h
    WHERE h.operation='claim'
      AND h.continuation_id=NEW.continuation_id
      AND h.action_id=NEW.action_id
      AND h.job_id=NEW.job_id
      AND h.worker_id=NEW.lease_owner
      AND h.job_lease_expires_at=NEW.lease_expires_at
      AND h.continuation_fencing_token=NEW.fencing_token
      AND h.runtime_host_generation=NEW.host_generation
  )
BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation claim transition requires immutable history');
END;

CREATE TRIGGER execass_continuation_settle_transition_requires_history
BEFORE UPDATE OF status,lease_owner,lease_expires_at,completed_at
ON execass_continuations
WHEN OLD.status='executing' AND NEW.status!='executing'
  AND NOT EXISTS (
    SELECT 1 FROM execass_continuation_operation_history h
    WHERE h.operation IN ('settle','expire','recover','reconcile')
      AND h.continuation_id=NEW.continuation_id
      AND h.action_id=NEW.action_id
      AND h.job_id=NEW.job_id
      AND h.continuation_fencing_token=NEW.fencing_token
      AND h.result_status=NEW.status
  )
BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation settle transition requires immutable history');
END;

-- ExecAss continuations reuse the one canonical jobs scheduler. The scheduler
-- may update lease/run state, but generic job APIs must never rewrite the
-- durable identity or payload that binds a job to one continuation.
CREATE TRIGGER execass_internal_job_identity_immutable
BEFORE UPDATE OF
  job_id, agent_id, name, schedule_kind, interval_seconds, run_at_ms,
  payload_json, max_retries, retry_backoff_ms, timeout_ms, deleted_at
ON jobs
WHEN json_extract(OLD.payload_json, '$.mode') = 'execass.continuation'
BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation job identity is immutable');
END;

CREATE TRIGGER execass_internal_job_delete_forbidden
BEFORE DELETE ON jobs
WHEN json_extract(OLD.payload_json, '$.mode') = 'execass.continuation'
BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation jobs cannot be deleted');
END;

-- EA-109 lifecycle kernel. These tables hold the only storage-local inputs to
-- phase selection, rather than deriving product truth from scheduler state.
CREATE TABLE execass_action_branches (
  action_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  action_revision INTEGER NOT NULL CHECK (action_revision > 0),
  target_delegation_revision INTEGER NOT NULL CHECK (target_delegation_revision > 0),
  target_plan_revision INTEGER NOT NULL CHECK (target_plan_revision > 0),
  stop_epoch INTEGER NOT NULL CHECK (stop_epoch >= 0),
  branch_kind TEXT NOT NULL CHECK (branch_kind IN ('ordinary', 'recovery')),
  status TEXT NOT NULL CHECK (status IN ('runnable', 'executing', 'waiting', 'uncertain', 'terminal', 'superseded')),
  action_summary TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  terminal_at INTEGER,
  CHECK (updated_at >= created_at),
  CHECK ((status IN ('terminal', 'superseded')) = (terminal_at IS NOT NULL)),
  UNIQUE (delegation_id, action_revision),
  UNIQUE (delegation_id, action_id),
  FOREIGN KEY (delegation_id, target_plan_revision)
    REFERENCES execass_plans(delegation_id, plan_revision) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_action_branches_selection
ON execass_action_branches(delegation_id, branch_kind, status, action_revision);

CREATE TABLE execass_attention_items (
  attention_id TEXT PRIMARY KEY,
  scope_kind TEXT NOT NULL DEFAULT 'delegation' CHECK (scope_kind IN ('delegation', 'runtime_host')),
  delegation_id TEXT REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  action_id TEXT REFERENCES execass_action_branches(action_id) ON DELETE RESTRICT,
  kind TEXT NOT NULL CHECK (kind IN (
    'confirmation', 'clarification', 'reply', 'recovery_choice', 'runtime_paused'
  )),
  status TEXT NOT NULL CHECK (status IN ('actionable', 'resolved', 'superseded')),
  reason TEXT NOT NULL,
  recommendation TEXT NOT NULL,
  alternatives_json TEXT NOT NULL CHECK (json_valid(alternatives_json)),
  required_assurance TEXT NOT NULL,
  decision_id TEXT REFERENCES execass_decisions(decision_id) ON DELETE RESTRICT,
  delegation_revision INTEGER CHECK (delegation_revision IS NULL OR delegation_revision > 0),
  runtime_host_generation INTEGER,
  runtime_host_instance_id TEXT,
  runtime_fencing_token INTEGER,
  runtime_actual_state TEXT CHECK (runtime_actual_state IS NULL OR runtime_actual_state IN (
    'starting','running_app_bound','handoff','running_background','draining','faulted'
  )),
  runtime_end_reason TEXT,
  active_work_binding_digest TEXT CHECK (
    active_work_binding_digest IS NULL OR
    (length(active_work_binding_digest) = 71 AND substr(active_work_binding_digest,1,7) = 'sha256:'
      AND substr(active_work_binding_digest,8) NOT GLOB '*[^0-9a-f]*')
  ),
  outbox_event_id TEXT REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  receipt_id TEXT REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT
    DEFERRABLE INITIALLY DEFERRED,
  created_at INTEGER NOT NULL,
  resolved_at INTEGER,
  CHECK ((status = 'actionable') = (resolved_at IS NULL)),
  CHECK (
    (scope_kind = 'delegation'
      AND delegation_id IS NOT NULL AND delegation_revision IS NOT NULL
      AND kind != 'runtime_paused'
      AND runtime_host_generation IS NULL AND runtime_host_instance_id IS NULL
      AND runtime_fencing_token IS NULL AND runtime_actual_state IS NULL
      AND runtime_end_reason IS NULL AND active_work_binding_digest IS NULL
      AND outbox_event_id IS NULL AND receipt_id IS NULL)
    OR
    (scope_kind = 'runtime_host'
      AND delegation_id IS NULL AND delegation_revision IS NULL
      AND action_id IS NULL AND decision_id IS NULL AND kind = 'runtime_paused'
      AND runtime_host_generation > 0 AND runtime_host_instance_id IS NOT NULL
      AND runtime_fencing_token > 0 AND runtime_actual_state IS NOT NULL
      AND runtime_end_reason IS NOT NULL AND active_work_binding_digest IS NOT NULL
      AND outbox_event_id IS NOT NULL AND receipt_id IS NOT NULL)
  ),
  FOREIGN KEY (runtime_host_generation, runtime_host_instance_id)
    REFERENCES execass_runtime_host_generations(generation, host_instance_id) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_attention_items_actionable
ON execass_attention_items(delegation_id, status, created_at);

CREATE TRIGGER execass_attention_runtime_generation_unique
BEFORE INSERT ON execass_attention_items
WHEN NEW.scope_kind='runtime_host' AND EXISTS (
  SELECT 1 FROM execass_attention_items existing
  WHERE existing.scope_kind='runtime_host'
    AND existing.runtime_host_generation=NEW.runtime_host_generation
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime generation already has canonical attention');
END;

CREATE TABLE execass_external_waits (
  external_wait_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  action_id TEXT REFERENCES execass_action_branches(action_id) ON DELETE RESTRICT,
  kind TEXT NOT NULL CHECK (kind IN ('external_party', 'system', 'time')),
  status TEXT NOT NULL CHECK (status IN ('waiting', 'resolved', 'superseded')),
  reason TEXT NOT NULL,
  details_json TEXT NOT NULL CHECK (json_valid(details_json)),
  delegation_revision INTEGER NOT NULL CHECK (delegation_revision > 0),
  created_at INTEGER NOT NULL,
  resolved_at INTEGER,
  CHECK ((status = 'waiting') = (resolved_at IS NULL))
);

CREATE INDEX idx_execass_external_waits_waiting
ON execass_external_waits(delegation_id, status, created_at);

CREATE TABLE execass_completion_assessments (
  assessment_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  assessment_revision INTEGER NOT NULL CHECK (assessment_revision > 0),
  criteria_revision INTEGER NOT NULL CHECK (criteria_revision > 0),
  terminal_phase TEXT NOT NULL CHECK (terminal_phase IN ('completed', 'partially_completed', 'failed')),
  material_pass_count INTEGER NOT NULL CHECK (material_pass_count >= 0),
  material_fail_count INTEGER NOT NULL CHECK (material_fail_count >= 0),
  material_unknown_count INTEGER NOT NULL CHECK (material_unknown_count >= 0),
  useful_outcome INTEGER NOT NULL CHECK (useful_outcome IN (0, 1)),
  exact_unmet_portion TEXT,
  no_remaining_path INTEGER NOT NULL CHECK (no_remaining_path IN (0, 1)),
  assessment_json TEXT NOT NULL CHECK (json_valid(assessment_json)),
  assessed_at INTEGER NOT NULL,
  UNIQUE (delegation_id, assessment_revision),
  FOREIGN KEY (delegation_id, criteria_revision)
    REFERENCES execass_criteria_sets(delegation_id, criteria_revision) ON DELETE RESTRICT,
  CHECK (terminal_phase != 'completed' OR (material_fail_count = 0 AND material_unknown_count = 0)),
  CHECK (terminal_phase != 'partially_completed' OR (useful_outcome = 1 AND no_remaining_path = 1 AND exact_unmet_portion IS NOT NULL)),
  CHECK (terminal_phase != 'failed' OR useful_outcome = 0)
);

CREATE TABLE execass_lifecycle_transitions (
  transition_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  state_revision INTEGER NOT NULL CHECK (state_revision > 0),
  previous_phase TEXT NOT NULL CHECK (previous_phase IN ('accepted', 'planning', 'in_motion', 'waiting_for_user', 'waiting_external', 'recovering', 'completed', 'partially_completed', 'failed')),
  selected_phase TEXT NOT NULL CHECK (selected_phase IN ('accepted', 'planning', 'in_motion', 'waiting_for_user', 'waiting_external', 'recovering', 'completed', 'partially_completed', 'failed')),
  previous_run_control TEXT NOT NULL CHECK (previous_run_control IN ('running', 'stop_requested', 'stopped')),
  selected_run_control TEXT NOT NULL CHECK (selected_run_control IN ('running', 'stop_requested', 'stopped')),
  selector_input_json TEXT NOT NULL CHECK (json_valid(selector_input_json)),
  command_identity TEXT NOT NULL UNIQUE,
  projection_snapshot_json TEXT NOT NULL CHECK (json_valid(projection_snapshot_json)),
  reason TEXT NOT NULL,
  outbox_event_id TEXT NOT NULL UNIQUE REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  occurred_at INTEGER NOT NULL,
  UNIQUE (delegation_id, state_revision)
);

CREATE TABLE execass_terminal_corrections (
  correction_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  terminal_assessment_id TEXT NOT NULL REFERENCES execass_completion_assessments(assessment_id) ON DELETE RESTRICT,
  correction_revision INTEGER NOT NULL CHECK (correction_revision > 0),
  contrary_evidence_json TEXT NOT NULL CHECK (json_valid(contrary_evidence_json)),
  warning TEXT NOT NULL,
  recorded_at INTEGER NOT NULL,
  command_identity TEXT NOT NULL UNIQUE,
  outbox_event_id TEXT NOT NULL UNIQUE REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  UNIQUE (delegation_id, correction_revision)
);

CREATE TABLE execass_amendment_criteria_links (
  amendment_id TEXT PRIMARY KEY REFERENCES execass_plan_amendments(amendment_id) ON DELETE RESTRICT,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  superseded_criteria_revision INTEGER NOT NULL CHECK (superseded_criteria_revision > 0),
  resulting_criteria_revision INTEGER NOT NULL CHECK (resulting_criteria_revision > superseded_criteria_revision),
  UNIQUE (delegation_id, resulting_criteria_revision),
  FOREIGN KEY (delegation_id, superseded_criteria_revision)
    REFERENCES execass_criteria_sets(delegation_id, criteria_revision) ON DELETE RESTRICT,
  FOREIGN KEY (delegation_id, resulting_criteria_revision)
    REFERENCES execass_criteria_sets(delegation_id, criteria_revision) ON DELETE RESTRICT
);

CREATE TABLE execass_logical_effects (
  logical_effect_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  continuation_id TEXT NOT NULL,
  action_kind TEXT NOT NULL CHECK (action_kind IN (
    'read_only_local_inspection_and_bounded_reversible_local_work',
    'private_draft_creation_without_transmission',
    'public_or_externally_consequential_communication',
    'irreversible_or_destructive_action',
    'credential_permission_privilege_or_trust_policy_change',
    'project_defining_scope_ownership_or_launch_decision',
    'secret_use_through_authorized_connector',
    'unknown_composite_aliased_plugin_shell_or_changed_version_action'
  )),
  operation_reversible INTEGER NOT NULL DEFAULT 0 CHECK (operation_reversible IN (0, 1)),
  declared_recovery_safe_boundary TEXT NOT NULL DEFAULT 'independent_absence'
    CHECK (declared_recovery_safe_boundary IN ('independent_absence')),
  state TEXT NOT NULL CHECK (state IN (
    'planned', 'claimed', 'invoking', 'succeeded', 'failed', 'outcome_unknown', 'reconciled_absent', 'reconciled_present'
  )),
  internal_idempotency_key TEXT NOT NULL UNIQUE,
  provider_identity TEXT,
  provider_idempotency_key TEXT,
  reconciliation_key TEXT,
  manifest_digest TEXT NOT NULL,
  payload_digest TEXT NOT NULL,
  outcome_json TEXT CHECK (outcome_json IS NULL OR json_valid(outcome_json)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY (delegation_id, continuation_id)
    REFERENCES execass_continuations(delegation_id, continuation_id) ON DELETE RESTRICT,
  UNIQUE (delegation_id, logical_effect_id),
  UNIQUE (delegation_id, continuation_id),
  UNIQUE (provider_identity, provider_idempotency_key),
  UNIQUE (provider_identity, reconciliation_key),
  CHECK (provider_idempotency_key IS NULL OR provider_identity IS NOT NULL),
  CHECK (
    (provider_identity IS NULL AND provider_idempotency_key IS NULL AND reconciliation_key IS NULL)
    OR
    (provider_identity IS NOT NULL AND (provider_idempotency_key IS NOT NULL OR reconciliation_key IS NOT NULL))
  ),
  CHECK (updated_at >= created_at)
);

CREATE INDEX idx_execass_logical_effects_reconcile
ON execass_logical_effects(state, provider_identity, updated_at);

CREATE TABLE execass_provider_attempts (
  attempt_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  logical_effect_id TEXT NOT NULL,
  continuation_id TEXT NOT NULL,
  action_id TEXT NOT NULL,
  claim_event_id TEXT NOT NULL REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  claim_receipt_id TEXT NOT NULL,
  attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
  fencing_token INTEGER NOT NULL CHECK (fencing_token > 0),
  host_generation INTEGER NOT NULL CHECK (host_generation > 0),
  host_instance_id TEXT NOT NULL,
  runtime_fencing_token INTEGER NOT NULL CHECK (runtime_fencing_token > 0),
  status TEXT NOT NULL CHECK (status IN (
    'prepared', 'invoking', 'succeeded', 'failed', 'outcome_unknown', 'reconciled_absent', 'reconciled_present'
  )),
  provider_request_digest TEXT NOT NULL,
  provider_response_digest TEXT,
  provider_error_class TEXT CHECK (provider_error_class IS NULL OR provider_error_class IN (
    'transient','rate_limited','authentication','permanent','unknown'
  )),
  remote_effect_id TEXT,
  started_at INTEGER NOT NULL,
  finished_at INTEGER,
  UNIQUE (logical_effect_id, attempt_number),
  FOREIGN KEY (delegation_id, logical_effect_id)
    REFERENCES execass_logical_effects(delegation_id, logical_effect_id) ON DELETE RESTRICT
  ,FOREIGN KEY (delegation_id, continuation_id)
    REFERENCES execass_continuations(delegation_id, continuation_id) ON DELETE RESTRICT
  ,CHECK (
    (status IN ('prepared','invoking') AND provider_response_digest IS NULL AND remote_effect_id IS NULL AND finished_at IS NULL)
    OR
    (status IN ('succeeded','failed','outcome_unknown','reconciled_absent','reconciled_present')
      AND provider_response_digest IS NOT NULL AND finished_at IS NOT NULL)
  ),
  CHECK ((status = 'failed') = (provider_error_class IS NOT NULL))
);

-- The effect recorder has its own public identity. It is deliberately
-- separate from confirmation and receipt-integrity keys. Evidence never
-- supplies verification authority: imports resolve exactly one active row
-- from this table first and only then verify the signed journal record.
CREATE TABLE execass_effect_recorder_keys (
  recorder_key_id TEXT PRIMARY KEY,
  key_generation INTEGER NOT NULL CHECK (key_generation > 0),
  verifying_key_hex TEXT NOT NULL UNIQUE CHECK (
    length(verifying_key_hex) = 64 AND verifying_key_hex NOT GLOB '*[^0-9a-f]*'
  ),
  verifying_key_digest TEXT NOT NULL UNIQUE CHECK (
    length(verifying_key_digest) = 64 AND verifying_key_digest NOT GLOB '*[^0-9a-f]*'
  ),
  canonical_root_identity TEXT NOT NULL CHECK (
    length(canonical_root_identity) = 71
    AND canonical_root_identity GLOB 'sha256:*'
    AND substr(canonical_root_identity, 8) NOT GLOB '*[^0-9a-f]*'
  ),
  installation_identity TEXT NOT NULL CHECK (length(trim(installation_identity)) > 0),
  os_user_identity_digest TEXT NOT NULL CHECK (
    length(os_user_identity_digest) = 64 AND os_user_identity_digest NOT GLOB '*[^0-9a-f]*'
  ),
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  status TEXT NOT NULL CHECK (status IN ('active', 'retired')),
  created_at INTEGER NOT NULL CHECK (created_at > 0),
  UNIQUE (recorder_key_id, key_generation)
);

CREATE UNIQUE INDEX idx_execass_effect_recorder_one_active
ON execass_effect_recorder_keys(status) WHERE status = 'active';

CREATE UNIQUE INDEX idx_execass_effect_recorder_active_generation
ON execass_effect_recorder_keys(key_generation) WHERE status = 'active';

CREATE TABLE execass_effect_recorder_evidence (
  recorder_record_digest TEXT PRIMARY KEY CHECK (
    length(recorder_record_digest) = 71
    AND recorder_record_digest GLOB 'sha256:*'
    AND substr(recorder_record_digest, 8) NOT GLOB '*[^0-9a-f]*'
  ),
  record_id TEXT NOT NULL UNIQUE CHECK (length(trim(record_id)) > 0),
  recorder_key_id TEXT NOT NULL,
  key_generation INTEGER NOT NULL CHECK (key_generation > 0),
  canonical_root_identity TEXT NOT NULL CHECK (
    length(canonical_root_identity) = 71
    AND canonical_root_identity GLOB 'sha256:*'
    AND substr(canonical_root_identity, 8) NOT GLOB '*[^0-9a-f]*'
  ),
  installation_identity TEXT NOT NULL CHECK (length(trim(installation_identity)) > 0),
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  os_user_identity_digest TEXT NOT NULL CHECK (
    length(os_user_identity_digest) = 64 AND os_user_identity_digest NOT GLOB '*[^0-9a-f]*'
  ),
  journal_sequence INTEGER NOT NULL CHECK (journal_sequence > 0),
  journal_kind TEXT NOT NULL CHECK (journal_kind IN ('present', 'absent', 'unknown')),
  journal_source TEXT NOT NULL CHECK (journal_source IN ('execution', 'reconciliation')),
  attempt_id TEXT NOT NULL REFERENCES execass_provider_attempts(attempt_id) ON DELETE RESTRICT,
  logical_effect_id TEXT NOT NULL REFERENCES execass_logical_effects(logical_effect_id) ON DELETE RESTRICT,
  command_digest TEXT NOT NULL CHECK (
    length(command_digest) = 71 AND command_digest GLOB 'sha256:*'
    AND substr(command_digest, 8) NOT GLOB '*[^0-9a-f]*'
  ),
  provider_identity TEXT NOT NULL CHECK (length(trim(provider_identity)) > 0),
  provider_version TEXT NOT NULL CHECK (length(trim(provider_version)) > 0),
  provider_request_digest TEXT NOT NULL CHECK (
    length(provider_request_digest) = 71 AND provider_request_digest GLOB 'sha256:*'
    AND substr(provider_request_digest, 8) NOT GLOB '*[^0-9a-f]*'
  ),
  provider_idempotency_key_digest TEXT CHECK (
    provider_idempotency_key_digest IS NULL OR (
      length(provider_idempotency_key_digest) = 71
      AND provider_idempotency_key_digest GLOB 'sha256:*'
      AND substr(provider_idempotency_key_digest, 8) NOT GLOB '*[^0-9a-f]*'
    )
  ),
  reconciliation_key_digest TEXT CHECK (
    reconciliation_key_digest IS NULL OR (
      length(reconciliation_key_digest) = 71
      AND reconciliation_key_digest GLOB 'sha256:*'
      AND substr(reconciliation_key_digest, 8) NOT GLOB '*[^0-9a-f]*'
    )
  ),
  remote_effect_id TEXT,
  response_digest TEXT CHECK (
    response_digest IS NULL OR (
      length(response_digest) = 71 AND response_digest GLOB 'sha256:*'
      AND substr(response_digest, 8) NOT GLOB '*[^0-9a-f]*'
    )
  ),
  provider_error_class TEXT CHECK (provider_error_class IS NULL OR provider_error_class IN (
    'transient','rate_limited','authentication','permanent','unknown'
  )),
  evidence_payload_digest TEXT CHECK (
    evidence_payload_digest IS NULL OR (
      length(evidence_payload_digest) = 71 AND evidence_payload_digest GLOB 'sha256:*'
      AND substr(evidence_payload_digest, 8) NOT GLOB '*[^0-9a-f]*'
    )
  ),
  technical_resource_actuals_json TEXT NOT NULL CHECK (
    json_valid(technical_resource_actuals_json)
    AND json_type(technical_resource_actuals_json) = 'array'
  ),
  reconciliation_window_start INTEGER,
  reconciliation_window_end INTEGER,
  observed_at INTEGER NOT NULL CHECK (observed_at > 0),
  imported_at INTEGER NOT NULL CHECK (imported_at >= observed_at),
  previous_record_digest TEXT NOT NULL,
  signed_payload_json TEXT NOT NULL CHECK (json_valid(signed_payload_json)),
  signature TEXT NOT NULL CHECK (
    length(signature) = 128 AND signature NOT GLOB '*[^0-9a-f]*'
  ),
  FOREIGN KEY (recorder_key_id, key_generation)
    REFERENCES execass_effect_recorder_keys(recorder_key_id, key_generation) ON DELETE RESTRICT,
  UNIQUE (recorder_key_id, key_generation, journal_sequence),
  UNIQUE (attempt_id, journal_sequence),
  CHECK (
    (journal_source = 'execution'
      AND reconciliation_window_start IS NULL
      AND reconciliation_window_end IS NULL)
    OR
    (journal_source = 'reconciliation'
      AND reconciliation_window_start IS NOT NULL
      AND reconciliation_window_end IS NOT NULL
      AND reconciliation_window_start >= 0
      AND reconciliation_window_end >= reconciliation_window_start
      AND observed_at >= reconciliation_window_end)
  ),
  CHECK (journal_kind = 'present' OR json_array_length(technical_resource_actuals_json) = 0),
  CHECK (journal_kind <> 'absent' OR remote_effect_id IS NULL),
  CHECK (
    (journal_source='execution' AND journal_kind='absent' AND provider_error_class IS NOT NULL)
    OR
    (NOT (journal_source='execution' AND journal_kind='absent') AND provider_error_class IS NULL)
  )
);

CREATE INDEX idx_execass_effect_recorder_evidence_attempt
ON execass_effect_recorder_evidence(attempt_id, journal_sequence DESC);

CREATE TABLE execass_recovery_episodes (
  recovery_episode_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  logical_effect_id TEXT NOT NULL UNIQUE,
  initial_attempt_id TEXT NOT NULL REFERENCES execass_provider_attempts(attempt_id) ON DELETE RESTRICT,
  action_id TEXT NOT NULL,
  manifest_digest TEXT NOT NULL,
  normalized_intent_digest TEXT NOT NULL CHECK (
    length(normalized_intent_digest)=71 AND normalized_intent_digest GLOB 'sha256:*'
    AND substr(normalized_intent_digest,8) NOT GLOB '*[^0-9a-f]*'
  ),
  effective_authority_digest TEXT NOT NULL CHECK (
    length(effective_authority_digest)=71 AND effective_authority_digest GLOB 'sha256:*'
    AND substr(effective_authority_digest,8) NOT GLOB '*[^0-9a-f]*'
  ),
  accepted_confirmation_grant_id TEXT REFERENCES execass_accepted_confirmation_grants(grant_id) ON DELETE RESTRICT,
  policy_json TEXT NOT NULL CHECK (json_valid(policy_json)),
  policy_digest TEXT NOT NULL CHECK (
    length(policy_digest)=71 AND policy_digest GLOB 'sha256:*'
    AND substr(policy_digest,8) NOT GLOB '*[^0-9a-f]*'
  ),
  opened_at INTEGER NOT NULL CHECK (opened_at > 0),
  UNIQUE (recovery_episode_id, delegation_id, logical_effect_id),
  FOREIGN KEY (delegation_id, logical_effect_id)
    REFERENCES execass_logical_effects(delegation_id, logical_effect_id) ON DELETE RESTRICT
);

CREATE TABLE execass_recovery_evaluations (
  recovery_evaluation_id TEXT PRIMARY KEY,
  recovery_episode_id TEXT NOT NULL REFERENCES execass_recovery_episodes(recovery_episode_id) ON DELETE RESTRICT,
  delegation_id TEXT NOT NULL,
  logical_effect_id TEXT NOT NULL,
  predecessor_attempt_id TEXT NOT NULL REFERENCES execass_provider_attempts(attempt_id) ON DELETE RESTRICT,
  evaluation_revision INTEGER NOT NULL CHECK (evaluation_revision > 0),
  recovery_state_revision INTEGER NOT NULL CHECK (recovery_state_revision > 0),
  objective_facts_json TEXT NOT NULL CHECK (json_valid(objective_facts_json)),
  objective_facts_digest TEXT NOT NULL CHECK (
    length(objective_facts_digest)=71 AND objective_facts_digest GLOB 'sha256:*'
    AND substr(objective_facts_digest,8) NOT GLOB '*[^0-9a-f]*'
  ),
  directive TEXT NOT NULL CHECK (directive IN (
    'retry_same_effect','replan_within_original_authority','wait_backoff',
    'wait_circuit_breaker','waiting_external','waiting_for_user',
    'partially_completed','failed'
  )),
  directive_json TEXT NOT NULL CHECK (json_valid(directive_json)),
  directive_digest TEXT NOT NULL CHECK (
    length(directive_digest)=71 AND directive_digest GLOB 'sha256:*'
    AND substr(directive_digest,8) NOT GLOB '*[^0-9a-f]*'
  ),
  not_before_ms INTEGER,
  outbox_event_id TEXT NOT NULL UNIQUE REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  evaluated_at INTEGER NOT NULL CHECK (evaluated_at > 0),
  CHECK (
    (directive IN ('wait_backoff','wait_circuit_breaker')
      AND not_before_ms IS NOT NULL AND not_before_ms > evaluated_at)
    OR
    (directive='retry_same_effect'
      AND not_before_ms IS NOT NULL AND not_before_ms <= evaluated_at)
    OR
    (directive NOT IN ('wait_backoff','wait_circuit_breaker','retry_same_effect')
      AND not_before_ms IS NULL)
  ),
  UNIQUE (recovery_episode_id, evaluation_revision),
  UNIQUE (recovery_episode_id, objective_facts_digest, directive_digest),
  FOREIGN KEY (recovery_episode_id, delegation_id, logical_effect_id)
    REFERENCES execass_recovery_episodes(recovery_episode_id, delegation_id, logical_effect_id) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_recovery_evaluations_due
ON execass_recovery_evaluations(directive, not_before_ms, evaluated_at);

CREATE TABLE execass_duplicate_risk_bindings (
  decision_id TEXT PRIMARY KEY REFERENCES execass_decisions(decision_id) ON DELETE RESTRICT,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  predecessor_logical_effect_id TEXT NOT NULL UNIQUE,
  predecessor_attempt_id TEXT NOT NULL UNIQUE REFERENCES execass_provider_attempts(attempt_id) ON DELETE RESTRICT,
  predecessor_uncertainty_evidence_digest TEXT NOT NULL,
  confirmed_logical_action_identity TEXT NOT NULL,
  accepted_confirmation_grant_id TEXT REFERENCES execass_accepted_confirmation_grants(grant_id) ON DELETE RESTRICT,
  created_at INTEGER NOT NULL,
  UNIQUE (decision_id, predecessor_logical_effect_id),
  UNIQUE (decision_id, predecessor_attempt_id),
  FOREIGN KEY (delegation_id, predecessor_logical_effect_id)
    REFERENCES execass_logical_effects(delegation_id, logical_effect_id) ON DELETE RESTRICT
);

CREATE TABLE execass_duplicate_risk_successors (
  decision_id TEXT PRIMARY KEY REFERENCES execass_duplicate_risk_bindings(decision_id) ON DELETE RESTRICT,
  predecessor_logical_effect_id TEXT NOT NULL UNIQUE,
  predecessor_attempt_id TEXT NOT NULL,
  successor_logical_effect_id TEXT NOT NULL UNIQUE REFERENCES execass_logical_effects(logical_effect_id) ON DELETE RESTRICT,
  predecessor_uncertainty_evidence_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  FOREIGN KEY (decision_id, predecessor_logical_effect_id)
    REFERENCES execass_duplicate_risk_bindings(decision_id, predecessor_logical_effect_id) ON DELETE RESTRICT,
  FOREIGN KEY (decision_id, predecessor_attempt_id)
    REFERENCES execass_duplicate_risk_bindings(decision_id, predecessor_attempt_id) ON DELETE RESTRICT,
  CHECK (predecessor_logical_effect_id <> successor_logical_effect_id)
);

CREATE TABLE execass_effect_tombstones (
  tombstone_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  logical_effect_id TEXT NOT NULL UNIQUE,
  internal_idempotency_key TEXT NOT NULL UNIQUE,
  provider_identity TEXT,
  provider_idempotency_key TEXT,
  reconciliation_key TEXT,
  terminal_state TEXT NOT NULL CHECK (terminal_state IN (
    'succeeded', 'failed', 'outcome_unknown', 'reconciled_absent', 'reconciled_present', 'superseded'
  )),
  outcome_digest TEXT,
  retained_at INTEGER NOT NULL,
  UNIQUE (provider_identity, provider_idempotency_key),
  FOREIGN KEY (delegation_id, logical_effect_id)
    REFERENCES execass_logical_effects(delegation_id, logical_effect_id) ON DELETE RESTRICT
);

CREATE TABLE execass_technical_resource_quota_snapshots (
  quota_snapshot_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  effective_authority_digest TEXT NOT NULL,
  scope_key TEXT NOT NULL CHECK (scope_key = 'delegation'),
  canonical_entries_json TEXT NOT NULL CHECK (json_valid(canonical_entries_json) AND json_type(canonical_entries_json)='array'),
  canonical_entries_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (delegation_id, policy_revision, effective_authority_digest, scope_key, canonical_entries_digest),
  UNIQUE (quota_snapshot_id, delegation_id)
);

CREATE TABLE execass_technical_resource_quota_entries (
  quota_snapshot_id TEXT NOT NULL REFERENCES execass_technical_resource_quota_snapshots(quota_snapshot_id) ON DELETE RESTRICT,
  technical_resource_kind TEXT NOT NULL CHECK (technical_resource_kind IN ('tokens', 'time_ms', 'connector_calls', 'resource_units')),
  unit TEXT NOT NULL,
  amount_limit INTEGER NOT NULL CHECK (amount_limit >= 0),
  PRIMARY KEY (quota_snapshot_id, technical_resource_kind, unit),
  CHECK (
    (technical_resource_kind='tokens' AND unit='token') OR
    (technical_resource_kind='time_ms' AND unit='ms') OR
    (technical_resource_kind='connector_calls' AND length(unit)=74 AND
      substr(unit,1,10)='connector:' AND substr(unit,11) NOT GLOB '*[^0-9a-f]*') OR
    (technical_resource_kind='resource_units' AND length(unit)=73 AND
      substr(unit,1,9)='resource:' AND substr(unit,10) NOT GLOB '*[^0-9a-f]*')
  )
);

CREATE INDEX idx_execass_technical_resource_quota_entries_lookup
ON execass_technical_resource_quota_entries(quota_snapshot_id, technical_resource_kind, unit);

CREATE TABLE execass_technical_resource_requirement_sets (
  requirement_set_id TEXT PRIMARY KEY,
  quota_snapshot_id TEXT NOT NULL REFERENCES execass_technical_resource_quota_snapshots(quota_snapshot_id) ON DELETE RESTRICT,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  logical_effect_id TEXT NOT NULL,
  action_id TEXT NOT NULL,
  manifest_digest TEXT NOT NULL,
  canonical_requirements_json TEXT NOT NULL CHECK (json_valid(canonical_requirements_json) AND json_type(canonical_requirements_json)='array'),
  canonical_requirements_digest TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (logical_effect_id),
  UNIQUE (requirement_set_id, quota_snapshot_id),
  FOREIGN KEY (quota_snapshot_id, delegation_id)
    REFERENCES execass_technical_resource_quota_snapshots(quota_snapshot_id, delegation_id) ON DELETE RESTRICT,
  FOREIGN KEY (delegation_id, logical_effect_id)
    REFERENCES execass_logical_effects(delegation_id, logical_effect_id) ON DELETE RESTRICT,
  FOREIGN KEY (delegation_id, action_id)
    REFERENCES execass_action_branches(delegation_id, action_id) ON DELETE RESTRICT
);

CREATE TABLE execass_technical_resource_requirements (
  requirement_set_id TEXT NOT NULL REFERENCES execass_technical_resource_requirement_sets(requirement_set_id) ON DELETE RESTRICT,
  quota_snapshot_id TEXT NOT NULL,
  technical_resource_kind TEXT NOT NULL CHECK (technical_resource_kind IN ('tokens', 'time_ms', 'connector_calls', 'resource_units')),
  unit TEXT NOT NULL,
  amount_required INTEGER NOT NULL CHECK (amount_required > 0),
  PRIMARY KEY (requirement_set_id, technical_resource_kind, unit),
  CHECK (
    (technical_resource_kind='tokens' AND unit='token') OR
    (technical_resource_kind='time_ms' AND unit='ms') OR
    (technical_resource_kind='connector_calls' AND length(unit)=74 AND
      substr(unit,1,10)='connector:' AND substr(unit,11) NOT GLOB '*[^0-9a-f]*') OR
    (technical_resource_kind='resource_units' AND length(unit)=73 AND
      substr(unit,1,9)='resource:' AND substr(unit,10) NOT GLOB '*[^0-9a-f]*')
  ),
  FOREIGN KEY (requirement_set_id, quota_snapshot_id)
    REFERENCES execass_technical_resource_requirement_sets(requirement_set_id, quota_snapshot_id) ON DELETE RESTRICT,
  FOREIGN KEY (quota_snapshot_id, technical_resource_kind, unit)
    REFERENCES execass_technical_resource_quota_entries(quota_snapshot_id, technical_resource_kind, unit) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_technical_resource_requirements_lookup
ON execass_technical_resource_requirements(quota_snapshot_id, technical_resource_kind, unit);

CREATE TABLE execass_technical_resource_reservations (
  reservation_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  logical_effect_id TEXT NOT NULL,
  quota_snapshot_id TEXT NOT NULL,
  continuation_id TEXT NOT NULL,
  claim_event_id TEXT NOT NULL REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  claim_receipt_id TEXT NOT NULL,
  technical_resource_kind TEXT NOT NULL CHECK (technical_resource_kind IN ('tokens', 'time_ms', 'connector_calls', 'resource_units')),
  unit TEXT NOT NULL,
  amount_reserved INTEGER NOT NULL CHECK (amount_reserved > 0),
  status TEXT NOT NULL CHECK (status IN ('reserved', 'settled', 'released', 'expired', 'reconciliation_required')),
  idempotency_key TEXT NOT NULL UNIQUE,
  continuation_fencing_token INTEGER NOT NULL CHECK (continuation_fencing_token > 0),
  runtime_host_generation INTEGER NOT NULL CHECK (runtime_host_generation > 0),
  runtime_fencing_token INTEGER NOT NULL CHECK (runtime_fencing_token > 0),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  settled_at INTEGER,
  CHECK (expires_at > created_at),
  CHECK ((status IN ('settled','released','expired')) = (settled_at IS NOT NULL)),
  CHECK (
    (technical_resource_kind='tokens' AND unit='token') OR
    (technical_resource_kind='time_ms' AND unit='ms') OR
    (technical_resource_kind='connector_calls' AND length(unit)=74 AND
      substr(unit,1,10)='connector:' AND substr(unit,11) NOT GLOB '*[^0-9a-f]*') OR
    (technical_resource_kind='resource_units' AND length(unit)=73 AND
      substr(unit,1,9)='resource:' AND substr(unit,10) NOT GLOB '*[^0-9a-f]*')
  ),
  UNIQUE (delegation_id, reservation_id),
  UNIQUE (logical_effect_id, quota_snapshot_id, technical_resource_kind, unit),
  FOREIGN KEY (delegation_id, logical_effect_id)
    REFERENCES execass_logical_effects(delegation_id, logical_effect_id) ON DELETE RESTRICT,
  FOREIGN KEY (delegation_id, continuation_id)
    REFERENCES execass_continuations(delegation_id, continuation_id) ON DELETE RESTRICT,
  FOREIGN KEY (quota_snapshot_id, technical_resource_kind, unit)
    REFERENCES execass_technical_resource_quota_entries(quota_snapshot_id, technical_resource_kind, unit) ON DELETE RESTRICT
);

CREATE INDEX idx_execass_technical_resource_reservations_active
ON execass_technical_resource_reservations(quota_snapshot_id, status, expires_at);

CREATE TABLE execass_technical_resource_actuals (
  technical_resource_actual_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  reservation_id TEXT NOT NULL UNIQUE,
  amount_actual INTEGER NOT NULL CHECK (amount_actual >= 0),
  continuation_fencing_token INTEGER NOT NULL CHECK (continuation_fencing_token > 0),
  runtime_host_generation INTEGER NOT NULL CHECK (runtime_host_generation > 0),
  runtime_fencing_token INTEGER NOT NULL CHECK (runtime_fencing_token > 0),
  evidence_digest TEXT NOT NULL,
  recorded_at INTEGER NOT NULL,
  FOREIGN KEY (delegation_id, reservation_id)
    REFERENCES execass_technical_resource_reservations(delegation_id, reservation_id) ON DELETE RESTRICT
);

CREATE TABLE execass_receipts (
  receipt_id TEXT PRIMARY KEY,
  scope_kind TEXT GENERATED ALWAYS AS (
    CASE WHEN delegation_id IS NULL THEN 'runtime_host' ELSE 'delegation' END
  ) STORED,
  scope_id TEXT GENERATED ALWAYS AS (
    CASE WHEN delegation_id IS NULL THEN 'execass-runtime-host' ELSE delegation_id END
  ) STORED,
  delegation_id TEXT REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  receipt_sequence INTEGER CHECK (receipt_sequence IS NULL OR receipt_sequence > 0),
  global_sequence INTEGER NOT NULL DEFAULT 0 CHECK (global_sequence >= 0),
  append_identity TEXT,
  receipt_kind TEXT,
  causation_id TEXT NOT NULL,
  causation_event_id TEXT NOT NULL REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  parent_receipt_id TEXT REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT,
  global_parent_receipt_id TEXT REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT,
  subject_kind TEXT,
  subject_id TEXT,
  subject_revision INTEGER CHECK (subject_revision IS NULL OR subject_revision >= 0),
  actor_type TEXT NOT NULL CHECK (actor_type IN (
    'human_local', 'human_remote', 'runtime', 'worker', 'connector', 'model'
  )),
  actor_identity TEXT NOT NULL,
  actor_authority_provenance_id TEXT REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  runtime_host_generation INTEGER NOT NULL CHECK (runtime_host_generation > 0),
  runtime_host_instance_id TEXT,
  runtime_fencing_token INTEGER CHECK (runtime_fencing_token IS NULL OR runtime_fencing_token > 0),
  state_revision INTEGER NOT NULL CHECK (state_revision > 0),
  canonical_payload BLOB NOT NULL,
  serialization_version TEXT NOT NULL,
  hash_algorithm TEXT NOT NULL,
  key_id TEXT NOT NULL,
  key_generation INTEGER NOT NULL CHECK (key_generation > 0),
  previous_receipt_digest TEXT,
  global_previous_receipt_digest TEXT,
  receipt_digest TEXT NOT NULL,
  keyed_integrity_tag TEXT NOT NULL,
  previous_key_integrity_tag TEXT,
  redacted_summary TEXT NOT NULL,
  occurred_at INTEGER NOT NULL,
  committed_at INTEGER NOT NULL,
  CHECK (
    (delegation_id IS NOT NULL AND receipt_sequence IS NOT NULL)
    OR
    (delegation_id IS NULL AND receipt_sequence IS NULL
      AND subject_kind='runtime_host_generation' AND subject_id='execass-runtime-host'
      AND parent_receipt_id IS NULL
      AND previous_receipt_digest IS NULL)
  ),
  UNIQUE (delegation_id, receipt_sequence),
  UNIQUE (global_sequence),
  UNIQUE (append_identity),
  UNIQUE (causation_event_id),
  UNIQUE (receipt_digest),
  UNIQUE (key_id, key_generation, receipt_id)
);

CREATE INDEX idx_execass_receipts_causation
ON execass_receipts(causation_id, committed_at);

CREATE TABLE execass_receipt_journal_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  receipt_count INTEGER NOT NULL CHECK (receipt_count >= 0),
  receipt_head_digest TEXT,
  latest_receipt_id TEXT REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT,
  CHECK ((receipt_count = 0) = (receipt_head_digest IS NULL)),
  CHECK ((receipt_count = 0) = (latest_receipt_id IS NULL))
);

INSERT INTO execass_receipt_journal_state(singleton,receipt_count,receipt_head_digest,latest_receipt_id)
VALUES (1,0,NULL,NULL);

CREATE TABLE execass_receipt_evidence_refs (
  receipt_id TEXT NOT NULL REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT,
  ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
  authority_kind TEXT NOT NULL CHECK (authority_kind IN (
    'session', 'run', 'job', 'job_run', 'task', 'board', 'board_card',
    'mail_thread', 'mail_message', 'artifact_attachment', 'artifact_board_card_asset',
    'artifact_mail_attachment', 'security_audit_event', 'assistant_tool_call_audit', 'tool_call'
  )),
  source_id TEXT NOT NULL,
  authoritative_revision INTEGER NOT NULL CHECK (authoritative_revision = 0),
  authority_link_id TEXT NOT NULL REFERENCES execass_authority_links(link_id) ON DELETE RESTRICT,
  observation_digest TEXT NOT NULL,
  deep_link TEXT NOT NULL,
  PRIMARY KEY (receipt_id, ordinal),
  UNIQUE (receipt_id, authority_kind, source_id, authoritative_revision)
);

-- Recorder evidence is not an existing Job, tool call, or security-audit
-- authority source. This distinct reference binds one canonical receipt to
-- the imported signed record without laundering its provenance.
CREATE TABLE execass_receipt_recorder_evidence_refs (
  receipt_id TEXT PRIMARY KEY
    REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  recorder_record_digest TEXT NOT NULL UNIQUE
    REFERENCES execass_effect_recorder_evidence(recorder_record_digest) ON DELETE RESTRICT,
  linked_at INTEGER NOT NULL CHECK (linked_at > 0)
);

CREATE INDEX idx_execass_receipt_recorder_evidence_source
ON execass_receipt_recorder_evidence_refs(recorder_record_digest,receipt_id);

CREATE INDEX idx_execass_receipt_evidence_source
ON execass_receipt_evidence_refs(authority_kind,source_id,receipt_id);

CREATE TRIGGER execass_receipt_canonical_insert_guard
BEFORE INSERT ON execass_receipts
WHEN NEW.serialization_version = 'carsinos.execass.receipt.cjson.v1'
BEGIN
  SELECT CASE WHEN NEW.global_sequence != (SELECT receipt_count+1 FROM execass_receipt_journal_state WHERE singleton=1)
    THEN RAISE(ABORT, 'ExecAss global receipt sequence must be gap-free') END;
  SELECT CASE WHEN NEW.delegation_id IS NOT NULL AND NEW.receipt_sequence != (SELECT receipt_chain_count+1 FROM execass_delegations WHERE delegation_id=NEW.delegation_id)
    THEN RAISE(ABORT, 'ExecAss delegation receipt sequence must be gap-free') END;
  SELECT CASE WHEN NEW.global_previous_receipt_digest IS NOT (SELECT receipt_head_digest FROM execass_receipt_journal_state WHERE singleton=1)
    THEN RAISE(ABORT, 'ExecAss global receipt parent digest mismatch') END;
  SELECT CASE WHEN NEW.delegation_id IS NOT NULL AND NEW.previous_receipt_digest IS NOT (SELECT receipt_chain_head_digest FROM execass_delegations WHERE delegation_id=NEW.delegation_id)
    THEN RAISE(ABORT, 'ExecAss delegation receipt parent digest mismatch') END;
  SELECT CASE WHEN NEW.global_parent_receipt_id IS NOT (SELECT latest_receipt_id FROM execass_receipt_journal_state WHERE singleton=1)
    THEN RAISE(ABORT, 'ExecAss global receipt parent identity mismatch') END;
  SELECT CASE WHEN NEW.delegation_id IS NOT NULL AND NEW.parent_receipt_id IS NOT (SELECT receipt_id FROM execass_receipts WHERE delegation_id=NEW.delegation_id ORDER BY receipt_sequence DESC LIMIT 1)
    THEN RAISE(ABORT, 'ExecAss delegation receipt parent identity mismatch') END;
  SELECT CASE WHEN NEW.append_identity IS NULL OR NEW.receipt_kind IS NULL OR NEW.subject_kind IS NULL OR NEW.subject_id IS NULL OR NEW.subject_revision IS NULL
    THEN RAISE(ABORT, 'ExecAss canonical receipt binding is incomplete') END;
  SELECT CASE WHEN NEW.receipt_kind NOT IN (
    'intake','plan','amendment','decision','continuation','action','effect','verifier',
    'recovery','resume','budget','completion','terminal_correction','authority_link',
    'key_rotation','global_stop','run_control','policy','runtime_settings','runtime_recovery'
  ) THEN RAISE(ABORT, 'ExecAss canonical receipt kind is unsupported') END;
  SELECT CASE WHEN NEW.subject_kind NOT IN (
    'delegation','plan','plan_amendment','decision','continuation','action_branch',
    'verifier_result','completion_assessment','terminal_correction','authority_link',
    'recovery_evaluation','outbox_event','global_runtime_control','policy_revision',
    'runtime_settings_revision','runtime_host_generation'
  ) THEN RAISE(ABORT, 'ExecAss canonical receipt subject kind is unsupported') END;
  SELECT CASE WHEN NEW.actor_authority_provenance_id IS NULL OR NEW.runtime_host_instance_id IS NULL OR NEW.runtime_fencing_token IS NULL
    THEN RAISE(ABORT, 'ExecAss canonical actor/runtime binding is incomplete') END;
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM execass_outbox_events e WHERE e.event_id=NEW.causation_event_id
      AND e.causation_id=NEW.causation_id AND e.occurred_at=NEW.occurred_at
      AND (
        (NEW.subject_kind='global_runtime_control'
          AND e.aggregate_id='global-stop-all' AND e.aggregate_revision=NEW.subject_revision)
        OR (NEW.subject_kind='policy_revision'
          AND e.aggregate_id='execass-policy' AND e.aggregate_revision=NEW.subject_revision)
        OR (NEW.subject_kind='runtime_settings_revision'
          AND e.aggregate_id='execass-runtime-host' AND e.aggregate_revision=NEW.subject_revision)
        OR (NEW.subject_kind='runtime_host_generation'
          AND NEW.delegation_id IS NULL
          AND e.aggregate_id='execass-runtime-host' AND e.aggregate_revision=NEW.subject_revision)
        OR
        (NEW.subject_kind NOT IN ('global_runtime_control','policy_revision','runtime_settings_revision','runtime_host_generation')
          AND e.aggregate_id=NEW.delegation_id AND e.aggregate_revision=NEW.state_revision)
      )
  ) THEN RAISE(ABORT, 'ExecAss receipt must bind its exact outbox event') END;
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM execass_authority_provenance a WHERE a.authority_provenance_id=NEW.actor_authority_provenance_id
      AND a.actor_type=NEW.actor_type AND a.credential_identity=NEW.actor_identity
  ) THEN RAISE(ABORT, 'ExecAss receipt actor authority mismatch') END;
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM execass_runtime_host_generations g JOIN execass_runtime_host_leases l
      ON l.generation=g.generation AND l.host_instance_id=g.host_instance_id
    WHERE g.generation=NEW.runtime_host_generation AND g.host_instance_id=NEW.runtime_host_instance_id
      AND l.fencing_token=NEW.runtime_fencing_token
  ) THEN RAISE(ABORT, 'ExecAss receipt runtime identity mismatch') END;
END;

CREATE TRIGGER execass_receipt_journal_advance_guard
BEFORE UPDATE ON execass_receipt_journal_state
WHEN NOT (
  OLD.singleton=1 AND NEW.singleton=1 AND NEW.receipt_count=OLD.receipt_count+1
  AND EXISTS (SELECT 1 FROM execass_receipts r WHERE r.receipt_id=NEW.latest_receipt_id
    AND r.global_sequence=NEW.receipt_count AND r.receipt_digest=NEW.receipt_head_digest
    AND r.global_previous_receipt_digest IS OLD.receipt_head_digest)
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss global receipt journal may only advance by its exact next receipt');
END;

CREATE TRIGGER execass_receipt_evidence_insert_guard
BEFORE INSERT ON execass_receipt_evidence_refs
WHEN NOT EXISTS (
  SELECT 1 FROM execass_receipts r JOIN execass_authority_links l
    ON l.link_id=NEW.authority_link_id AND l.delegation_id=r.delegation_id
  WHERE r.receipt_id=NEW.receipt_id AND l.authority_kind=NEW.authority_kind
    AND l.authoritative_revision=NEW.authoritative_revision
    AND COALESCE(l.session_id,l.run_id,l.job_id,l.job_run_id,l.task_id,l.board_id,l.board_card_id,
      l.mail_thread_id,l.mail_message_id,l.attachment_id,l.board_card_asset_id,l.mail_attachment_id,
      l.security_audit_event_id,l.assistant_tool_call_audit_event_id,l.tool_call_id)=NEW.source_id
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt evidence must match an exact authority link');
END;

CREATE TRIGGER execass_receipt_evidence_immutable
BEFORE UPDATE ON execass_receipt_evidence_refs BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt evidence is immutable');
END;

CREATE TRIGGER execass_receipt_evidence_no_delete
BEFORE DELETE ON execass_receipt_evidence_refs BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt evidence cannot be deleted online');
END;

CREATE TRIGGER execass_receipt_recorder_evidence_insert_guard
BEFORE INSERT ON execass_receipt_recorder_evidence_refs
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_effect_recorder_evidence evidence
  JOIN execass_logical_effects effect
    ON effect.logical_effect_id=evidence.logical_effect_id
  WHERE evidence.recorder_record_digest=NEW.recorder_record_digest
    AND effect.delegation_id=NEW.delegation_id
    AND evidence.imported_at=NEW.linked_at
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt recorder evidence lacks exact imported provenance');
END;

CREATE TRIGGER execass_receipt_recorder_evidence_immutable
BEFORE UPDATE ON execass_receipt_recorder_evidence_refs BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt recorder evidence is immutable');
END;

CREATE TRIGGER execass_receipt_recorder_evidence_no_delete
BEFORE DELETE ON execass_receipt_recorder_evidence_refs BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt recorder evidence cannot be deleted online');
END;

CREATE TRIGGER execass_receipt_recorder_evidence_receipt_guard
AFTER INSERT ON execass_receipts
WHEN EXISTS (
  SELECT 1 FROM execass_receipt_recorder_evidence_refs ref
  WHERE ref.receipt_id=NEW.receipt_id
    AND (ref.delegation_id<>NEW.delegation_id OR ref.recorder_record_digest<>NEW.causation_id)
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt does not canonically bind its recorder evidence');
END;

CREATE TABLE execass_receipt_keys (
  key_id TEXT NOT NULL,
  key_generation INTEGER NOT NULL UNIQUE CHECK (key_generation > 0),
  status TEXT NOT NULL CHECK (status IN ('provisioned', 'active', 'retired', 'lost')),
  rotated_from_key_id TEXT,
  rotated_from_key_generation INTEGER,
  created_at INTEGER NOT NULL,
  registry_integrity_tag TEXT NOT NULL CHECK (
    length(registry_integrity_tag) = 64
    AND registry_integrity_tag = lower(registry_integrity_tag)
    AND registry_integrity_tag NOT GLOB '*[^0-9a-f]*'
  ),
  activated_anchor_generation INTEGER,
  PRIMARY KEY (key_id, key_generation),
  FOREIGN KEY (rotated_from_key_id, rotated_from_key_generation)
    REFERENCES execass_receipt_keys(key_id, key_generation),
  CHECK ((key_generation = 1) = (rotated_from_key_id IS NULL)),
  CHECK ((rotated_from_key_id IS NULL) = (rotated_from_key_generation IS NULL)),
  CHECK (
    (status = 'provisioned' AND activated_anchor_generation IS NULL)
    OR (status IN ('active', 'retired') AND activated_anchor_generation IS NOT NULL)
    OR status = 'lost'
  )
);

CREATE UNIQUE INDEX idx_execass_receipt_keys_one_active
ON execass_receipt_keys(status) WHERE status = 'active';

CREATE UNIQUE INDEX idx_execass_receipt_keys_one_provisioned
ON execass_receipt_keys(status) WHERE status = 'provisioned';

CREATE TABLE execass_receipt_anchor_state (
  anchor_id TEXT PRIMARY KEY,
  root_identity TEXT NOT NULL,
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  anchor_generation INTEGER NOT NULL CHECK (anchor_generation > 0),
  status TEXT NOT NULL CHECK (status IN ('prepared', 'finalized', 'quarantined')),
  receipt_count INTEGER NOT NULL CHECK (receipt_count >= 0),
  receipt_head_digest TEXT,
  key_id TEXT NOT NULL,
  key_generation INTEGER NOT NULL CHECK (key_generation > 0),
  transaction_id TEXT NOT NULL UNIQUE,
  external_receipt_digest TEXT NOT NULL,
  prepared_document_digest TEXT NOT NULL,
  receipt_commit_confirmed INTEGER NOT NULL DEFAULT 0 CHECK (receipt_commit_confirmed IN (0, 1)),
  receipt_committed_at INTEGER,
  receipt_commit_confirmation_tag TEXT,
  finalized_document_digest TEXT,
  prepared_at INTEGER NOT NULL,
  finalized_at INTEGER,
  quarantined_at INTEGER,
  quarantine_reason TEXT,
  FOREIGN KEY (key_id, key_generation) REFERENCES execass_receipt_keys(key_id, key_generation),
  UNIQUE (root_identity, state_root_generation, anchor_generation),
  CHECK (status != 'prepared' OR finalized_at IS NULL),
  CHECK (status != 'prepared' OR finalized_document_digest IS NULL),
  CHECK ((receipt_commit_confirmed = 1) = (receipt_committed_at IS NOT NULL)),
  CHECK ((receipt_commit_confirmed = 1) = (receipt_commit_confirmation_tag IS NOT NULL)),
  CHECK (status != 'finalized' OR finalized_at IS NOT NULL),
  CHECK (status != 'finalized' OR finalized_document_digest IS NOT NULL),
  CHECK ((status = 'quarantined') = (quarantined_at IS NOT NULL)),
  CHECK ((status = 'quarantined') = (quarantine_reason IS NOT NULL))
);

CREATE TRIGGER execass_receipt_anchor_identity_immutable
BEFORE UPDATE OF anchor_id, root_identity, state_root_generation, anchor_generation,
  receipt_count, receipt_head_digest, key_id, key_generation, transaction_id,
  external_receipt_digest, prepared_document_digest, prepared_at
ON execass_receipt_anchor_state BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt anchor identity is immutable');
END;

CREATE TRIGGER execass_receipt_keys_identity_immutable
BEFORE UPDATE OF key_id,key_generation,rotated_from_key_id,rotated_from_key_generation,created_at,registry_integrity_tag
ON execass_receipt_keys BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt key identity is immutable');
END;

CREATE TRIGGER execass_receipt_keys_transition_guard
BEFORE UPDATE ON execass_receipt_keys
WHEN NOT (
  (OLD.status = 'provisioned' AND NEW.status = 'active'
    AND NEW.activated_anchor_generation IS NOT NULL)
  OR
  (OLD.status = 'active' AND NEW.status = 'retired'
    AND NEW.activated_anchor_generation IS OLD.activated_anchor_generation)
  OR
  (OLD.status IN ('provisioned', 'active') AND NEW.status = 'lost'
    AND NEW.activated_anchor_generation IS OLD.activated_anchor_generation)
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt key transition is invalid');
END;

CREATE TRIGGER execass_receipt_keys_no_delete
BEFORE DELETE ON execass_receipt_keys BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt key history is immutable');
END;

CREATE TRIGGER execass_receipt_anchor_terminal_immutable
BEFORE UPDATE ON execass_receipt_anchor_state
WHEN OLD.status = 'quarantined' BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt anchor terminal state is immutable');
END;

CREATE TRIGGER execass_receipt_anchor_delete_guard
BEFORE DELETE ON execass_receipt_anchor_state
WHEN NOT (OLD.status = 'prepared' AND OLD.receipt_commit_confirmed = 0) BEGIN
  SELECT RAISE(ABORT, 'Only an unconfirmed prepared receipt anchor may be discarded');
END;

CREATE TRIGGER execass_receipt_anchor_transition_guard
BEFORE UPDATE ON execass_receipt_anchor_state
WHEN NOT (
  (OLD.status = 'prepared' AND NEW.status = 'prepared'
    AND OLD.receipt_commit_confirmed = 0
    AND NEW.receipt_commit_confirmed = 1
    AND NEW.receipt_committed_at IS NOT NULL
    AND NEW.receipt_commit_confirmation_tag IS NOT NULL
    AND NEW.finalized_at IS NULL
    AND NEW.finalized_document_digest IS NULL
    AND NEW.quarantined_at IS NULL
    AND NEW.quarantine_reason IS NULL)
  OR
  (OLD.status = 'prepared' AND NEW.status = 'finalized'
    AND OLD.receipt_commit_confirmed = 1
    AND NEW.receipt_commit_confirmed = 1
    AND NEW.receipt_committed_at IS OLD.receipt_committed_at
    AND NEW.receipt_commit_confirmation_tag IS OLD.receipt_commit_confirmation_tag
    AND NEW.finalized_at IS NOT NULL
    AND NEW.finalized_document_digest IS NOT NULL
    AND NEW.quarantined_at IS NULL
    AND NEW.quarantine_reason IS NULL)
  OR
  (OLD.status IN ('prepared', 'finalized') AND NEW.status = 'quarantined'
    AND NEW.receipt_commit_confirmed = OLD.receipt_commit_confirmed
    AND NEW.receipt_committed_at IS OLD.receipt_committed_at
    AND NEW.receipt_commit_confirmation_tag IS OLD.receipt_commit_confirmation_tag
    AND NEW.quarantined_at IS NOT NULL
    AND NEW.quarantine_reason IS NOT NULL
    AND NEW.finalized_at IS OLD.finalized_at
    AND NEW.finalized_document_digest IS OLD.finalized_document_digest)
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipt anchor transition is invalid');
END;

CREATE TABLE execass_outbox_events (
  global_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  event_id TEXT NOT NULL UNIQUE,
  event_name TEXT NOT NULL CHECK (event_name IN (
    'execass.v1.delegation.transitioned',
    'execass.v1.decision.recorded',
    'execass.v1.continuation.claimed_or_result_recorded',
    'execass.v1.recovery.updated',
    'execass.v1.completion.assessed',
    'execass.v1.summary.changed',
    'execass.v1.policy.changed',
    'execass.v1.runtime_host.changed',
    'execass.v1.global_stop.changed',
    'execass.v1.receipt.integrity_failed',
    'execass.v1.notification.scheduled'
  )),
  aggregate_id TEXT NOT NULL,
  aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0),
  correlation_id TEXT NOT NULL,
  causation_id TEXT NOT NULL,
  occurred_at INTEGER NOT NULL,
  schema_version TEXT NOT NULL CHECK (schema_version = 'v1'),
  safe_payload_json TEXT NOT NULL CHECK (json_valid(safe_payload_json)),
  duplicate_identity TEXT NOT NULL UNIQUE,
  published_at INTEGER,
  UNIQUE (event_name, aggregate_id, aggregate_revision, causation_id)
);

CREATE INDEX idx_execass_outbox_unpublished
ON execass_outbox_events(global_sequence) WHERE published_at IS NULL;

CREATE UNIQUE INDEX idx_execass_outbox_one_delegation_transition_per_revision
ON execass_outbox_events(aggregate_id, aggregate_revision)
WHERE event_name = 'execass.v1.delegation.transitioned';

CREATE TABLE execass_outbox_cursors (
  consumer_id TEXT PRIMARY KEY,
  principal_id TEXT NOT NULL,
  client_id_digest TEXT NOT NULL,
  last_global_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_global_sequence >= 0),
  cursor_revision INTEGER NOT NULL CHECK (cursor_revision > 0),
  updated_at INTEGER NOT NULL,
  UNIQUE (principal_id, client_id_digest)
);

CREATE TABLE execass_summary_deliveries (
  delivery_id TEXT PRIMARY KEY,
  displayed_cursor TEXT NOT NULL UNIQUE,
  projection_version TEXT NOT NULL,
  through_global_sequence INTEGER NOT NULL CHECK (through_global_sequence >= 0),
  item_set_digest TEXT NOT NULL,
  item_count INTEGER NOT NULL CHECK (item_count >= 0),
  request_correlation_id TEXT NOT NULL,
  delivered_at INTEGER NOT NULL,
  UNIQUE (delivery_id, displayed_cursor)
);

CREATE TABLE execass_summary_delivery_items (
  delivery_id TEXT NOT NULL REFERENCES execass_summary_deliveries(delivery_id) ON DELETE RESTRICT,
  item_id TEXT NOT NULL,
  item_revision INTEGER NOT NULL CHECK (item_revision > 0),
  projection_kind TEXT NOT NULL CHECK (projection_kind IN ('needs_you', 'in_motion', 'done', 'next', 'receipts')),
  PRIMARY KEY (delivery_id, item_id)
);

CREATE TRIGGER execass_summary_delivery_items_count_guard
BEFORE INSERT ON execass_summary_delivery_items
WHEN (SELECT COUNT(*) FROM execass_summary_delivery_items WHERE delivery_id=NEW.delivery_id)
     >= (SELECT item_count FROM execass_summary_deliveries WHERE delivery_id=NEW.delivery_id)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss delivered item set is sealed');
END;

CREATE TABLE execass_summary_acknowledgements (
  acknowledgement_id TEXT PRIMARY KEY,
  delivery_id TEXT NOT NULL,
  displayed_cursor TEXT NOT NULL,
  acknowledged_items_digest TEXT NOT NULL,
  idempotency_key TEXT NOT NULL UNIQUE,
  acknowledged_at INTEGER NOT NULL,
  UNIQUE (delivery_id, displayed_cursor),
  FOREIGN KEY (delivery_id, displayed_cursor)
    REFERENCES execass_summary_deliveries(delivery_id, displayed_cursor) ON DELETE RESTRICT
);

CREATE TABLE execass_notifications (
  notification_id TEXT PRIMARY KEY,
  attention_id TEXT REFERENCES execass_attention_items(attention_id) ON DELETE RESTRICT,
  completion_assessment_id TEXT REFERENCES execass_completion_assessments(assessment_id) ON DELETE RESTRICT,
  outbox_event_id TEXT NOT NULL UNIQUE REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  decision_id TEXT REFERENCES execass_decisions(decision_id) ON DELETE RESTRICT,
  reason_revision INTEGER NOT NULL CHECK (reason_revision > 0),
  attention_variant TEXT CHECK (attention_variant IS NULL OR attention_variant IN (
    'confirmation', 'clarification', 'reply', 'recovery_choice'
  )),
  reason TEXT NOT NULL,
  channel TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('scheduled', 'dispatched', 'cancelled', 'failed')),
  safe_payload_json TEXT NOT NULL CHECK (json_valid(safe_payload_json)),
  requested_at INTEGER NOT NULL,
  scheduled_at INTEGER NOT NULL,
  next_reminder_at INTEGER,
  quiet_hours_json TEXT CHECK (quiet_hours_json IS NULL OR json_valid(quiet_hours_json)),
  reminder_count INTEGER NOT NULL DEFAULT 0 CHECK (reminder_count BETWEEN 0 AND 3),
  last_reminded_at INTEGER,
  dispatched_at INTEGER,
  updated_at INTEGER NOT NULL,
  idempotency_key TEXT NOT NULL UNIQUE,
  CHECK ((attention_id IS NOT NULL) != (completion_assessment_id IS NOT NULL))
);

CREATE INDEX idx_execass_notifications_due
ON execass_notifications(status, scheduled_at, next_reminder_at);

CREATE UNIQUE INDEX idx_execass_notifications_dedupe
ON execass_notifications(
  delegation_id, COALESCE(decision_id, ''), reason_revision, channel
);

CREATE TABLE execass_runtime_host_generations (
  generation INTEGER PRIMARY KEY AUTOINCREMENT,
  ownership_scope TEXT NOT NULL,
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  installation_identity TEXT NOT NULL,
  os_user_identity_digest TEXT NOT NULL,
  host_instance_id TEXT NOT NULL UNIQUE,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  end_reason TEXT,
  UNIQUE (ownership_scope, generation),
  UNIQUE (generation, host_instance_id)
);

CREATE TABLE execass_runtime_host_leases (
  lease_id TEXT PRIMARY KEY,
  ownership_scope TEXT NOT NULL,
  generation INTEGER NOT NULL,
  host_instance_id TEXT NOT NULL,
  fencing_token INTEGER NOT NULL CHECK (fencing_token > 0),
  acquired_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  released_at INTEGER,
  CHECK (expires_at > acquired_at),
  UNIQUE (ownership_scope, generation, fencing_token),
  FOREIGN KEY (generation, host_instance_id)
    REFERENCES execass_runtime_host_generations(generation, host_instance_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX idx_execass_runtime_host_one_live_lease
ON execass_runtime_host_leases(ownership_scope) WHERE released_at IS NULL;

CREATE TABLE execass_runtime_host_states (
  generation INTEGER NOT NULL,
  host_instance_id TEXT NOT NULL,
  fencing_token INTEGER NOT NULL CHECK (fencing_token > 0),
  state_root_generation INTEGER NOT NULL CHECK (state_root_generation > 0),
  actual_state TEXT NOT NULL CHECK (actual_state IN (
    'stopped','starting','running_app_bound','handoff',
    'running_background','draining','faulted'
  )),
  updated_at INTEGER NOT NULL CHECK (updated_at > 0),
  PRIMARY KEY (generation, host_instance_id, fencing_token),
  FOREIGN KEY (generation, host_instance_id)
    REFERENCES execass_runtime_host_generations(generation, host_instance_id) ON DELETE RESTRICT
);

-- The one canonical, append-only operational policy authority. Revision 1 is
-- the unconfigured technical bootstrap; every later revision is an exact
-- authenticated owner snapshot committed with its outbox event and receipt.
CREATE TABLE execass_policy_revisions (
  policy_revision INTEGER PRIMARY KEY CHECK (policy_revision > 0),
  idempotency_key TEXT UNIQUE,
  policy_snapshot_json TEXT NOT NULL CHECK (
    json_valid(policy_snapshot_json) AND json_type(policy_snapshot_json) = 'object'
  ),
  policy_snapshot_digest TEXT NOT NULL CHECK (
    length(policy_snapshot_digest) = 64 AND policy_snapshot_digest NOT GLOB '*[^0-9a-f]*'
  ),
  request_digest TEXT UNIQUE CHECK (
    request_digest IS NULL OR
    (length(request_digest) = 64 AND request_digest NOT GLOB '*[^0-9a-f]*')
  ),
  authority_provenance_id TEXT NOT NULL UNIQUE
    REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  outbox_event_id TEXT UNIQUE
    REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  receipt_id TEXT UNIQUE
    REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  created_at INTEGER NOT NULL CHECK (created_at > 0),
  CHECK (
    (policy_revision = 1 AND idempotency_key IS NULL AND request_digest IS NULL
      AND outbox_event_id IS NULL AND receipt_id IS NULL)
    OR
    (policy_revision > 1 AND length(trim(idempotency_key)) > 0
      AND request_digest IS NOT NULL AND outbox_event_id IS NOT NULL AND receipt_id IS NOT NULL)
  )
);

CREATE TABLE execass_global_runtime_control (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  engaged INTEGER NOT NULL CHECK (engaged IN (0, 1)),
  global_stop_epoch INTEGER NOT NULL CHECK (global_stop_epoch >= 0),
  current_policy_revision INTEGER NOT NULL CHECK (current_policy_revision > 0),
  updated_at INTEGER NOT NULL
);

-- This fixed, non-runnable record is the only receipt-chain carrier for
-- machine-wide controls. It is seeded so emergency stop-all works before the
-- first user delegation exists; callers never select a user delegation.
INSERT INTO execass_authority_provenance(
  authority_provenance_id,actor_type,credential_identity,authenticated_ingress,channel_assurance,
  source_correlation_id,authority_kind,normalized_scope_json,policy_revision,evidence_digest,created_at
) VALUES(
  'execass-global-control-carrier-authority','runtime','execass-global-control','internal-runtime','runtime-safety',
  'execass-global-control-init','runtime_safety_state','{}',1,
  '0000000000000000000000000000000000000000000000000000000000000000',1
);

INSERT INTO execass_policy_revisions(
  policy_revision,policy_snapshot_json,policy_snapshot_digest,
  authority_provenance_id,created_at
) VALUES(
  1,'{"configured":false,"profile":null}',
  '3b92dfe651382c3edee23b78d327a58c0511064ac93dcec60fe4a10e2cf91738',
  'execass-global-control-carrier-authority',1
);

INSERT INTO execass_delegations(
  delegation_id,normalized_original_intent,intake_evidence_json,ingress_source,ingress_credential_identity,
  source_correlation_id,ingress_idempotency_key,classifier_version,classifier_reasons_json,phase,run_control,
  state_revision,policy_revision,effective_authority_json,authority_provenance_id,stop_epoch,
  receipt_chain_count,created_at,updated_at
) VALUES(
  'execass-global-control-carrier','internal global runtime control receipt carrier','{}','internal-runtime',
  'execass-global-control','execass-global-control-init','execass-global-control-init','v1','["internal_control"]',
  'accepted','running',1,1,'{}','execass-global-control-carrier-authority',0,0,1,1
);

INSERT INTO execass_global_runtime_control(singleton, engaged, global_stop_epoch, current_policy_revision, updated_at)
VALUES (1, 0, 0, 1, 0);

CREATE TRIGGER execass_global_runtime_control_transition_guard
BEFORE UPDATE ON execass_global_runtime_control
WHEN NEW.singleton != OLD.singleton
  OR NEW.global_stop_epoch < OLD.global_stop_epoch
  OR NEW.updated_at < OLD.updated_at
  OR NEW.current_policy_revision < OLD.current_policy_revision
  OR NEW.current_policy_revision > OLD.current_policy_revision + 1
  OR (
    NEW.current_policy_revision = OLD.current_policy_revision + 1
    AND NOT EXISTS (
      SELECT 1 FROM execass_policy_revisions policy
      WHERE policy.policy_revision=NEW.current_policy_revision
    )
  )
  OR (
    OLD.engaged = 0 AND NEW.engaged = 1
    AND NEW.global_stop_epoch != OLD.global_stop_epoch + 1
  )
  OR (
    OLD.engaged = 1 AND NEW.engaged = 0
    AND NEW.global_stop_epoch != OLD.global_stop_epoch
  )
  OR (
    NEW.engaged = OLD.engaged
    AND NEW.global_stop_epoch != OLD.global_stop_epoch
  )
BEGIN
  SELECT RAISE(ABORT, 'ExecAss global runtime control transition is invalid');
END;

CREATE TRIGGER execass_global_runtime_control_no_delete
BEFORE DELETE ON execass_global_runtime_control
BEGIN
  SELECT RAISE(ABORT, 'ExecAss global runtime control cannot be deleted');
END;

CREATE TABLE execass_runtime_settings_revisions (
  settings_revision INTEGER PRIMARY KEY AUTOINCREMENT,
  desired_mode TEXT NOT NULL CHECK (desired_mode IN ('app_bound', 'background')),
  actual_state TEXT NOT NULL CHECK (actual_state IN (
    'stopped', 'starting', 'running_app_bound', 'handoff', 'running_background', 'draining', 'faulted'
  )),
  start_at_login INTEGER NOT NULL CHECK (start_at_login IN (0, 1)),
  settings_json TEXT NOT NULL CHECK (
    json_valid(settings_json) AND json_type(settings_json) = 'object'
  ),
  settings_digest TEXT NOT NULL CHECK (
    length(settings_digest) = 64 AND settings_digest NOT GLOB '*[^0-9a-f]*'
  ),
  idempotency_key TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
  request_digest TEXT NOT NULL UNIQUE CHECK (
    length(request_digest) = 64 AND request_digest NOT GLOB '*[^0-9a-f]*'
  ),
  policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
  authority_provenance_id TEXT NOT NULL UNIQUE REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  outbox_event_id TEXT NOT NULL UNIQUE REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  receipt_id TEXT NOT NULL UNIQUE REFERENCES execass_receipts(receipt_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  created_at INTEGER NOT NULL CHECK (created_at > 0),
  CHECK (start_at_login = 0 OR desired_mode = 'background')
);

-- One reference table links orchestration to existing authoritative records.
-- It stores no copied status, payload, or authority state.
CREATE TABLE execass_authority_links (
  link_id TEXT PRIMARY KEY,
  delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  link_revision INTEGER NOT NULL CHECK (link_revision > 0),
  delegation_state_revision INTEGER NOT NULL CHECK (delegation_state_revision > 0),
  correlation_id TEXT NOT NULL,
  causation_id TEXT NOT NULL,
  outbox_event_id TEXT NOT NULL REFERENCES execass_outbox_events(event_id) ON DELETE RESTRICT,
  authority_kind TEXT NOT NULL CHECK (authority_kind IN (
    'session', 'run', 'job', 'job_run', 'task', 'board', 'board_card',
    'mail_thread', 'mail_message', 'artifact_attachment', 'artifact_board_card_asset',
    'artifact_mail_attachment', 'security_audit_event', 'assistant_tool_call_audit', 'tool_call'
  )),
  session_id TEXT REFERENCES sessions(session_id) ON DELETE RESTRICT,
  run_id TEXT REFERENCES runs(run_id) ON DELETE RESTRICT,
  job_id TEXT REFERENCES jobs(job_id) ON DELETE RESTRICT,
  job_run_id TEXT REFERENCES job_runs(job_run_id) ON DELETE RESTRICT,
  task_id TEXT REFERENCES tasks(task_id) ON DELETE RESTRICT,
  board_id TEXT REFERENCES boards(board_id) ON DELETE RESTRICT,
  board_card_id TEXT REFERENCES board_cards(card_id) ON DELETE RESTRICT,
  mail_thread_id TEXT REFERENCES agent_mail_threads(thread_id) ON DELETE RESTRICT,
  mail_message_id TEXT REFERENCES agent_mail_messages(message_id) ON DELETE RESTRICT,
  attachment_id TEXT REFERENCES attachments(attachment_id) ON DELETE RESTRICT,
  board_card_asset_id TEXT REFERENCES board_card_assets(card_asset_id) ON DELETE RESTRICT,
  mail_attachment_id TEXT REFERENCES agent_mail_attachments(attachment_id) ON DELETE RESTRICT,
  -- A security audit event is retention-moved from the live table to the
  -- archive table.  Its existence is enforced by lineage triggers below, not
  -- a live-table FK, so the immutable reference remains resolvable.
  security_audit_event_id TEXT,
  assistant_tool_call_audit_event_id TEXT REFERENCES assistant_tool_calls_audit(event_id) ON DELETE RESTRICT,
  tool_call_id TEXT REFERENCES tool_calls(tool_call_id) ON DELETE RESTRICT,
  authoritative_revision INTEGER NOT NULL CHECK (authoritative_revision = 0),
  linked_at INTEGER NOT NULL,
  UNIQUE (delegation_id, link_revision),
  CHECK (
    (authority_kind = 'session' AND session_id IS NOT NULL) OR
    (authority_kind = 'run' AND run_id IS NOT NULL) OR
    (authority_kind = 'job' AND job_id IS NOT NULL) OR
    (authority_kind = 'job_run' AND job_run_id IS NOT NULL) OR
    (authority_kind = 'task' AND task_id IS NOT NULL) OR
    (authority_kind = 'board' AND board_id IS NOT NULL) OR
    (authority_kind = 'board_card' AND board_card_id IS NOT NULL) OR
    (authority_kind = 'mail_thread' AND mail_thread_id IS NOT NULL) OR
    (authority_kind = 'mail_message' AND mail_message_id IS NOT NULL) OR
    (authority_kind = 'artifact_attachment' AND attachment_id IS NOT NULL) OR
    (authority_kind = 'artifact_board_card_asset' AND board_card_asset_id IS NOT NULL) OR
    (authority_kind = 'artifact_mail_attachment' AND mail_attachment_id IS NOT NULL) OR
    (authority_kind = 'security_audit_event' AND security_audit_event_id IS NOT NULL) OR
    (authority_kind = 'assistant_tool_call_audit' AND assistant_tool_call_audit_event_id IS NOT NULL) OR
    (authority_kind = 'tool_call' AND tool_call_id IS NOT NULL)
  ),
  CHECK (
    (session_id IS NOT NULL) + (run_id IS NOT NULL) + (job_id IS NOT NULL) +
    (job_run_id IS NOT NULL) + (task_id IS NOT NULL) + (board_id IS NOT NULL) +
    (board_card_id IS NOT NULL) + (mail_thread_id IS NOT NULL) +
    (mail_message_id IS NOT NULL) + (attachment_id IS NOT NULL) +
    (board_card_asset_id IS NOT NULL) + (mail_attachment_id IS NOT NULL) +
    (security_audit_event_id IS NOT NULL) + (assistant_tool_call_audit_event_id IS NOT NULL) +
    (tool_call_id IS NOT NULL) = 1
  )
);

CREATE INDEX idx_execass_authority_links_delegation_kind
ON execass_authority_links(delegation_id, authority_kind, authoritative_revision DESC);

CREATE INDEX idx_execass_authority_links_outbox
ON execass_authority_links(outbox_event_id, delegation_id, link_revision);

CREATE UNIQUE INDEX idx_execass_authority_link_session_observation
ON execass_authority_links(delegation_id, authority_kind, session_id, authoritative_revision) WHERE session_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_run_observation
ON execass_authority_links(delegation_id, authority_kind, run_id, authoritative_revision) WHERE run_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_job_observation
ON execass_authority_links(delegation_id, authority_kind, job_id, authoritative_revision) WHERE job_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_job_run_observation
ON execass_authority_links(delegation_id, authority_kind, job_run_id, authoritative_revision) WHERE job_run_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_task_observation
ON execass_authority_links(delegation_id, authority_kind, task_id, authoritative_revision) WHERE task_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_board_observation
ON execass_authority_links(delegation_id, authority_kind, board_id, authoritative_revision) WHERE board_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_board_card_observation
ON execass_authority_links(delegation_id, authority_kind, board_card_id, authoritative_revision) WHERE board_card_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_mail_thread_observation
ON execass_authority_links(delegation_id, authority_kind, mail_thread_id, authoritative_revision) WHERE mail_thread_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_mail_message_observation
ON execass_authority_links(delegation_id, authority_kind, mail_message_id, authoritative_revision) WHERE mail_message_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_attachment_observation
ON execass_authority_links(delegation_id, authority_kind, attachment_id, authoritative_revision) WHERE attachment_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_board_asset_observation
ON execass_authority_links(delegation_id, authority_kind, board_card_asset_id, authoritative_revision) WHERE board_card_asset_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_mail_attachment_observation
ON execass_authority_links(delegation_id, authority_kind, mail_attachment_id, authoritative_revision) WHERE mail_attachment_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_security_audit_observation
ON execass_authority_links(delegation_id, authority_kind, security_audit_event_id, authoritative_revision) WHERE security_audit_event_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_assistant_audit_observation
ON execass_authority_links(delegation_id, authority_kind, assistant_tool_call_audit_event_id, authoritative_revision) WHERE assistant_tool_call_audit_event_id IS NOT NULL;
CREATE UNIQUE INDEX idx_execass_authority_link_tool_call_observation
ON execass_authority_links(delegation_id, authority_kind, tool_call_id, authoritative_revision) WHERE tool_call_id IS NOT NULL;

-- Immutable adapter observation of the authoritative source's parent identity.
-- Root/shareable authorities intentionally have no row here.
CREATE TABLE execass_authority_parent_bindings (
  link_id TEXT PRIMARY KEY REFERENCES execass_authority_links(link_id) ON DELETE RESTRICT,
  owner_kind TEXT NOT NULL CHECK (owner_kind IN (
    'agent','session','run','job','project','board','board_card','message','mail_thread','mail_message'
  )),
  expected_owner_id TEXT NOT NULL CHECK (length(trim(expected_owner_id)) > 0)
);

CREATE TRIGGER execass_authority_parent_binding_kind
BEFORE INSERT ON execass_authority_parent_bindings
WHEN NOT EXISTS (
  SELECT 1 FROM execass_authority_links l WHERE l.link_id=NEW.link_id AND (
    (l.authority_kind IN ('session','job') AND NEW.owner_kind='agent') OR
    (l.authority_kind IN ('run','assistant_tool_call_audit') AND NEW.owner_kind='session') OR
    (l.authority_kind='job_run' AND NEW.owner_kind='job') OR
    (l.authority_kind='task' AND NEW.owner_kind='project') OR
    (l.authority_kind='board_card' AND NEW.owner_kind='board') OR
    (l.authority_kind='mail_message' AND NEW.owner_kind='mail_thread') OR
    (l.authority_kind='artifact_attachment' AND NEW.owner_kind='message') OR
    (l.authority_kind='artifact_board_card_asset' AND NEW.owner_kind='board_card') OR
    (l.authority_kind='artifact_mail_attachment' AND NEW.owner_kind='mail_message') OR
    (l.authority_kind='tool_call' AND NEW.owner_kind='run')
  )
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss authority parent binding kind mismatch');
END;

CREATE TRIGGER execass_authority_parent_bindings_immutable
BEFORE UPDATE ON execass_authority_parent_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss authority parent bindings are immutable');
END;

CREATE TRIGGER execass_authority_parent_bindings_no_delete
BEFORE DELETE ON execass_authority_parent_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss authority parent bindings cannot be deleted online');
END;

CREATE TRIGGER execass_authority_provenance_immutable
BEFORE UPDATE ON execass_authority_provenance BEGIN
  SELECT RAISE(ABORT, 'ExecAss authority provenance is immutable');
END;

CREATE TRIGGER execass_authority_provenance_no_delete
BEFORE DELETE ON execass_authority_provenance BEGIN
  SELECT RAISE(ABORT, 'ExecAss authority provenance cannot be deleted online');
END;

CREATE TRIGGER execass_confirmation_authority_keys_immutable
BEFORE UPDATE ON execass_confirmation_authority_keys BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation authority keys are immutable');
END;

CREATE TRIGGER execass_run_control_attestation_insert_binding
BEFORE INSERT ON execass_run_control_attestations
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_confirmation_authority_keys pinned
  JOIN execass_authority_provenance authority
    ON authority.authority_provenance_id=NEW.authority_provenance_id
  JOIN execass_owner_ingress_bindings ingress
    ON ingress.actor_type=NEW.actor_type
   AND ingress.credential_identity=NEW.credential_identity
   AND ingress.authenticated_ingress=NEW.authenticated_ingress
   AND ingress.channel_assurance=NEW.channel_assurance
   AND ingress.status='active'
  WHERE pinned.key_id=NEW.pinned_key_id
    AND pinned.key_generation=NEW.pinned_key_generation
    AND pinned.status='active'
    AND pinned.canonical_root_identity=NEW.canonical_root_identity
    AND pinned.installation_identity=NEW.installation_identity
    AND pinned.os_user_identity_digest=NEW.os_user_identity_digest
    AND pinned.state_root_generation=NEW.state_root_generation
    AND authority.actor_type=NEW.actor_type
    AND authority.credential_identity=NEW.credential_identity
    AND authority.authenticated_ingress=NEW.authenticated_ingress
    AND authority.channel_assurance=NEW.channel_assurance
    AND authority.source_correlation_id=NEW.request_correlation_id
    AND authority.source_message_id IS NEW.source_message_id
    AND authority.authority_kind='run_control_attestation'
    AND authority.normalized_scope_json=NEW.normalized_scope_json
    AND authority.policy_revision=NEW.policy_revision
    AND authority.evidence_digest=NEW.attestation_digest
    AND authority.created_at=NEW.issued_at
    AND authority.expires_at=NEW.observed_at + 60000
    AND ((NEW.actor_type='human_local' AND ingress.provider_event_required=0)
      OR (NEW.actor_type='human_remote' AND ingress.provider_event_required=1))
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss run-control attestation lacks exact pinned custody and provenance binding');
END;

CREATE TRIGGER execass_run_control_attestations_immutable
BEFORE UPDATE ON execass_run_control_attestations BEGIN
  SELECT RAISE(ABORT, 'ExecAss run-control attestations are immutable');
END;

CREATE TRIGGER execass_run_control_attestations_no_delete
BEFORE DELETE ON execass_run_control_attestations BEGIN
  SELECT RAISE(ABORT, 'ExecAss run-control attestations cannot be deleted');
END;

CREATE TRIGGER execass_confirmation_authority_keys_no_delete
BEFORE DELETE ON execass_confirmation_authority_keys BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation authority keys cannot be deleted online');
END;

CREATE TRIGGER execass_owner_ingress_bindings_immutable
BEFORE UPDATE ON execass_owner_ingress_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss owner ingress bindings are immutable');
END;

CREATE TRIGGER execass_owner_ingress_bindings_no_delete
BEFORE DELETE ON execass_owner_ingress_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss owner ingress bindings cannot be deleted online');
END;

CREATE TRIGGER execass_channel_reply_bindings_immutable
BEFORE UPDATE ON execass_channel_reply_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss channel reply bindings are immutable');
END;

CREATE TRIGGER execass_channel_reply_bindings_no_delete
BEFORE DELETE ON execass_channel_reply_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss channel reply bindings cannot be deleted online');
END;

CREATE TRIGGER execass_confirmation_attestation_insert_binding
BEFORE INSERT ON execass_confirmation_attestations
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_decisions AS decision
  JOIN execass_confirmation_challenges AS challenge
    ON challenge.decision_id = decision.decision_id
  JOIN execass_confirmation_challenge_alternatives AS alternative
    ON alternative.challenge_id = challenge.challenge_id
   AND alternative.logical_action_id = NEW.selected_logical_action_id
  JOIN execass_authority_provenance AS authority
    ON authority.authority_provenance_id = NEW.authority_provenance_id
  JOIN execass_confirmation_authority_keys AS pinned_key
    ON pinned_key.key_id = NEW.pinned_key_id
   AND pinned_key.key_generation = NEW.pinned_key_generation
  JOIN execass_owner_ingress_bindings AS ingress
    ON ingress.actor_type = NEW.actor_type
   AND ingress.credential_identity = NEW.credential_identity
   AND ingress.authenticated_ingress = NEW.authenticated_ingress
   AND ingress.channel_assurance = NEW.channel_assurance
  WHERE decision.decision_id = NEW.decision_id
    AND decision.status = 'pending'
    AND decision.decision_kind = 'dangerous_action_confirmation'
    AND challenge.status = 'pending'
    AND pinned_key.status = 'active'
    AND ingress.status = 'active'
    AND json_extract(NEW.signed_payload_json, '$.actor_type') = NEW.actor_type
    AND json_extract(NEW.signed_payload_json, '$.credential_identity') = NEW.credential_identity
    AND json_extract(NEW.signed_payload_json, '$.authenticated_ingress') = NEW.authenticated_ingress
    AND json_extract(NEW.signed_payload_json, '$.channel_assurance') = NEW.channel_assurance
    AND json_extract(NEW.signed_payload_json, '$.request_correlation_id') = NEW.request_correlation_id
    AND json_extract(NEW.signed_payload_json, '$.source_message_id') IS NEW.source_message_id
    AND json_extract(NEW.signed_payload_json, '$.provider_event_id') IS NEW.provider_event_id
    AND json_extract(NEW.signed_payload_json, '$.decision_id') = NEW.decision_id
    AND json_extract(NEW.signed_payload_json, '$.decision_revision') = decision.decision_revision
    AND json_extract(NEW.signed_payload_json, '$.decision_result') = 'confirm_and_continue'
    AND json_extract(NEW.signed_payload_json, '$.policy_revision') = decision.policy_revision
    AND json_extract(NEW.signed_payload_json, '$.canonical_manifest_digest') = challenge.manifest_digest
    AND json_extract(NEW.signed_payload_json, '$.selected_logical_action_id') = NEW.selected_logical_action_id
    AND json_extract(NEW.signed_payload_json, '$.challenge_nonce_digest') = challenge.nonce_digest
    AND json_extract(NEW.signed_payload_json, '$.challenge_expires_at_ms') = challenge.expires_at
    AND json_extract(NEW.signed_payload_json, '$.issued_at_ms') = NEW.issued_at
    AND json_extract(NEW.signed_payload_json, '$.canonical_root_identity') = pinned_key.canonical_root_identity
    AND json_extract(NEW.signed_payload_json, '$.installation_identity') = pinned_key.installation_identity
    AND json_extract(NEW.signed_payload_json, '$.os_user_identity_digest') = pinned_key.os_user_identity_digest
    AND json_extract(NEW.signed_payload_json, '$.state_root_generation') = pinned_key.state_root_generation
    AND json_extract(NEW.signed_payload_json, '$.signer_key_generation') = pinned_key.key_generation
    AND authority.actor_type = NEW.actor_type
    AND authority.credential_identity = NEW.credential_identity
    AND authority.authenticated_ingress = NEW.authenticated_ingress
    AND authority.channel_assurance = NEW.channel_assurance
    AND authority.source_correlation_id = NEW.request_correlation_id
    AND authority.source_message_id IS NEW.source_message_id
    AND authority.authority_kind = 'decision_resolution'
    AND authority.policy_revision = decision.policy_revision
    AND authority.bound_decision_id = NEW.decision_id
    AND authority.bound_decision_revision = decision.decision_revision
    AND authority.bound_manifest_digest = challenge.manifest_digest
    AND authority.bound_challenge_nonce_digest = challenge.nonce_digest
    AND authority.evidence_digest = NEW.attestation_digest
    AND authority.created_at = NEW.issued_at
    AND authority.expires_at = NEW.expires_at
    AND (ingress.provider_event_required = 0 OR NEW.provider_event_id IS NOT NULL)
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation attestation lacks exact pinned authority binding');
END;

CREATE TRIGGER execass_confirmation_attestations_immutable
BEFORE UPDATE ON execass_confirmation_attestations BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation attestations are immutable');
END;

CREATE TRIGGER execass_confirmation_attestations_no_delete
BEFORE DELETE ON execass_confirmation_attestations BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation attestations cannot be deleted online');
END;

CREATE TRIGGER execass_delegation_revision_monotonic
BEFORE UPDATE ON execass_delegations
WHEN NEW.state_revision < OLD.state_revision
  OR (
    NEW.state_revision = OLD.state_revision
    AND NOT EXISTS (
      SELECT 1 FROM execass_lifecycle_transitions AS history
      WHERE history.delegation_id = NEW.delegation_id
        AND history.state_revision = NEW.state_revision
        AND json_extract(history.projection_snapshot_json, '$.phase') = NEW.phase
        AND json_extract(history.projection_snapshot_json, '$.run_control') = NEW.run_control
        AND json_extract(history.projection_snapshot_json, '$.current_plan_revision') IS NEW.current_plan_revision
        AND json_extract(history.projection_snapshot_json, '$.current_criteria_revision') IS NEW.current_criteria_revision
        AND json_extract(history.projection_snapshot_json, '$.pending_decision_id') IS NEW.pending_decision_id
        AND json_extract(history.projection_snapshot_json, '$.external_wait_json') IS NEW.external_wait_json
        AND json_extract(history.projection_snapshot_json, '$.completion_assessment_json') IS NEW.completion_assessment_json
        AND json_extract(history.projection_snapshot_json, '$.stop_epoch') = NEW.stop_epoch
        AND json_extract(history.projection_snapshot_json, '$.updated_at') = NEW.updated_at
        AND json_extract(history.projection_snapshot_json, '$.terminal_at') IS NEW.terminal_at
    )
    AND NOT (
      NEW.phase = OLD.phase
      AND NEW.run_control = OLD.run_control
      AND NEW.current_plan_revision IS OLD.current_plan_revision
      AND NEW.current_criteria_revision IS OLD.current_criteria_revision
      AND NEW.policy_revision = OLD.policy_revision
      AND NEW.effective_authority_json = OLD.effective_authority_json
      AND NEW.pending_decision_id IS OLD.pending_decision_id
      AND NEW.external_wait_json IS OLD.external_wait_json
      AND NEW.stop_epoch = OLD.stop_epoch
      AND NEW.completion_assessment_json IS OLD.completion_assessment_json
      AND NEW.receipt_chain_count = OLD.receipt_chain_count + 1
      AND NEW.updated_at = OLD.updated_at
      AND NEW.acknowledged_at IS OLD.acknowledged_at
      AND NEW.terminal_at IS OLD.terminal_at
      AND EXISTS (
        SELECT 1 FROM execass_receipts r
        WHERE r.delegation_id=NEW.delegation_id
          AND r.receipt_sequence=NEW.receipt_chain_count
          AND r.receipt_digest=NEW.receipt_chain_head_digest
          AND r.previous_receipt_digest IS OLD.receipt_chain_head_digest
      )
    )
  )
  OR NEW.delegation_id != OLD.delegation_id
  OR NEW.normalized_original_intent != OLD.normalized_original_intent
  OR NEW.intake_evidence_json != OLD.intake_evidence_json
  OR NEW.ingress_source != OLD.ingress_source
  OR NEW.ingress_credential_identity != OLD.ingress_credential_identity
  OR NEW.source_correlation_id != OLD.source_correlation_id
  OR NEW.ingress_idempotency_key != OLD.ingress_idempotency_key
  OR NEW.classifier_version != OLD.classifier_version
  OR NEW.classifier_reasons_json != OLD.classifier_reasons_json
  OR NEW.authority_provenance_id != OLD.authority_provenance_id
  OR NEW.created_at != OLD.created_at BEGIN
  SELECT RAISE(ABORT, 'ExecAss delegation state revision must increase');
END;

CREATE TRIGGER execass_plans_immutable
BEFORE UPDATE ON execass_plans BEGIN
  SELECT RAISE(ABORT, 'ExecAss plans are immutable');
END;

CREATE TRIGGER execass_plans_no_delete
BEFORE DELETE ON execass_plans BEGIN
  SELECT RAISE(ABORT, 'ExecAss plans cannot be deleted online');
END;

CREATE TRIGGER execass_plan_amendments_immutable
BEFORE UPDATE ON execass_plan_amendments BEGIN
  SELECT RAISE(ABORT, 'ExecAss plan amendments are immutable');
END;

CREATE TRIGGER execass_plan_amendments_no_delete
BEFORE DELETE ON execass_plan_amendments BEGIN
  SELECT RAISE(ABORT, 'ExecAss plan amendments cannot be deleted online');
END;

CREATE TRIGGER execass_outcome_criteria_immutable
BEFORE UPDATE ON execass_outcome_criteria BEGIN
  SELECT RAISE(ABORT, 'ExecAss outcome criteria are immutable');
END;

CREATE TRIGGER execass_outcome_criteria_no_delete
BEFORE DELETE ON execass_outcome_criteria BEGIN
  SELECT RAISE(ABORT, 'ExecAss outcome criteria cannot be deleted online');
END;

CREATE TRIGGER execass_verifier_results_immutable
BEFORE UPDATE ON execass_verifier_results BEGIN
  SELECT RAISE(ABORT, 'ExecAss verifier results are immutable');
END;

CREATE TRIGGER execass_verifier_results_no_delete
BEFORE DELETE ON execass_verifier_results BEGIN
  SELECT RAISE(ABORT, 'ExecAss verifier results cannot be deleted online');
END;

CREATE TRIGGER execass_decision_binding_immutable
BEFORE UPDATE OF decision_id, delegation_id, decision_revision, delegation_revision, plan_revision,
  policy_revision, decision_kind, exact_presented_action_json, manifest_digest, payload_digest,
  confirmed_logical_action_identity, payload_and_material_operands_json,
  target_audience_path_json, connector_tool_identity, connector_tool_version,
  side_effect_envelope_json, recommendation, consequence, alternatives_json,
  idempotency_key, requested_at
ON execass_decisions
WHEN NOT (
  OLD.decision_kind = 'dangerous_action_confirmation'
  AND OLD.status = 'pending'
  AND NEW.status = 'resolved'
  AND NEW.result = 'confirm_and_continue'
  AND EXISTS (
    SELECT 1 FROM execass_confirmation_challenge_alternatives AS alternative
    JOIN execass_confirmation_challenges AS challenge
      ON challenge.challenge_id = alternative.challenge_id
    WHERE challenge.decision_id = NEW.decision_id
      AND challenge.selected_logical_action_id = alternative.logical_action_id
      AND alternative.exact_presented_action_json = NEW.exact_presented_action_json
      AND alternative.confirmed_logical_action_identity = NEW.confirmed_logical_action_identity
      AND alternative.manifest_digest = NEW.manifest_digest
      AND alternative.payload_digest = NEW.payload_digest
      AND alternative.payload_and_material_operands_json = NEW.payload_and_material_operands_json
      AND alternative.target_audience_path_json = NEW.target_audience_path_json
      AND alternative.connector_tool_identity IS NEW.connector_tool_identity
      AND alternative.connector_tool_version IS NEW.connector_tool_version
      AND alternative.canonical_action_envelope_or_selector_json = NEW.side_effect_envelope_json
      AND alternative.declared_consequence = NEW.consequence
  )
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss decision binding is immutable');
END;

CREATE TRIGGER execass_decisions_no_delete
BEFORE DELETE ON execass_decisions BEGIN
  SELECT RAISE(ABORT, 'ExecAss decisions cannot be deleted online');
END;

CREATE TRIGGER execass_decision_resolution_requires_pending_record
BEFORE INSERT ON execass_decisions
WHEN NEW.status = 'resolved' BEGIN
  SELECT RAISE(ABORT, 'ExecAss decisions must be resolved from one persisted pending record');
END;

CREATE TRIGGER execass_decision_resolution_requires_server_derived_owner
BEFORE UPDATE OF status, result, resolved_at, resolved_by_authority_provenance_id
ON execass_decisions
WHEN NEW.status = 'resolved' AND (
  OLD.status != 'pending' OR NOT EXISTS (
    SELECT 1 FROM execass_authority_provenance AS resolver
    WHERE resolver.authority_provenance_id = NEW.resolved_by_authority_provenance_id
      AND resolver.actor_type IN ('human_local', 'human_remote')
      AND resolver.authority_kind = 'decision_resolution'
      AND resolver.bound_decision_id = NEW.decision_id
      AND resolver.bound_decision_revision = NEW.decision_revision
      AND resolver.bound_manifest_digest = NEW.manifest_digest
      AND resolver.created_at >= NEW.requested_at
      AND (
        NEW.decision_kind != 'dangerous_action_confirmation' OR resolver.created_at <= (
          SELECT challenge.expires_at
          FROM execass_confirmation_challenges AS challenge
          WHERE challenge.decision_id = NEW.decision_id
        )
      )
  )
  OR (
    NEW.decision_kind = 'dangerous_action_confirmation'
    AND NEW.result = 'confirm_and_continue'
    AND NOT EXISTS (
      SELECT 1 FROM execass_confirmation_challenges AS challenge
      JOIN execass_authority_provenance AS resolver
        ON resolver.authority_provenance_id = NEW.resolved_by_authority_provenance_id
      JOIN execass_confirmation_attestations AS attestation
        ON attestation.decision_id = NEW.decision_id
       AND attestation.authority_provenance_id = resolver.authority_provenance_id
      WHERE challenge.decision_id = NEW.decision_id
        AND challenge.delegation_id = NEW.delegation_id
        AND challenge.decision_revision = NEW.decision_revision
        AND challenge.manifest_digest = NEW.manifest_digest
        AND challenge.status = 'resolved'
        AND challenge.resolved_at <= challenge.expires_at
        AND resolver.created_at <= challenge.expires_at
        AND resolver.created_at <= challenge.resolved_at
        AND resolver.bound_challenge_nonce_digest = challenge.nonce_digest
        AND attestation.selected_logical_action_id = challenge.selected_logical_action_id
        AND attestation.verified_at = challenge.resolved_at
    )
  )
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss decision resolution requires server-derived owner assurance');
END;

CREATE TRIGGER execass_confirmation_challenge_insert_binding
BEFORE INSERT ON execass_confirmation_challenges
WHEN NEW.status != 'pending' OR NEW.resolved_at IS NOT NULL OR NOT EXISTS (
  SELECT 1 FROM execass_decisions AS decision
  WHERE decision.decision_id = NEW.decision_id
    AND decision.delegation_id = NEW.delegation_id
    AND decision.decision_revision = NEW.decision_revision
    AND decision.decision_kind = 'dangerous_action_confirmation'
    AND decision.status = 'pending'
    AND decision.exact_presented_action_json = NEW.exact_presented_action_json
    AND decision.confirmed_logical_action_identity = NEW.confirmed_logical_action_identity
    AND decision.manifest_digest = NEW.manifest_digest
    AND decision.payload_digest = NEW.payload_digest
    AND decision.payload_and_material_operands_json = NEW.payload_and_material_operands_json
    AND decision.connector_tool_identity IS NEW.connector_tool_identity
    AND decision.connector_tool_version IS NEW.connector_tool_version
    AND decision.side_effect_envelope_json = NEW.canonical_action_envelope_or_selector_json
    AND decision.consequence = NEW.declared_consequence
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation challenge must be a pending exact binding of one dangerous decision');
END;

CREATE TRIGGER execass_confirmation_challenge_binding_immutable
BEFORE UPDATE OF challenge_id, decision_id, delegation_id, decision_revision,
  exact_presented_action_json, confirmed_logical_action_identity, manifest_digest, payload_digest,
  payload_and_material_operands_json, connector_tool_identity, connector_tool_version,
  canonical_action_envelope_or_selector_json, declared_consequence, nonce_digest,
  created_at, expires_at
ON execass_confirmation_challenges BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation challenge binding is immutable');
END;

CREATE TRIGGER execass_confirmation_challenge_selection_once
BEFORE UPDATE OF selected_logical_action_id ON execass_confirmation_challenges
WHEN OLD.selected_logical_action_id IS NOT NULL
  OR NEW.selected_logical_action_id IS NULL
  OR NEW.status != 'resolved'
  OR NOT EXISTS (
    SELECT 1 FROM execass_confirmation_challenge_alternatives
    WHERE challenge_id = NEW.challenge_id
      AND logical_action_id = NEW.selected_logical_action_id
  ) BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation challenge selection must be one disclosed alternative');
END;

CREATE TRIGGER execass_confirmation_challenge_alternative_insert_binding
BEFORE INSERT ON execass_confirmation_challenge_alternatives
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_confirmation_challenges AS challenge
  JOIN execass_decisions AS decision ON decision.decision_id = challenge.decision_id
  WHERE challenge.challenge_id = NEW.challenge_id
    AND challenge.status = 'pending'
    AND (
      (
        decision.alternatives_json = '["confirm_and_continue","revise","decline"]'
        AND NEW.exact_presented_action_json = decision.exact_presented_action_json
        AND NEW.confirmed_logical_action_identity = decision.confirmed_logical_action_identity
        AND NEW.manifest_digest = decision.manifest_digest
        AND NEW.payload_digest = decision.payload_digest
        AND NEW.payload_and_material_operands_json = decision.payload_and_material_operands_json
        AND NEW.target_audience_path_json = decision.target_audience_path_json
        AND NEW.connector_tool_identity IS decision.connector_tool_identity
        AND NEW.connector_tool_version IS decision.connector_tool_version
        AND NEW.canonical_action_envelope_or_selector_json = decision.side_effect_envelope_json
        AND NEW.declared_consequence = decision.consequence
      )
      OR EXISTS (
        SELECT 1 FROM json_each(decision.alternatives_json, '$.alternatives') AS disclosure
        WHERE json_extract(disclosure.value, '$.logical_action_id') = NEW.logical_action_id
          AND json_extract(disclosure.value, '$.exact_presented_action') = NEW.exact_presented_action_json
          AND json_extract(disclosure.value, '$.confirmed_logical_action_identity') = NEW.confirmed_logical_action_identity
          AND json_extract(disclosure.value, '$.manifest_digest') = NEW.manifest_digest
          AND json_extract(disclosure.value, '$.payload_digest') = NEW.payload_digest
          AND json_extract(disclosure.value, '$.payload_and_material_operands') = NEW.payload_and_material_operands_json
          AND json_extract(disclosure.value, '$.resolved_scope') = NEW.target_audience_path_json
          AND json_extract(disclosure.value, '$.connector_tool_identity') IS NEW.connector_tool_identity
          AND json_extract(disclosure.value, '$.connector_tool_version') IS NEW.connector_tool_version
          AND json_extract(disclosure.value, '$.canonical_action_envelope_or_selector') = NEW.canonical_action_envelope_or_selector_json
          AND json_extract(disclosure.value, '$.declared_consequence') = NEW.declared_consequence
      )
    )
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation alternative must bind one pending challenge');
END;

CREATE TRIGGER execass_confirmation_challenge_alternatives_immutable
BEFORE UPDATE ON execass_confirmation_challenge_alternatives BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation alternative binding is immutable');
END;

CREATE TRIGGER execass_confirmation_challenge_alternatives_no_delete
BEFORE DELETE ON execass_confirmation_challenge_alternatives BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation alternatives cannot be deleted online');
END;

CREATE TRIGGER execass_confirmation_challenge_resolution_once
BEFORE UPDATE OF status, resolved_at ON execass_confirmation_challenges
WHEN OLD.status != 'pending' OR (
  NEW.status = 'resolved' AND (NEW.resolved_at IS NULL OR NEW.resolved_at >= NEW.expires_at)
) OR (
  NEW.status = 'expired' AND NEW.resolved_at IS NOT NULL
) OR NEW.status NOT IN ('resolved', 'expired') BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation challenge resolves only once');
END;

CREATE TRIGGER execass_confirmation_challenges_no_delete
BEFORE DELETE ON execass_confirmation_challenges BEGIN
  SELECT RAISE(ABORT, 'ExecAss confirmation challenges cannot be deleted online');
END;

CREATE TRIGGER execass_confirmation_grant_identity_immutable
BEFORE UPDATE OF grant_id, delegation_id, decision_id, confirmed_logical_action_identity,
  canonical_action_envelope_or_selector_json, payload_and_material_operands_json,
  payload_and_material_operands_digest,
  connector_tool_identity, connector_tool_version, declared_consequence,
  accepted_by_authority_provenance_id, confirmation_attestation_digest, accepted_at
ON execass_accepted_confirmation_grants BEGIN
  SELECT RAISE(ABORT, 'ExecAss accepted confirmation grant identity is immutable');
END;

CREATE TRIGGER execass_confirmation_grant_requires_resolved_challenge
BEFORE INSERT ON execass_accepted_confirmation_grants
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_decisions AS decision
  JOIN execass_confirmation_challenges AS challenge
    ON challenge.decision_id = decision.decision_id
  JOIN execass_confirmation_challenge_alternatives AS alternative
    ON alternative.challenge_id = challenge.challenge_id
   AND alternative.logical_action_id = challenge.selected_logical_action_id
  JOIN execass_confirmation_attestations AS attestation
    ON attestation.decision_id = decision.decision_id
   AND attestation.authority_provenance_id = decision.resolved_by_authority_provenance_id
  WHERE decision.decision_id = NEW.decision_id
    AND decision.delegation_id = NEW.delegation_id
    AND decision.decision_kind = 'dangerous_action_confirmation'
    AND decision.status = 'resolved'
    AND decision.result = 'confirm_and_continue'
    AND decision.resolved_by_authority_provenance_id = NEW.accepted_by_authority_provenance_id
    AND attestation.attestation_digest = NEW.confirmation_attestation_digest
    AND attestation.selected_logical_action_id = challenge.selected_logical_action_id
    AND decision.confirmed_logical_action_identity = NEW.confirmed_logical_action_identity
    AND decision.side_effect_envelope_json = NEW.canonical_action_envelope_or_selector_json
    AND decision.payload_digest = NEW.payload_and_material_operands_digest
    AND decision.payload_and_material_operands_json = NEW.payload_and_material_operands_json
    AND decision.connector_tool_identity IS NEW.connector_tool_identity
    AND decision.connector_tool_version IS NEW.connector_tool_version
    AND decision.consequence = NEW.declared_consequence
    AND challenge.status = 'resolved'
    AND challenge.resolved_at <= challenge.expires_at
    AND NEW.accepted_at >= challenge.resolved_at
    AND alternative.exact_presented_action_json = decision.exact_presented_action_json
    AND alternative.confirmed_logical_action_identity = NEW.confirmed_logical_action_identity
    AND alternative.manifest_digest = decision.manifest_digest
    AND alternative.payload_digest = NEW.payload_and_material_operands_digest
    AND alternative.payload_and_material_operands_json = NEW.payload_and_material_operands_json
    AND alternative.connector_tool_identity IS NEW.connector_tool_identity
    AND alternative.connector_tool_version IS NEW.connector_tool_version
    AND alternative.canonical_action_envelope_or_selector_json = NEW.canonical_action_envelope_or_selector_json
    AND alternative.declared_consequence = NEW.declared_consequence
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss accepted confirmation grant requires one resolved dangerous-action challenge');
END;

CREATE TRIGGER execass_accepted_confirmation_grants_no_delete
BEFORE DELETE ON execass_accepted_confirmation_grants BEGIN
  SELECT RAISE(ABORT, 'ExecAss accepted confirmation grants cannot be deleted online');
END;

CREATE TRIGGER execass_confirmation_grant_invalidation_is_action_specific
BEFORE UPDATE OF invalidated_at, invalidation_reason
ON execass_accepted_confirmation_grants
WHEN OLD.invalidated_at IS NOT NULL OR NEW.invalidated_at IS NULL OR NEW.invalidation_reason IS NULL BEGIN
  SELECT RAISE(ABORT, 'ExecAss accepted confirmation grant invalidation must be one explicit material action change');
END;

CREATE TRIGGER execass_confirmation_grant_owner_invalidation_provenance
BEFORE UPDATE OF invalidated_at, invalidation_reason, invalidated_by_authority_provenance_id
ON execass_accepted_confirmation_grants
WHEN NEW.invalidation_reason IN (
  'explicit_action_specific_owner_amendment',
  'explicit_action_specific_owner_revocation',
  'explicit_action_specific_owner_cancellation'
) AND NOT EXISTS (
  SELECT 1 FROM execass_authority_provenance AS authority
  WHERE authority.authority_provenance_id = NEW.invalidated_by_authority_provenance_id
    AND authority.actor_type IN ('human_local', 'human_remote')
    AND authority.authority_kind = 'action_specific_owner_amendment'
    AND authority.bound_decision_id = NEW.decision_id
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss explicit confirmation-grant invalidation requires a bound authenticated owner amendment');
END;

CREATE TRIGGER execass_receipts_immutable
BEFORE UPDATE ON execass_receipts BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipts are immutable');
END;

CREATE TRIGGER execass_receipts_no_delete
BEFORE DELETE ON execass_receipts BEGIN
  SELECT RAISE(ABORT, 'ExecAss receipts cannot be deleted online');
END;

CREATE TRIGGER execass_effect_tombstones_immutable
BEFORE UPDATE ON execass_effect_tombstones BEGIN
  SELECT RAISE(ABORT, 'ExecAss effect tombstones are immutable');
END;

CREATE TRIGGER execass_effect_tombstones_no_delete
BEFORE DELETE ON execass_effect_tombstones BEGIN
  SELECT RAISE(ABORT, 'ExecAss effect tombstones cannot be deleted online');
END;

CREATE TRIGGER execass_continuation_fence_monotonic
BEFORE UPDATE ON execass_continuations
WHEN NEW.fencing_token < OLD.fencing_token
  OR NEW.host_generation < OLD.host_generation
  OR NEW.continuation_id != OLD.continuation_id
  OR NEW.delegation_id != OLD.delegation_id
  OR NEW.target_delegation_revision != OLD.target_delegation_revision
  OR NEW.target_plan_revision != OLD.target_plan_revision
  OR NEW.action_id != OLD.action_id
  OR NEW.branch_kind != OLD.branch_kind
  OR NEW.causation_kind != OLD.causation_kind
  OR NEW.causation_id != OLD.causation_id
  OR NEW.stop_epoch != OLD.stop_epoch
  OR NEW.global_stop_epoch != OLD.global_stop_epoch
  OR NEW.created_at != OLD.created_at BEGIN
  SELECT RAISE(ABORT, 'ExecAss continuation fencing cannot decrease');
END;

CREATE TRIGGER execass_criteria_sets_lineage_immutable
BEFORE UPDATE ON execass_criteria_sets
WHEN NEW.criteria_set_id != OLD.criteria_set_id
  OR NEW.delegation_id != OLD.delegation_id
  OR NEW.criteria_revision != OLD.criteria_revision
  OR NEW.parent_criteria_revision IS NOT OLD.parent_criteria_revision
  OR NEW.created_at != OLD.created_at
  OR OLD.disposition = 'superseded'
  OR NEW.disposition NOT IN ('superseded', OLD.disposition)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss criteria-set lineage is immutable');
END;

CREATE TRIGGER execass_criteria_sets_no_delete
BEFORE DELETE ON execass_criteria_sets BEGIN
  SELECT RAISE(ABORT, 'ExecAss criteria sets cannot be deleted');
END;

CREATE TRIGGER execass_action_branch_identity_immutable
BEFORE UPDATE ON execass_action_branches
WHEN NEW.action_id != OLD.action_id
  OR NEW.delegation_id != OLD.delegation_id
  OR NEW.action_revision != OLD.action_revision
  OR NEW.target_delegation_revision != OLD.target_delegation_revision
  OR NEW.target_plan_revision != OLD.target_plan_revision
  OR NEW.stop_epoch != OLD.stop_epoch
  OR NEW.branch_kind != OLD.branch_kind
  OR NEW.action_summary != OLD.action_summary
  OR NEW.created_at != OLD.created_at
  OR OLD.status IN ('terminal', 'superseded') AND NEW.status != OLD.status
BEGIN
  SELECT RAISE(ABORT, 'ExecAss action branch identity is immutable');
END;

CREATE TRIGGER execass_attention_identity_immutable
BEFORE UPDATE ON execass_attention_items
WHEN NEW.attention_id != OLD.attention_id
  OR NEW.scope_kind != OLD.scope_kind
  OR NEW.delegation_id IS NOT OLD.delegation_id
  OR NEW.action_id IS NOT OLD.action_id
  OR NEW.kind != OLD.kind
  OR NEW.reason != OLD.reason
  OR NEW.recommendation != OLD.recommendation
  OR NEW.alternatives_json != OLD.alternatives_json
  OR NEW.required_assurance != OLD.required_assurance
  OR NEW.decision_id IS NOT OLD.decision_id
  OR NEW.delegation_revision IS NOT OLD.delegation_revision
  OR NEW.runtime_host_generation IS NOT OLD.runtime_host_generation
  OR NEW.runtime_host_instance_id IS NOT OLD.runtime_host_instance_id
  OR NEW.runtime_fencing_token IS NOT OLD.runtime_fencing_token
  OR NEW.runtime_actual_state IS NOT OLD.runtime_actual_state
  OR NEW.runtime_end_reason IS NOT OLD.runtime_end_reason
  OR NEW.active_work_binding_digest IS NOT OLD.active_work_binding_digest
  OR NEW.outbox_event_id IS NOT OLD.outbox_event_id
  OR NEW.receipt_id IS NOT OLD.receipt_id
  OR NEW.created_at != OLD.created_at
BEGIN
  SELECT RAISE(ABORT, 'ExecAss attention identity is immutable');
END;

CREATE TRIGGER execass_external_wait_identity_immutable
BEFORE UPDATE ON execass_external_waits
WHEN NEW.external_wait_id != OLD.external_wait_id
  OR NEW.delegation_id != OLD.delegation_id
  OR NEW.action_id IS NOT OLD.action_id
  OR NEW.kind != OLD.kind
  OR NEW.reason != OLD.reason
  OR NEW.details_json != OLD.details_json
  OR NEW.delegation_revision != OLD.delegation_revision
  OR NEW.created_at != OLD.created_at
BEGIN
  SELECT RAISE(ABORT, 'ExecAss external wait identity is immutable');
END;

CREATE TRIGGER execass_logical_effect_identity_immutable
BEFORE UPDATE OF logical_effect_id, delegation_id, continuation_id, action_kind,
  operation_reversible, declared_recovery_safe_boundary,
  internal_idempotency_key, provider_identity, provider_idempotency_key,
  reconciliation_key, manifest_digest, payload_digest, created_at
ON execass_logical_effects BEGIN
  SELECT RAISE(ABORT, 'ExecAss logical effect identity is immutable');
END;

CREATE TRIGGER execass_logical_effect_no_delete
BEFORE DELETE ON execass_logical_effects BEGIN
  SELECT RAISE(ABORT, 'ExecAss logical effects cannot be deleted');
END;

-- Storage imports only recorder-signed terminal observations. It does not
-- accept a runtime/gateway-selected execution result. A signed execution
-- `Absent` is the only authority for the definite `failed` state.
CREATE TRIGGER execass_logical_effect_recorder_execution_guard
BEFORE UPDATE OF state,outcome_json,updated_at ON execass_logical_effects
WHEN OLD.state='invoking'
  AND NEW.state IN ('succeeded','failed','outcome_unknown','reconciled_absent')
  AND (
    NEW.state='reconciled_absent'
    OR NOT EXISTS (
      SELECT 1 FROM execass_effect_recorder_evidence evidence
      WHERE evidence.logical_effect_id=OLD.logical_effect_id
        AND evidence.journal_source='execution'
        AND evidence.journal_kind=CASE NEW.state
          WHEN 'succeeded' THEN 'present'
          WHEN 'outcome_unknown' THEN 'unknown'
          WHEN 'failed' THEN 'absent' END
        AND evidence.response_digest IS json_extract(NEW.outcome_json,'$.response_digest')
        AND evidence.provider_error_class IS json_extract(NEW.outcome_json,'$.provider_error_class')
        AND evidence.remote_effect_id IS json_extract(NEW.outcome_json,'$.remote_effect_id')
        AND evidence.recorder_record_digest=json_extract(NEW.outcome_json,'$.recorder_record_digest')
        AND NEW.state=json_extract(NEW.outcome_json,'$.state')
        AND json_type(NEW.outcome_json)='object'
        AND (SELECT COUNT(*) FROM json_each(NEW.outcome_json))=5
        AND evidence.observed_at=NEW.updated_at
    )
  )
BEGIN
  SELECT RAISE(ABORT, 'ExecAss execution effect result requires exact imported signed recorder evidence');
END;

CREATE TRIGGER execass_logical_effect_recorder_reconcile_guard
BEFORE UPDATE OF state,outcome_json,updated_at ON execass_logical_effects
WHEN OLD.state='outcome_unknown' AND NEW.state IN ('reconciled_absent','reconciled_present')
AND NOT EXISTS (
  SELECT 1 FROM execass_effect_recorder_evidence evidence
  WHERE evidence.logical_effect_id=OLD.logical_effect_id
    AND evidence.journal_source='reconciliation'
    AND evidence.journal_kind=CASE NEW.state
      WHEN 'reconciled_absent' THEN 'absent' ELSE 'present' END
    AND evidence.response_digest IS json_extract(NEW.outcome_json,'$.response_digest')
    AND evidence.provider_error_class IS json_extract(NEW.outcome_json,'$.provider_error_class')
    AND evidence.remote_effect_id IS json_extract(NEW.outcome_json,'$.remote_effect_id')
    AND evidence.recorder_record_digest=json_extract(NEW.outcome_json,'$.recorder_record_digest')
    AND NEW.state=json_extract(NEW.outcome_json,'$.state')
    AND json_type(NEW.outcome_json)='object'
    AND (SELECT COUNT(*) FROM json_each(NEW.outcome_json))=5
    AND evidence.observed_at=NEW.updated_at
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss reconciled effect requires exact imported signed reconciliation evidence');
END;

CREATE TRIGGER execass_effect_recorder_keys_immutable
BEFORE UPDATE ON execass_effect_recorder_keys BEGIN
  SELECT RAISE(ABORT, 'ExecAss effect-recorder keys are immutable');
END;

CREATE TRIGGER execass_effect_recorder_keys_no_delete
BEFORE DELETE ON execass_effect_recorder_keys BEGIN
  SELECT RAISE(ABORT, 'ExecAss effect-recorder keys cannot be deleted');
END;

CREATE TRIGGER execass_effect_recorder_evidence_insert_guard
BEFORE INSERT ON execass_effect_recorder_evidence
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_effect_recorder_keys recorder_key
  JOIN execass_provider_attempts attempt
    ON attempt.attempt_id=NEW.attempt_id
  JOIN execass_logical_effects effect
    ON effect.logical_effect_id=NEW.logical_effect_id
   AND effect.logical_effect_id=attempt.logical_effect_id
   AND effect.delegation_id=attempt.delegation_id
  WHERE recorder_key.recorder_key_id=NEW.recorder_key_id
    AND recorder_key.key_generation=NEW.key_generation
    AND recorder_key.status='active'
    AND recorder_key.canonical_root_identity=NEW.canonical_root_identity
    AND recorder_key.installation_identity=NEW.installation_identity
    AND recorder_key.state_root_generation=NEW.state_root_generation
    AND recorder_key.os_user_identity_digest=NEW.os_user_identity_digest
    AND execass_verify_recorder_evidence_v1(
      recorder_key.verifying_key_hex,
      NEW.signed_payload_json,
      NEW.signature
    )=1
    AND effect.provider_identity=NEW.provider_identity
    AND attempt.provider_request_digest=NEW.provider_request_digest
    AND attempt.attempt_number=(
      SELECT MAX(latest.attempt_number)
      FROM execass_provider_attempts latest
      WHERE latest.logical_effect_id=effect.logical_effect_id
    )
    AND (
      (NEW.journal_source='execution' AND NEW.journal_kind IN ('present','absent','unknown')
        AND effect.state IN ('invoking','outcome_unknown')
        AND attempt.status IN ('invoking','outcome_unknown'))
      OR
      (NEW.journal_source='reconciliation' AND NEW.journal_kind IN ('present','absent','unknown')
        AND effect.state='outcome_unknown'
        AND attempt.status='outcome_unknown')
    )
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss recorder evidence lacks exact active key/attempt/effect provenance');
END;

CREATE TRIGGER execass_effect_recorder_evidence_payload_guard
BEFORE INSERT ON execass_effect_recorder_evidence
WHEN NOT (
  json_extract(NEW.signed_payload_json,'$.record_id')=NEW.record_id
  AND json_extract(NEW.signed_payload_json,'$.record_digest')=NEW.recorder_record_digest
  AND json_extract(NEW.signed_payload_json,'$.recorder_key_id')=NEW.recorder_key_id
  AND json_extract(NEW.signed_payload_json,'$.recorder_key_generation')=NEW.key_generation
  AND json_extract(NEW.signed_payload_json,'$.canonical_root_identity')=NEW.canonical_root_identity
  AND json_extract(NEW.signed_payload_json,'$.installation_id')=NEW.installation_identity
  AND json_extract(NEW.signed_payload_json,'$.state_root_generation')=NEW.state_root_generation
  AND json_extract(NEW.signed_payload_json,'$.os_user_identity_digest')=NEW.os_user_identity_digest
  AND json_extract(NEW.signed_payload_json,'$.sequence')=NEW.journal_sequence
  AND json_extract(NEW.signed_payload_json,'$.kind')=NEW.journal_kind
  AND json_extract(NEW.signed_payload_json,'$.source')=NEW.journal_source
  AND json_extract(NEW.signed_payload_json,'$.attempt_id')=NEW.attempt_id
  AND json_extract(NEW.signed_payload_json,'$.logical_effect_id')=NEW.logical_effect_id
  AND json_extract(NEW.signed_payload_json,'$.command_digest')=NEW.command_digest
  AND json_extract(NEW.signed_payload_json,'$.provider_identity')=NEW.provider_identity
  AND json_extract(NEW.signed_payload_json,'$.provider_version')=NEW.provider_version
  AND json_extract(NEW.signed_payload_json,'$.provider_request_digest')=NEW.provider_request_digest
  AND json_extract(NEW.signed_payload_json,'$.provider_idempotency_key_digest') IS NEW.provider_idempotency_key_digest
  AND json_extract(NEW.signed_payload_json,'$.reconciliation_key_digest') IS NEW.reconciliation_key_digest
  AND json_extract(NEW.signed_payload_json,'$.remote_effect_id') IS NEW.remote_effect_id
  AND json_extract(NEW.signed_payload_json,'$.response_digest') IS NEW.response_digest
  AND json_extract(NEW.signed_payload_json,'$.provider_error_class') IS NEW.provider_error_class
  AND json_extract(NEW.signed_payload_json,'$.evidence_payload_digest') IS NEW.evidence_payload_digest
  AND json(json_extract(NEW.signed_payload_json,'$.technical_resource_actuals'))=json(NEW.technical_resource_actuals_json)
  AND json_extract(NEW.signed_payload_json,'$.reconciliation_window_start_ms') IS NEW.reconciliation_window_start
  AND json_extract(NEW.signed_payload_json,'$.reconciliation_window_end_ms') IS NEW.reconciliation_window_end
  AND json_extract(NEW.signed_payload_json,'$.observed_at_ms')=NEW.observed_at
  AND json_extract(NEW.signed_payload_json,'$.previous_record_digest')=NEW.previous_record_digest
  AND json_extract(NEW.signed_payload_json,'$.signature_hex')=''
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss recorder evidence projection does not match its signed payload');
END;

CREATE TRIGGER execass_effect_recorder_evidence_immutable
BEFORE UPDATE ON execass_effect_recorder_evidence BEGIN
  SELECT RAISE(ABORT, 'ExecAss effect-recorder evidence is immutable');
END;

CREATE TRIGGER execass_effect_recorder_evidence_no_delete
BEFORE DELETE ON execass_effect_recorder_evidence BEGIN
  SELECT RAISE(ABORT, 'ExecAss effect-recorder evidence cannot be deleted');
END;

CREATE TRIGGER execass_provider_attempt_identity_immutable
BEFORE UPDATE OF attempt_id, delegation_id, logical_effect_id, continuation_id,
  action_id, claim_event_id, claim_receipt_id, attempt_number, fencing_token,
  host_generation, host_instance_id, runtime_fencing_token, provider_request_digest, started_at
ON execass_provider_attempts BEGIN
  SELECT RAISE(ABORT, 'ExecAss provider attempt identity is immutable');
END;

CREATE TRIGGER execass_provider_attempt_no_delete
BEFORE DELETE ON execass_provider_attempts BEGIN
  SELECT RAISE(ABORT, 'ExecAss provider attempts cannot be deleted');
END;

CREATE TRIGGER execass_provider_attempt_transition_guard
BEFORE UPDATE OF status, provider_response_digest, provider_error_class, remote_effect_id, finished_at
ON execass_provider_attempts
WHEN NOT (
  (OLD.status='prepared' AND NEW.status IN ('prepared','invoking'))
  OR (OLD.status='invoking' AND NEW.status IN ('invoking','succeeded','failed','outcome_unknown'))
  OR (OLD.status='outcome_unknown' AND NEW.status IN ('outcome_unknown','reconciled_absent','reconciled_present'))
  OR (OLD.status IN ('succeeded','failed','reconciled_absent','reconciled_present') AND NEW.status=OLD.status)
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss provider attempt transition is invalid');
END;

CREATE TRIGGER execass_provider_attempt_recorder_execution_guard
BEFORE UPDATE OF status,provider_response_digest,provider_error_class,remote_effect_id,finished_at
ON execass_provider_attempts
WHEN OLD.status='invoking'
  AND NEW.status IN ('succeeded','failed','outcome_unknown')
  AND NOT EXISTS (
    SELECT 1 FROM execass_effect_recorder_evidence evidence
    WHERE evidence.attempt_id=OLD.attempt_id
      AND evidence.logical_effect_id=OLD.logical_effect_id
      AND evidence.journal_source='execution'
      AND evidence.journal_kind=CASE NEW.status
        WHEN 'succeeded' THEN 'present'
        WHEN 'outcome_unknown' THEN 'unknown'
        WHEN 'failed' THEN 'absent' END
      AND evidence.response_digest IS NEW.provider_response_digest
      AND evidence.provider_error_class IS NEW.provider_error_class
      AND evidence.remote_effect_id IS NEW.remote_effect_id
      AND evidence.observed_at=NEW.finished_at
  )
BEGIN
  SELECT RAISE(ABORT, 'ExecAss execution attempt result requires exact imported signed recorder evidence');
END;

CREATE TRIGGER execass_provider_attempt_terminal_fields_immutable
BEFORE UPDATE ON execass_provider_attempts
WHEN OLD.finished_at IS NOT NULL AND (
  NEW.provider_response_digest IS NOT OLD.provider_response_digest
  OR NEW.provider_error_class IS NOT OLD.provider_error_class
  OR NEW.remote_effect_id IS NOT OLD.remote_effect_id
  OR NEW.finished_at IS NOT OLD.finished_at
)
AND NOT (
  OLD.status='outcome_unknown'
  AND NEW.status IN ('reconciled_absent','reconciled_present')
  AND EXISTS (
    SELECT 1 FROM execass_effect_recorder_evidence evidence
    WHERE evidence.attempt_id=OLD.attempt_id
      AND evidence.logical_effect_id=OLD.logical_effect_id
      AND evidence.journal_source='reconciliation'
      AND evidence.journal_kind=CASE NEW.status
        WHEN 'reconciled_absent' THEN 'absent' ELSE 'present' END
      AND evidence.response_digest IS NEW.provider_response_digest
      AND evidence.provider_error_class IS NEW.provider_error_class
      AND evidence.remote_effect_id IS NEW.remote_effect_id
      AND evidence.observed_at=NEW.finished_at
  )
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss provider attempt terminal result is immutable');
END;

CREATE TRIGGER execass_provider_attempt_recorder_reconcile_guard
BEFORE UPDATE OF status,provider_response_digest,provider_error_class,remote_effect_id,finished_at
ON execass_provider_attempts
WHEN OLD.status='outcome_unknown' AND NEW.status IN ('reconciled_absent','reconciled_present')
AND NOT EXISTS (
  SELECT 1 FROM execass_effect_recorder_evidence evidence
  WHERE evidence.attempt_id=OLD.attempt_id
    AND evidence.logical_effect_id=OLD.logical_effect_id
    AND evidence.journal_source='reconciliation'
    AND evidence.journal_kind=CASE NEW.status
      WHEN 'reconciled_absent' THEN 'absent' ELSE 'present' END
    AND evidence.response_digest IS NEW.provider_response_digest
    AND evidence.provider_error_class IS NEW.provider_error_class
    AND evidence.remote_effect_id IS NEW.remote_effect_id
    AND evidence.observed_at=NEW.finished_at
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss reconciled attempt requires exact imported signed recorder evidence');
END;

CREATE TRIGGER execass_provider_attempt_claim_binding_guard
BEFORE INSERT ON execass_provider_attempts
WHEN NOT EXISTS (
  SELECT 1 FROM execass_continuation_operation_history h
  JOIN execass_logical_effects e
    ON e.delegation_id=h.delegation_id
   AND e.continuation_id=h.continuation_id
   AND e.logical_effect_id=NEW.logical_effect_id
  WHERE h.operation='claim'
    AND h.event_id=NEW.claim_event_id
    AND h.claim_receipt_id=NEW.claim_receipt_id
    AND h.delegation_id=NEW.delegation_id
    AND h.continuation_id=NEW.continuation_id
    AND h.action_id=NEW.action_id
    AND h.continuation_fencing_token=NEW.fencing_token
    AND h.runtime_host_generation=NEW.host_generation
    AND h.runtime_host_instance_id=NEW.host_instance_id
    AND h.runtime_fencing_token=NEW.runtime_fencing_token
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss provider attempt lacks exact immutable claim binding');
END;

CREATE TRIGGER execass_outcome_unknown_attempt_prohibition
BEFORE INSERT ON execass_provider_attempts
WHEN EXISTS (
  SELECT 1 FROM execass_logical_effects AS effect
  WHERE effect.logical_effect_id = NEW.logical_effect_id
    AND effect.state = 'outcome_unknown'
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss outcome_unknown effect cannot be retried before reconciliation');
END;

CREATE TRIGGER execass_duplicate_risk_binding_insert_guard
BEFORE INSERT ON execass_duplicate_risk_bindings
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_decisions d
  JOIN execass_logical_effects e
    ON e.delegation_id=d.delegation_id
   AND e.logical_effect_id=NEW.predecessor_logical_effect_id
  JOIN execass_continuations c
    ON c.delegation_id=e.delegation_id
   AND c.continuation_id=e.continuation_id
  JOIN execass_provider_attempts a
    ON a.delegation_id=e.delegation_id
   AND a.logical_effect_id=e.logical_effect_id
   AND a.attempt_id=NEW.predecessor_attempt_id
  WHERE d.decision_id=NEW.decision_id
    AND d.delegation_id=NEW.delegation_id
    AND d.decision_kind='duplicate_risk_retry'
    AND d.status='pending'
    AND d.confirmed_logical_action_identity=NEW.confirmed_logical_action_identity
    AND c.action_id=d.confirmed_logical_action_identity
    AND e.manifest_digest=d.manifest_digest
    AND e.payload_digest=d.payload_digest
    AND e.state='outcome_unknown'
    AND a.status='outcome_unknown'
    AND a.provider_response_digest=NEW.predecessor_uncertainty_evidence_digest
    AND a.attempt_number=(
      SELECT MAX(latest.attempt_number)
      FROM execass_provider_attempts latest
      WHERE latest.logical_effect_id=e.logical_effect_id
    )
    AND (
      NEW.accepted_confirmation_grant_id IS NULL
      OR EXISTS (
        SELECT 1 FROM execass_accepted_confirmation_grants g
        WHERE g.grant_id=NEW.accepted_confirmation_grant_id
          AND g.delegation_id=d.delegation_id
          AND g.confirmed_logical_action_identity=d.confirmed_logical_action_identity
          AND g.invalidated_at IS NULL
      )
    )
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss duplicate-risk binding lacks one exact unresolved predecessor');
END;

CREATE TRIGGER execass_duplicate_risk_bindings_immutable
BEFORE UPDATE ON execass_duplicate_risk_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss duplicate-risk bindings are immutable');
END;

CREATE TRIGGER execass_duplicate_risk_bindings_no_delete
BEFORE DELETE ON execass_duplicate_risk_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss duplicate-risk bindings cannot be deleted');
END;

CREATE TRIGGER execass_duplicate_risk_successor_insert_guard
BEFORE INSERT ON execass_duplicate_risk_successors
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_duplicate_risk_bindings b
  JOIN execass_decisions d ON d.decision_id=b.decision_id
  JOIN execass_logical_effects predecessor
    ON predecessor.logical_effect_id=b.predecessor_logical_effect_id
  JOIN execass_logical_effects successor
    ON successor.logical_effect_id=NEW.successor_logical_effect_id
  JOIN execass_continuations continuation
    ON continuation.continuation_id=successor.continuation_id
   AND continuation.delegation_id=successor.delegation_id
  WHERE b.decision_id=NEW.decision_id
    AND b.predecessor_logical_effect_id=NEW.predecessor_logical_effect_id
    AND b.predecessor_attempt_id=NEW.predecessor_attempt_id
    AND b.predecessor_uncertainty_evidence_digest=NEW.predecessor_uncertainty_evidence_digest
    AND d.status='resolved'
    AND d.result='confirm_and_continue'
    AND predecessor.state='outcome_unknown'
    AND successor.state='planned'
    AND successor.delegation_id=predecessor.delegation_id
    AND successor.manifest_digest=predecessor.manifest_digest
    AND successor.payload_digest=predecessor.payload_digest
    AND successor.provider_identity IS predecessor.provider_identity
    AND successor.internal_idempotency_key<>predecessor.internal_idempotency_key
    AND continuation.causation_kind='decision'
    AND continuation.causation_id=d.decision_id
    AND (
      (predecessor.provider_identity IS NULL
        AND predecessor.provider_idempotency_key IS NULL
        AND predecessor.reconciliation_key IS NULL
        AND successor.provider_idempotency_key IS NULL
        AND successor.reconciliation_key IS NULL)
      OR
      (predecessor.provider_identity IS NOT NULL
        AND predecessor.provider_idempotency_key IS NOT NULL
        AND predecessor.reconciliation_key IS NOT NULL
        AND successor.provider_idempotency_key IS NOT NULL
        AND successor.reconciliation_key IS NOT NULL
        AND successor.provider_idempotency_key<>predecessor.provider_idempotency_key
        AND successor.reconciliation_key<>predecessor.reconciliation_key)
    )
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss duplicate-risk successor is not one distinct linked effect');
END;

CREATE TRIGGER execass_duplicate_risk_successors_immutable
BEFORE UPDATE ON execass_duplicate_risk_successors BEGIN
  SELECT RAISE(ABORT, 'ExecAss duplicate-risk successors are immutable');
END;

CREATE TRIGGER execass_duplicate_risk_successors_no_delete
BEFORE DELETE ON execass_duplicate_risk_successors BEGIN
  SELECT RAISE(ABORT, 'ExecAss duplicate-risk successors cannot be deleted');
END;

CREATE TRIGGER execass_technical_resource_quota_snapshots_immutable
BEFORE UPDATE ON execass_technical_resource_quota_snapshots BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource quota snapshots are immutable');
END;

CREATE TRIGGER execass_technical_resource_quota_snapshots_no_delete
BEFORE DELETE ON execass_technical_resource_quota_snapshots BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource quota snapshots cannot be deleted');
END;

CREATE TRIGGER execass_technical_resource_quota_entries_immutable
BEFORE UPDATE ON execass_technical_resource_quota_entries BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource quota entries are immutable');
END;

CREATE TRIGGER execass_technical_resource_quota_entries_no_delete
BEFORE DELETE ON execass_technical_resource_quota_entries BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource quota entries cannot be deleted');
END;

CREATE TRIGGER execass_technical_resource_requirement_set_insert_guard
BEFORE INSERT ON execass_technical_resource_requirement_sets
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_logical_effects e
  JOIN execass_continuations c
    ON c.continuation_id=e.continuation_id
   AND c.delegation_id=e.delegation_id
  JOIN execass_technical_resource_quota_snapshots q
    ON q.quota_snapshot_id=NEW.quota_snapshot_id
   AND q.delegation_id=NEW.delegation_id
  WHERE e.logical_effect_id=NEW.logical_effect_id
    AND e.delegation_id=NEW.delegation_id
    AND e.manifest_digest=NEW.manifest_digest
    AND c.action_id=NEW.action_id
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource requirement set lacks exact snapshot/effect/action/manifest binding');
END;

CREATE TRIGGER execass_technical_resource_requirement_sets_immutable
BEFORE UPDATE ON execass_technical_resource_requirement_sets BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource requirement sets are immutable');
END;

CREATE TRIGGER execass_technical_resource_requirement_sets_no_delete
BEFORE DELETE ON execass_technical_resource_requirement_sets BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource requirement sets cannot be deleted');
END;

CREATE TRIGGER execass_technical_resource_requirements_immutable
BEFORE UPDATE ON execass_technical_resource_requirements BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource requirements are immutable');
END;

CREATE TRIGGER execass_technical_resource_requirements_no_delete
BEFORE DELETE ON execass_technical_resource_requirements BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource requirements cannot be deleted');
END;

CREATE TRIGGER execass_technical_resource_reservation_insert_guard
BEFORE INSERT ON execass_technical_resource_reservations
WHEN NOT EXISTS (
  SELECT 1
  FROM execass_continuation_operation_history h
  JOIN execass_logical_effects e
    ON e.logical_effect_id=NEW.logical_effect_id
   AND e.delegation_id=NEW.delegation_id
   AND e.continuation_id=NEW.continuation_id
  JOIN execass_technical_resource_requirement_sets s
    ON s.quota_snapshot_id=NEW.quota_snapshot_id
   AND s.delegation_id=NEW.delegation_id
   AND s.logical_effect_id=NEW.logical_effect_id
   AND s.action_id=h.action_id
   AND s.manifest_digest=e.manifest_digest
  JOIN execass_technical_resource_requirements requirement
    ON requirement.requirement_set_id=s.requirement_set_id
   AND requirement.quota_snapshot_id=NEW.quota_snapshot_id
   AND requirement.technical_resource_kind=NEW.technical_resource_kind
   AND requirement.unit=NEW.unit
   AND requirement.amount_required=NEW.amount_reserved
  WHERE h.operation='claim'
    AND h.event_id=NEW.claim_event_id
    AND h.claim_receipt_id=NEW.claim_receipt_id
    AND h.continuation_id=NEW.continuation_id
    AND h.delegation_id=NEW.delegation_id
    AND h.continuation_fencing_token=NEW.continuation_fencing_token
    AND h.runtime_host_generation=NEW.runtime_host_generation
    AND h.runtime_fencing_token=NEW.runtime_fencing_token
    AND h.technical_quota_snapshot_id=NEW.quota_snapshot_id
) BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource reservation requires exact immutable claim provenance');
END;

CREATE TRIGGER execass_technical_resource_reservation_capacity_guard
BEFORE INSERT ON execass_technical_resource_reservations
WHEN NEW.amount_reserved + COALESCE((
  SELECT SUM(CASE
    WHEN r.status IN ('reserved','reconciliation_required') THEN r.amount_reserved
    WHEN r.status='settled' THEN a.amount_actual
    ELSE 0 END)
  FROM execass_technical_resource_reservations r
  LEFT JOIN execass_technical_resource_actuals a ON a.reservation_id=r.reservation_id
  WHERE r.quota_snapshot_id=NEW.quota_snapshot_id
    AND r.technical_resource_kind=NEW.technical_resource_kind
    AND r.unit=NEW.unit
),0) > (
  SELECT amount_limit FROM execass_technical_resource_quota_entries q
  WHERE q.quota_snapshot_id=NEW.quota_snapshot_id
    AND q.technical_resource_kind=NEW.technical_resource_kind
    AND q.unit=NEW.unit
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource quota is exhausted');
END;

CREATE TRIGGER execass_technical_resource_reservation_identity_immutable
BEFORE UPDATE OF reservation_id, delegation_id, logical_effect_id, quota_snapshot_id,
  continuation_id, claim_event_id, claim_receipt_id, technical_resource_kind,
  unit, amount_reserved, idempotency_key, continuation_fencing_token,
  runtime_host_generation, runtime_fencing_token, created_at, expires_at
ON execass_technical_resource_reservations BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource reservation identity is immutable');
END;

CREATE TRIGGER execass_technical_resource_reservation_status_guard
BEFORE UPDATE OF status,settled_at ON execass_technical_resource_reservations
WHEN NOT (
  (OLD.status='reserved' AND NEW.status IN ('settled','released','expired','reconciliation_required')) OR
  (OLD.status='reconciliation_required' AND NEW.status IN ('settled','released'))
) OR (NEW.status='settled' AND NOT EXISTS (
  SELECT 1 FROM execass_technical_resource_actuals a
  WHERE a.reservation_id=OLD.reservation_id
    AND a.delegation_id=OLD.delegation_id
    AND a.continuation_fencing_token=OLD.continuation_fencing_token
    AND a.runtime_host_generation=OLD.runtime_host_generation
    AND a.runtime_fencing_token=OLD.runtime_fencing_token
)) OR (NEW.status IN ('released','expired') AND EXISTS (
  SELECT 1 FROM execass_technical_resource_actuals a WHERE a.reservation_id=OLD.reservation_id
))
BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource reservation transition is invalid');
END;

CREATE TRIGGER execass_technical_resource_reservations_no_delete
BEFORE DELETE ON execass_technical_resource_reservations BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource reservations cannot be deleted');
END;

CREATE TRIGGER execass_technical_resource_actual_insert_guard
BEFORE INSERT ON execass_technical_resource_actuals
WHEN length(NEW.evidence_digest)=0 OR NOT EXISTS (
  SELECT 1 FROM execass_technical_resource_reservations r
  WHERE r.reservation_id=NEW.reservation_id
    AND r.delegation_id=NEW.delegation_id
    AND r.status IN ('reserved','reconciliation_required')
    AND NEW.amount_actual <= r.amount_reserved
    AND NEW.continuation_fencing_token=r.continuation_fencing_token
    AND NEW.runtime_host_generation=r.runtime_host_generation
    AND NEW.runtime_fencing_token=r.runtime_fencing_token
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource actual lacks exact live reservation provenance');
END;

CREATE TRIGGER execass_technical_resource_actuals_immutable
BEFORE UPDATE ON execass_technical_resource_actuals BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource actuals are immutable');
END;

CREATE TRIGGER execass_technical_resource_actuals_no_delete
BEFORE DELETE ON execass_technical_resource_actuals BEGIN
  SELECT RAISE(ABORT, 'ExecAss technical resource actuals cannot be deleted');
END;

CREATE TRIGGER execass_outbox_cursor_monotonic
BEFORE UPDATE ON execass_outbox_cursors
WHEN NEW.principal_id != OLD.principal_id OR NEW.client_id_digest != OLD.client_id_digest
  OR NEW.last_global_sequence < OLD.last_global_sequence OR NEW.cursor_revision <= OLD.cursor_revision BEGIN
  SELECT RAISE(ABORT, 'ExecAss outbox cursor must advance monotonically');
END;

CREATE TRIGGER execass_outbox_cursors_no_delete
BEFORE DELETE ON execass_outbox_cursors BEGIN
  SELECT RAISE(ABORT, 'ExecAss outbox cursors cannot be deleted online');
END;

CREATE TRIGGER execass_outbox_events_immutable_identity
BEFORE UPDATE OF global_sequence, event_id, event_name, aggregate_id, aggregate_revision,
  correlation_id, causation_id, occurred_at, schema_version, safe_payload_json,
  duplicate_identity
ON execass_outbox_events BEGIN
  SELECT RAISE(ABORT, 'ExecAss outbox event identity is immutable');
END;

CREATE TRIGGER execass_outbox_global_sequence_gap_free
AFTER INSERT ON execass_outbox_events
WHEN NEW.global_sequence != COALESCE((
  SELECT global_sequence + 1 FROM execass_outbox_events
  WHERE global_sequence < NEW.global_sequence
  ORDER BY global_sequence DESC LIMIT 1
), 1) BEGIN
  SELECT RAISE(ABORT, 'ExecAss outbox global sequence must be gap-free');
END;

CREATE TRIGGER execass_outbox_events_no_delete
BEFORE DELETE ON execass_outbox_events BEGIN
  SELECT RAISE(ABORT, 'ExecAss outbox events cannot be deleted online');
END;

CREATE TRIGGER execass_summary_deliveries_immutable
BEFORE UPDATE ON execass_summary_deliveries BEGIN
  SELECT RAISE(ABORT, 'ExecAss summary deliveries are immutable');
END;

CREATE TRIGGER execass_summary_deliveries_no_delete
BEFORE DELETE ON execass_summary_deliveries BEGIN
  SELECT RAISE(ABORT, 'ExecAss summary deliveries cannot be deleted');
END;

CREATE TRIGGER execass_summary_delivery_items_immutable
BEFORE UPDATE ON execass_summary_delivery_items BEGIN
  SELECT RAISE(ABORT, 'ExecAss delivered item sets are immutable');
END;

CREATE TRIGGER execass_summary_delivery_items_no_delete
BEFORE DELETE ON execass_summary_delivery_items BEGIN
  SELECT RAISE(ABORT, 'ExecAss delivered item sets cannot be deleted');
END;

CREATE TRIGGER execass_notifications_identity_immutable
BEFORE UPDATE ON execass_notifications
WHEN NEW.notification_id != OLD.notification_id
  OR NEW.attention_id IS NOT OLD.attention_id
  OR NEW.completion_assessment_id IS NOT OLD.completion_assessment_id
  OR NEW.outbox_event_id != OLD.outbox_event_id
  OR NEW.delegation_id != OLD.delegation_id
  OR NEW.decision_id IS NOT OLD.decision_id
  OR NEW.reason_revision != OLD.reason_revision
  OR NEW.attention_variant IS NOT OLD.attention_variant
  OR NEW.reason != OLD.reason
  OR NEW.channel != OLD.channel
  OR NEW.safe_payload_json != OLD.safe_payload_json
  OR NEW.requested_at != OLD.requested_at
  OR NEW.scheduled_at != OLD.scheduled_at
  OR NEW.quiet_hours_json IS NOT OLD.quiet_hours_json
  OR NEW.idempotency_key != OLD.idempotency_key
BEGIN
  SELECT RAISE(ABORT, 'ExecAss notification identity is immutable');
END;

CREATE TRIGGER execass_notifications_monotonic
BEFORE UPDATE ON execass_notifications
WHEN NEW.reminder_count < OLD.reminder_count
  OR NEW.reminder_count > OLD.reminder_count + 1
  OR NEW.updated_at < OLD.updated_at
  OR (OLD.last_reminded_at IS NOT NULL
      AND (NEW.last_reminded_at IS NULL OR NEW.last_reminded_at < OLD.last_reminded_at))
  OR (OLD.status='cancelled' AND NEW.status!='cancelled')
  OR (NEW.status='cancelled' AND NEW.next_reminder_at IS NOT NULL)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss notification reminder state must advance monotonically');
END;

CREATE TRIGGER execass_notifications_no_delete
BEFORE DELETE ON execass_notifications BEGIN
  SELECT RAISE(ABORT, 'ExecAss notifications cannot be deleted');
END;

CREATE TRIGGER execass_summary_acknowledgements_immutable
BEFORE UPDATE ON execass_summary_acknowledgements BEGIN
  SELECT RAISE(ABORT, 'ExecAss acknowledgements are immutable');
END;

CREATE TRIGGER execass_summary_acknowledgements_no_delete
BEFORE DELETE ON execass_summary_acknowledgements BEGIN
  SELECT RAISE(ABORT, 'ExecAss acknowledgements cannot be deleted');
END;

CREATE TRIGGER execass_runtime_generation_identity_immutable
BEFORE UPDATE OF generation, ownership_scope, state_root_generation,
  installation_identity, os_user_identity_digest, host_instance_id, started_at
ON execass_runtime_host_generations BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime host generation identity is immutable');
END;

CREATE TRIGGER execass_runtime_lease_identity_immutable
BEFORE UPDATE OF lease_id, ownership_scope, generation, host_instance_id,
  fencing_token, acquired_at, expires_at
ON execass_runtime_host_leases BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime host lease identity is immutable');
END;

CREATE TRIGGER execass_runtime_generation_terminal_irreversible
BEFORE UPDATE OF ended_at,end_reason ON execass_runtime_host_generations
WHEN OLD.ended_at IS NOT NULL
  AND (NEW.ended_at IS NOT OLD.ended_at OR NEW.end_reason IS NOT OLD.end_reason)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss ended runtime generation cannot be reopened or rewritten');
END;

CREATE TRIGGER execass_runtime_lease_release_irreversible
BEFORE UPDATE OF released_at ON execass_runtime_host_leases
WHEN OLD.released_at IS NOT NULL AND NEW.released_at IS NOT OLD.released_at
BEGIN
  SELECT RAISE(ABORT, 'ExecAss released runtime host lease cannot be reopened or rewritten');
END;

CREATE TRIGGER execass_runtime_host_state_identity_immutable
BEFORE UPDATE OF generation,host_instance_id,fencing_token,state_root_generation
ON execass_runtime_host_states BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime host state identity is immutable');
END;

CREATE TRIGGER execass_runtime_host_state_no_delete
BEFORE DELETE ON execass_runtime_host_states BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime host state cannot be deleted');
END;

CREATE TRIGGER execass_runtime_settings_immutable
BEFORE UPDATE ON execass_runtime_settings_revisions BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime settings revisions are immutable');
END;

CREATE TRIGGER execass_runtime_settings_no_delete
BEFORE DELETE ON execass_runtime_settings_revisions BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime settings revisions cannot be deleted');
END;

CREATE TRIGGER execass_policy_revisions_immutable
BEFORE UPDATE ON execass_policy_revisions BEGIN
  SELECT RAISE(ABORT, 'ExecAss policy revisions are immutable');
END;

CREATE TRIGGER execass_policy_revisions_no_delete
BEFORE DELETE ON execass_policy_revisions BEGIN
  SELECT RAISE(ABORT, 'ExecAss policy revisions cannot be deleted');
END;

CREATE TRIGGER execass_policy_owner_provenance_guard
BEFORE INSERT ON execass_policy_revisions
WHEN NEW.policy_revision > 1 AND NOT EXISTS (
  SELECT 1 FROM execass_authority_provenance authority
  JOIN execass_owner_ingress_bindings ingress
    ON ingress.actor_type=authority.actor_type
    AND ingress.credential_identity=authority.credential_identity
    AND ingress.authenticated_ingress=authority.authenticated_ingress
    AND ingress.channel_assurance=authority.channel_assurance
    AND ingress.status='active'
  WHERE authority.authority_provenance_id=NEW.authority_provenance_id
    AND authority.actor_type IN ('human_local','human_remote')
    AND authority.authority_kind='policy_snapshot'
    AND authority.policy_revision=NEW.policy_revision
    AND authority.source_correlation_id=(SELECT correlation_id FROM execass_outbox_events WHERE event_id=NEW.outbox_event_id)
    AND json_extract(authority.normalized_scope_json,'$.policy_revision')=NEW.policy_revision
    AND json_extract(authority.normalized_scope_json,'$.policy_snapshot_digest')=NEW.policy_snapshot_digest
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss policy revision requires exact active-owner provenance');
END;

CREATE TRIGGER execass_policy_revision_progression_guard
BEFORE INSERT ON execass_policy_revisions
WHEN NEW.policy_revision > 1 AND NEW.policy_revision != (
  SELECT current_policy_revision + 1 FROM execass_global_runtime_control WHERE singleton=1
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss policy revision must advance the canonical pointer exactly once');
END;

CREATE TRIGGER execass_runtime_settings_owner_provenance_guard
BEFORE INSERT ON execass_runtime_settings_revisions
WHEN NOT EXISTS (
  SELECT 1 FROM execass_authority_provenance authority
  JOIN execass_owner_ingress_bindings ingress
    ON ingress.actor_type=authority.actor_type
    AND ingress.credential_identity=authority.credential_identity
    AND ingress.authenticated_ingress=authority.authenticated_ingress
    AND ingress.channel_assurance=authority.channel_assurance
    AND ingress.status='active'
  WHERE authority.authority_provenance_id=NEW.authority_provenance_id
    AND authority.actor_type IN ('human_local','human_remote')
    AND authority.authority_kind='runtime_settings_snapshot'
    AND authority.policy_revision=NEW.policy_revision
    AND authority.source_correlation_id=(SELECT correlation_id FROM execass_outbox_events WHERE event_id=NEW.outbox_event_id)
    AND json_extract(authority.normalized_scope_json,'$.settings_revision')=NEW.settings_revision
    AND json_extract(authority.normalized_scope_json,'$.settings_digest')=NEW.settings_digest
)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss runtime settings revision requires exact active-owner provenance');
END;

CREATE TRIGGER execass_authority_links_immutable
BEFORE UPDATE ON execass_authority_links BEGIN
  SELECT RAISE(ABORT, 'ExecAss authority links are immutable');
END;

CREATE TRIGGER execass_authority_links_no_delete
BEFORE DELETE ON execass_authority_links BEGIN
  SELECT RAISE(ABORT, 'ExecAss authority links cannot be deleted');
END;

CREATE TRIGGER execass_authority_links_insert_guard
BEFORE INSERT ON execass_authority_links BEGIN
  SELECT CASE WHEN NEW.delegation_state_revision != (
    SELECT state_revision FROM execass_delegations WHERE delegation_id = NEW.delegation_id
  ) THEN RAISE(ABORT, 'authority link must bind the resulting delegation revision') END;
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM execass_outbox_events
    WHERE event_id = NEW.outbox_event_id
      AND aggregate_id = NEW.delegation_id
      AND aggregate_revision = NEW.delegation_state_revision
      AND correlation_id = NEW.correlation_id
      AND causation_id = NEW.causation_id
      AND event_name = 'execass.v1.delegation.transitioned'
      AND occurred_at = NEW.linked_at
  ) THEN RAISE(ABORT, 'authority link must bind its exact delegation outbox event') END;
  SELECT CASE WHEN NEW.link_revision != COALESCE((
    SELECT MAX(link_revision) + 1 FROM execass_authority_links
    WHERE delegation_id = NEW.delegation_id
  ), 1) THEN RAISE(ABORT, 'authority link revision must be gap-free') END;
  SELECT CASE WHEN NEW.authority_kind = 'security_audit_event' AND NOT EXISTS (
    SELECT 1 FROM security_audit_events WHERE event_id = NEW.security_audit_event_id
    UNION ALL
    SELECT 1 FROM security_audit_events_archive WHERE event_id = NEW.security_audit_event_id
  ) THEN RAISE(ABORT, 'security audit authority source is missing') END;
END;

CREATE TRIGGER execass_security_audit_archive_no_delete_when_linked
BEFORE DELETE ON security_audit_events_archive
WHEN EXISTS (
  SELECT 1 FROM execass_authority_links
  WHERE authority_kind = 'security_audit_event'
    AND security_audit_event_id = OLD.event_id
)
BEGIN
  SELECT RAISE(ABORT, 'linked archived security audit event cannot be deleted');
END;

CREATE TRIGGER execass_security_audit_live_no_delete_without_archive
BEFORE DELETE ON security_audit_events
WHEN EXISTS (
  SELECT 1 FROM execass_authority_links
  WHERE authority_kind = 'security_audit_event'
    AND security_audit_event_id = OLD.event_id
)
AND NOT EXISTS (
  SELECT 1 FROM security_audit_events_archive WHERE event_id = OLD.event_id
)
BEGIN
  SELECT RAISE(ABORT, 'linked security audit event must be archived before deletion');
END;

CREATE TRIGGER execass_lifecycle_transitions_immutable
BEFORE UPDATE ON execass_lifecycle_transitions BEGIN
  SELECT RAISE(ABORT, 'ExecAss lifecycle transitions are immutable');
END;

CREATE TRIGGER execass_lifecycle_transitions_no_delete
BEFORE DELETE ON execass_lifecycle_transitions BEGIN
  SELECT RAISE(ABORT, 'ExecAss lifecycle transitions cannot be deleted');
END;

CREATE TRIGGER execass_completion_assessments_immutable
BEFORE UPDATE ON execass_completion_assessments BEGIN
  SELECT RAISE(ABORT, 'ExecAss completion assessments are immutable');
END;

CREATE TRIGGER execass_completion_assessments_no_delete
BEFORE DELETE ON execass_completion_assessments BEGIN
  SELECT RAISE(ABORT, 'ExecAss completion assessments cannot be deleted');
END;

CREATE TRIGGER execass_terminal_corrections_immutable
BEFORE UPDATE ON execass_terminal_corrections BEGIN
  SELECT RAISE(ABORT, 'ExecAss terminal corrections are immutable');
END;

CREATE TRIGGER execass_terminal_corrections_no_delete
BEFORE DELETE ON execass_terminal_corrections BEGIN
  SELECT RAISE(ABORT, 'ExecAss terminal corrections cannot be deleted');
END;

CREATE TRIGGER execass_amendment_criteria_links_immutable
BEFORE UPDATE ON execass_amendment_criteria_links BEGIN
  SELECT RAISE(ABORT, 'ExecAss amendment criteria links are immutable');
END;

CREATE TRIGGER execass_amendment_criteria_links_no_delete
BEFORE DELETE ON execass_amendment_criteria_links BEGIN
  SELECT RAISE(ABORT, 'ExecAss amendment criteria links cannot be deleted');
END;

CREATE TRIGGER execass_recovery_episodes_immutable
BEFORE UPDATE ON execass_recovery_episodes BEGIN
  SELECT RAISE(ABORT, 'ExecAss recovery episodes are immutable');
END;

CREATE TRIGGER execass_recovery_episodes_no_delete
BEFORE DELETE ON execass_recovery_episodes BEGIN
  SELECT RAISE(ABORT, 'ExecAss recovery episodes cannot be deleted');
END;

CREATE TRIGGER execass_recovery_evaluations_immutable
BEFORE UPDATE ON execass_recovery_evaluations BEGIN
  SELECT RAISE(ABORT, 'ExecAss recovery evaluations are immutable');
END;

CREATE TRIGGER execass_recovery_evaluations_no_delete
BEFORE DELETE ON execass_recovery_evaluations BEGIN
  SELECT RAISE(ABORT, 'ExecAss recovery evaluations cannot be deleted');
END;

CREATE TRIGGER execass_delegation_criteria_set_guard
BEFORE UPDATE OF current_criteria_revision ON execass_delegations
WHEN NEW.current_criteria_revision IS NOT NULL
AND NOT EXISTS (SELECT 1 FROM execass_criteria_sets WHERE delegation_id=NEW.delegation_id AND criteria_revision=NEW.current_criteria_revision)
BEGIN
  SELECT RAISE(ABORT, 'delegation current criteria revision must reference a real criteria set');
END;

-- EA-216 saved-routine substrate. These rows only reserve typed trigger jobs
-- and a non-executable admission plan; the lifecycle owner remains the sole
-- creator of delegations/continuations.
CREATE TABLE execass_routines (
  routine_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version > 0),
  enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
  timezone TEXT NOT NULL,
  overlap_policy TEXT NOT NULL CHECK (overlap_policy IN ('earlier','later')),
  catch_up_policy TEXT NOT NULL CHECK (catch_up_policy IN ('skip','latest_only','replay')),
  replay_cap INTEGER NOT NULL CHECK (replay_cap BETWEEN 1 AND 10),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL CHECK (updated_at >= created_at)
);

CREATE TABLE execass_routine_versions (
  routine_id TEXT NOT NULL REFERENCES execass_routines(routine_id) ON DELETE RESTRICT,
  routine_version INTEGER NOT NULL CHECK (routine_version > 0),
  source_delegation_id TEXT NOT NULL REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  saved_owner_authority_provenance_id TEXT NOT NULL REFERENCES execass_authority_provenance(authority_provenance_id) ON DELETE RESTRICT,
  normalized_original_intent TEXT NOT NULL,
  resolved_leaf_manifest_json TEXT NOT NULL CHECK (json_valid(resolved_leaf_manifest_json)),
  manifest_digest TEXT NOT NULL CHECK (length(manifest_digest)=64 AND manifest_digest NOT GLOB '*[^0-9a-f]*'),
  saved_selector_json TEXT NOT NULL CHECK (json_valid(saved_selector_json)),
  saved_action_envelope_json TEXT NOT NULL CHECK (json_valid(saved_action_envelope_json)),
  accepted_confirmation_grant_id TEXT REFERENCES execass_accepted_confirmation_grants(grant_id) ON DELETE RESTRICT,
  effective_policy_snapshot_json TEXT NOT NULL CHECK (json_valid(effective_policy_snapshot_json)),
  effective_policy_revision INTEGER NOT NULL CHECK (effective_policy_revision > 0),
  stable_leaf_digest TEXT NOT NULL CHECK (length(stable_leaf_digest)=64 AND stable_leaf_digest NOT GLOB '*[^0-9a-f]*'),
  created_at INTEGER NOT NULL,
  PRIMARY KEY (routine_id,routine_version),
  FOREIGN KEY (source_delegation_id,manifest_digest)
    REFERENCES execass_plans(delegation_id,manifest_digest) ON DELETE RESTRICT
);

CREATE TABLE execass_routine_schedule_state (
  routine_id TEXT PRIMARY KEY REFERENCES execass_routines(routine_id) ON DELETE RESTRICT,
  local_hour INTEGER NOT NULL CHECK (local_hour BETWEEN 0 AND 23),
  local_minute INTEGER NOT NULL CHECK (local_minute BETWEEN 0 AND 59),
  last_evaluated_instant_ms INTEGER,
  updated_at INTEGER NOT NULL
);

CREATE TABLE execass_routine_occurrences (
  occurrence_id TEXT PRIMARY KEY,
  routine_id TEXT NOT NULL,
  routine_version INTEGER NOT NULL,
  scheduled_instant_ms INTEGER NOT NULL,
  scheduled_local TEXT NOT NULL,
  utc_offset_seconds INTEGER NOT NULL,
  time_resolution TEXT NOT NULL CHECK (time_resolution IN ('single','earlier','later','gap_advanced')),
  effective_policy_revision INTEGER NOT NULL CHECK (effective_policy_revision > 0),
  status TEXT NOT NULL CHECK (status IN ('planned','admission_planned','skipped','settled')),
  admission_plan_json TEXT CHECK (admission_plan_json IS NULL OR json_valid(admission_plan_json)),
  admitted_delegation_id TEXT UNIQUE REFERENCES execass_delegations(delegation_id) ON DELETE RESTRICT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
  UNIQUE(routine_id,scheduled_instant_ms),
  FOREIGN KEY(routine_id,routine_version)
    REFERENCES execass_routine_versions(routine_id,routine_version) ON DELETE RESTRICT
);

CREATE TABLE execass_routine_job_bindings (
  occurrence_id TEXT PRIMARY KEY REFERENCES execass_routine_occurrences(occurrence_id) ON DELETE RESTRICT,
  job_id TEXT NOT NULL UNIQUE REFERENCES jobs(job_id) ON DELETE RESTRICT,
  created_at INTEGER NOT NULL
);

CREATE TABLE execass_routine_driver_jobs (
  routine_id TEXT PRIMARY KEY REFERENCES execass_routines(routine_id) ON DELETE RESTRICT,
  job_id TEXT NOT NULL UNIQUE REFERENCES jobs(job_id) ON DELETE RESTRICT,
  created_at INTEGER NOT NULL
);

CREATE TABLE execass_routine_trigger_operations (
  operation_id TEXT PRIMARY KEY,
  occurrence_id TEXT UNIQUE REFERENCES execass_routine_occurrences(occurrence_id) ON DELETE RESTRICT,
  job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE RESTRICT,
  operation TEXT NOT NULL CHECK (operation IN ('settle_trigger','advance_driver')),
  lease_owner TEXT,
  lease_expires_at INTEGER,
  occurred_at INTEGER NOT NULL,
  CHECK (lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL),
  CHECK ((operation='settle_trigger') = (occurrence_id IS NOT NULL))
);

CREATE TRIGGER execass_routine_versions_immutable
BEFORE UPDATE ON execass_routine_versions BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine versions are immutable');
END;
CREATE TRIGGER execass_routine_versions_no_delete
BEFORE DELETE ON execass_routine_versions BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine versions cannot be deleted');
END;
CREATE TRIGGER execass_routine_occurrence_identity_immutable
BEFORE UPDATE OF occurrence_id,routine_id,routine_version,scheduled_instant_ms,scheduled_local,utc_offset_seconds,time_resolution,effective_policy_revision,created_at ON execass_routine_occurrences BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine occurrence identity is immutable');
END;
CREATE TRIGGER execass_routine_occurrences_no_delete
BEFORE DELETE ON execass_routine_occurrences BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine occurrences cannot be deleted');
END;
CREATE TRIGGER execass_routine_job_bindings_immutable
BEFORE UPDATE ON execass_routine_job_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine job bindings are immutable');
END;
CREATE TRIGGER execass_routine_job_bindings_no_delete
BEFORE DELETE ON execass_routine_job_bindings BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine job bindings cannot be deleted');
END;
CREATE TRIGGER execass_routine_driver_jobs_immutable
BEFORE UPDATE ON execass_routine_driver_jobs BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine driver bindings are immutable');
END;
CREATE TRIGGER execass_routine_driver_jobs_no_delete
BEFORE DELETE ON execass_routine_driver_jobs BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine driver bindings cannot be deleted');
END;
CREATE TRIGGER execass_routine_trigger_operations_immutable
BEFORE UPDATE ON execass_routine_trigger_operations BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine trigger operations are immutable');
END;
CREATE TRIGGER execass_routine_trigger_operations_no_delete
BEFORE DELETE ON execass_routine_trigger_operations BEGIN
  SELECT RAISE(ABORT, 'ExecAss routine trigger operations cannot be deleted');
END;
CREATE TRIGGER execass_reserved_routine_job_immutable
BEFORE UPDATE OF agent_id,name,enabled,schedule_kind,interval_seconds,run_at_ms,next_run_at,payload_json,max_retries,retry_backoff_ms,timeout_ms,deleted_at ON jobs
WHEN (EXISTS (SELECT 1 FROM execass_routine_job_bindings WHERE job_id=OLD.job_id)
      OR EXISTS (SELECT 1 FROM execass_routine_driver_jobs WHERE job_id=OLD.job_id))
 AND NOT (
   EXISTS (SELECT 1 FROM execass_routine_job_bindings WHERE job_id=OLD.job_id)
   AND NEW.enabled=0 AND NEW.next_run_at IS NULL AND NEW.lease_owner IS NULL AND NEW.lease_expires_at IS NULL
   AND NEW.agent_id=OLD.agent_id AND NEW.name=OLD.name AND NEW.schedule_kind=OLD.schedule_kind
   AND NEW.interval_seconds IS OLD.interval_seconds AND NEW.run_at_ms IS OLD.run_at_ms
   AND NEW.payload_json=OLD.payload_json AND NEW.max_retries=OLD.max_retries
   AND NEW.retry_backoff_ms=OLD.retry_backoff_ms AND NEW.timeout_ms=OLD.timeout_ms
   AND NEW.deleted_at IS OLD.deleted_at
   AND EXISTS (SELECT 1 FROM execass_routine_trigger_operations
               WHERE job_id=OLD.job_id AND operation='settle_trigger' AND occurred_at=NEW.updated_at)
 )
 AND NOT (
   EXISTS (SELECT 1 FROM execass_routine_driver_jobs WHERE job_id=OLD.job_id)
   AND NEW.next_run_at=NEW.updated_at+60000
   AND NEW.lease_owner IS NULL AND NEW.lease_expires_at IS NULL
   AND EXISTS (SELECT 1 FROM execass_routine_trigger_operations
               WHERE job_id=OLD.job_id AND operation='advance_driver' AND occurred_at=NEW.updated_at)
 )
BEGIN
  SELECT RAISE(ABORT, 'ExecAss reserved routine trigger job is immutable');
END;
CREATE TRIGGER execass_reserved_routine_job_no_delete
BEFORE DELETE ON jobs
WHEN EXISTS (SELECT 1 FROM execass_routine_job_bindings WHERE job_id=OLD.job_id)
   OR EXISTS (SELECT 1 FROM execass_routine_driver_jobs WHERE job_id=OLD.job_id)
BEGIN
  SELECT RAISE(ABORT, 'ExecAss reserved routine trigger job cannot be deleted');
END;
