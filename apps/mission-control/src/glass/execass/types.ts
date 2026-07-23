/**
 * Hand-mapped TypeScript types for the ExecAss v1.1 wire contract.
 *
 * Source of truth: contracts/execass/v1/schema/*.json (generated from
 * crates/carsinos-protocol/src/execass.rs). These types mirror the checked
 * JSON Schemas exactly; do not invent fields or defaults here.
 */

// ————————————————————————————————————————————————— shared enums

export type DelegationPhase =
  | "accepted"
  | "planning"
  | "in_motion"
  | "waiting_for_user"
  | "waiting_external"
  | "recovering"
  | "completed"
  | "partially_completed"
  | "failed";

export type RunControlState = "running" | "stop_requested" | "stopped";

export type DecisionKind =
  | "clarification"
  | "dangerous_action_confirmation"
  | "owner_configured_checkpoint"
  | "recovery_choice"
  | "duplicate_risk_retry"
  | "stop"
  | "policy_change";

export type DecisionResult = "confirm_and_continue" | "revise" | "decline" | "stop";

export type DecisionStatus = "pending" | "resolved" | "superseded" | "expired";

export type AssuranceRequirement =
  | "verified_owner_resolution"
  | "mechanical_resolution";

export type AttentionKind =
  | "confirmation"
  | "clarification"
  | "reply"
  | "recovery_choice"
  | "runtime_paused";

export type NextItemKind = "routine" | "commitment" | "deadline" | "follow_up";

export type TechnicalResourceKind =
  | "tokens"
  | "time_ms"
  | "connector_calls"
  | "resource_units";

export type RuntimeHostActualState =
  | "stopped"
  | "starting"
  | "running_app_bound"
  | "handoff"
  | "running_background"
  | "draining"
  | "faulted";

export type RuntimeHostDesiredMode = "app_bound" | "background";

export type OwnerResolutionIngress =
  | "local_owner_session"
  | "authenticated_remote_owner_channel";

export type AutonomyProfile = "locked_down" | "balanced" | "full_send" | "custom";

// ————————————————————————————————————————————————— shared structs

export interface TechnicalResourceQuota {
  kind: TechnicalResourceKind;
  limit: number;
  reserved: number;
  consumed: number;
}

export interface OwnerResolutionSummary {
  ingress: OwnerResolutionIngress;
  verified_evidence_ref: string;
}

export interface AcceptedConfirmationGrant {
  delegation_id: string;
  normalized_intent: string;
  confirmed_logical_action_identity: string;
  canonical_action_envelope_or_selector: string;
  payload_and_material_operands_digest: string;
  connector_or_tool_identity_and_version: string;
  declared_consequence: string;
}

export interface DecisionChallenge {
  decision_revision: number;
  exact_presented_action_or_alternative: string;
  declared_consequence: string;
  nonce_or_token: string;
  expires_at_ms: number;
}

/** Server-derived facts a native owner signer needs for one decision. */
export interface DecisionProofChallenge {
  decision_id: string;
  decision_revision: number;
  normalized_intent_digest: string;
  policy_revision: number;
  canonical_manifest_digest: string;
  selected_logical_action_id: string;
  presented_action_digest: string;
  declared_consequence_digest: string;
  challenge_digest: string;
  expires_at_ms: number;
}

export interface DecisionSummary {
  decision_id: string;
  delegation_id: string;
  revision: number;
  status: DecisionStatus;
  kind: DecisionKind;
  assurance_required: AssuranceRequirement;
  recommendation: string;
  why_now: string;
  consequence: string;
  alternatives: string[];
  exact_manifest_digest: string;
  technical_resources: TechnicalResourceQuota[];
  requested_at_ms: number;
  authoritative_deep_link: string;
  accepted_confirmation_grant?: AcceptedConfirmationGrant | null;
  challenge?: DecisionChallenge | null;
  local_owner_proof_challenge?: DecisionProofChallenge | null;
  resolved_at_ms?: number | null;
  resolved_owner?: OwnerResolutionSummary | null;
  result?: DecisionResult | null;
}

export interface DelegationSummary {
  delegation_id: string;
  phase: DelegationPhase;
  run_control: RunControlState;
  state_revision: number;
  intent_summary: string;
  outcome_summary: string;
  policy_revision: number;
  stop_epoch: number;
  created_at_ms: number;
  updated_at_ms: number;
  authoritative_deep_link: string;
  acknowledged_at_ms?: number | null;
  pending_decision?: DecisionSummary | null;
  pending_external_wait?: string | null;
  terminal_at_ms?: number | null;
}

export type AttentionSubject =
  | {
      scope_kind: "delegation";
      delegation_id: string;
      delegation_revision: number;
    }
  | {
      scope_kind: "runtime_host";
      runtime_host_generation: number;
      runtime_host_instance_id: string;
      runtime_fencing_token: number;
      runtime_actual_state: RuntimeHostActualState;
      runtime_end_reason: string;
      active_work_binding_digest: string;
    };

export interface AttentionItem {
  attention_id: string;
  kind: AttentionKind;
  subject: AttentionSubject;
  reason: string;
  recommendation: string;
  alternatives_or_actions: string[];
  assurance_required: AssuranceRequirement;
  deadline_reminder_state: string;
  authoritative_deep_link: string;
  deadline_at_ms?: number | null;
  decision_id?: string | null;
  /** Non-decision attention (reply, runtime_paused) serializes this as null. */
  decision_kind?: DecisionKind | null;
  decision_revision?: number | null;
}

export interface NextItem {
  next_item_id: string;
  kind: NextItemKind;
  summary: string;
  authoritative_deep_link: string;
  delegation_id?: string | null;
  due_at_ms?: number | null;
  scheduled_for_ms?: number | null;
}

export type ReceiptScope =
  | { scope_kind: "delegation"; delegation_id: string; delegation_sequence: number }
  | { scope_kind: "runtime_host"; runtime_host_aggregate_id: string };

export interface ReceiptEvidenceSummary {
  authority_kind: string;
  source_id: string;
  authoritative_revision: number;
  authority_link_id: string;
  observation_digest: string;
  deep_link: string;
}

export interface ReceiptSummary {
  receipt_id: string;
  scope: ReceiptScope;
  global_sequence: number;
  receipt_kind: string;
  subject_kind: string;
  subject_id: string;
  subject_revision: number;
  occurred_at_ms: number;
  committed_at_ms: number;
  evidence_refs: ReceiptEvidenceSummary[];
  receipt_digest: string;
  key_id: string;
  key_generation: number;
  integrity_tag: string;
  safe_summary: string;
  delegation_previous_receipt_digest?: string | null;
  global_previous_receipt_digest?: string | null;
  previous_key_integrity_tag?: string | null;
}

export interface DeliveredItem {
  item_id: string;
  revision: number;
}

export interface SummaryCursor {
  cursor: string;
  displayed_at_ms: number;
  delivered: DeliveredItem[];
}

// ————————————————————————————————————————————————— summary

export interface SummaryResponse {
  needs_you: AttentionItem[];
  in_motion: DelegationSummary[];
  done: DelegationSummary[];
  next: NextItem[];
  receipts: ReceiptSummary[];
  displayed: SummaryCursor;
}

export interface SummaryAckRequest {
  idempotency_key: string;
  displayed: SummaryCursor;
}

export interface SummaryAckResponse {
  acknowledged: boolean;
  displayed: SummaryCursor;
  acknowledged_at_ms: number;
}

// ————————————————————————————————————————————————— intake

export interface IntakeRequest {
  request_id: string;
  idempotency_key: string;
  text: string;
  source_correlation_id: string;
  attach_to_delegation_id?: string | null;
}

export type IntakeResponse =
  | { kind: "conversational"; response_text: string; request_audit_ref: string }
  | { kind: "delegation"; delegation: DelegationSummary; created: boolean };

/** HMAC intake proof returned by the native shell; sent base64url in X-ExecAss-Owner-Proof. */
export interface LocalOwnerIntakeProof {
  authenticated_client_id: string;
  request_correlation_id: string;
  request_id: string;
  idempotency_key: string;
  attach_to_delegation_id?: string | null;
  normalized_intent_digest: string;
  instruction_digest: string;
  proof_hex: string;
}

// ————————————————————————————————————————————————— delegations

export interface DelegationListQuery {
  cursor?: string | null;
  limit?: number | null;
  phase?: DelegationPhase | null;
  run_control?: RunControlState | null;
}

export interface DelegationListResponse {
  items: DelegationSummary[];
  next_cursor?: string | null;
}

export type BranchState =
  | "runnable"
  | "executing"
  | "waiting"
  | "uncertain"
  | "terminal";

export type ContinuationStatus =
  | "pending"
  | "runnable"
  | "claimed"
  | "waiting"
  | "completed"
  | "failed"
  | "superseded";

export type EffectStatus =
  | "planned"
  | "claimed"
  | "succeeded"
  | "failed"
  | "outcome_unknown"
  | "unresolved";

export type VerifierResult = "pass" | "fail" | "unknown";

export type DangerSource = "known_category" | "model_credible_danger";

export type KnownDangerCategory =
  | "whole_drive_volume_boot_recovery_or_core_os_tree_erasure_or_unusable"
  | "whole_user_profile_or_home_erasure_or_unusable"
  | "complete_carsinos_state_integrity_runtime_enforcement_stop_fencing_or_recovery_configuration_erasure_or_unusable"
  | "whole_connected_external_account_closure_or_erasure"
  | "last_verified_administrative_recovery_or_decryption_path_destruction";

export type ObjectiveRetrySafetyFact =
  | "attempt_count"
  | "elapsed_time"
  | "backoff"
  | "technical_resource_quota"
  | "circuit_breakers"
  | "provider_error_class"
  | "idempotency"
  | "independent_absence_or_reconciliation_proof"
  | "reversibility"
  | "declared_safe_boundary";

export interface DangerAssessment {
  declared_consequence: string;
  requires_one_confirmation: boolean;
  source: DangerSource;
  known_category?: KnownDangerCategory | null;
}

export interface ActionSummary {
  action_id: string;
  branch_state: BranchState;
  danger_assessments: DangerAssessment[];
  manifest_digest: string;
  manifest_revision: number;
  requires_assurance: AssuranceRequirement;
  safe_boundary_description: string;
  technical_resources: TechnicalResourceQuota[];
  required_decision_kind?: DecisionKind | null;
}

export interface ContinuationSummary {
  continuation_id: string;
  delegation_id: string;
  plan_revision: number;
  policy_revision: number;
  safe_summary: string;
  status: ContinuationStatus;
  claimed_at_ms?: number | null;
  completed_at_ms?: number | null;
  scheduled_for_ms?: number | null;
}

export interface EffectSummary {
  action_id: string;
  effect_id: string;
  safe_summary: string;
  status: EffectStatus;
  external_reference?: string | null;
  occurred_at_ms?: number | null;
  provider_idempotency_key?: string | null;
}

export interface OutcomeCriterionSummary {
  criterion_id: string;
  expected_predicate: string;
  material: boolean;
  verifier_result: VerifierResult;
  verifier_type: string;
  authoritative_evidence_ref: string;
}

export interface VerifierSummary {
  verifier_id: string;
  criterion_id: string;
  verifier_type: string;
  result: VerifierResult;
  safe_summary: string;
  assessed_at_ms: number;
  authoritative_evidence_ref: string;
}

export interface RecoverySummary {
  recovery_id: string;
  action_id: string;
  automatic_retry_permitted: boolean;
  objective_retry_safety_facts: ObjectiveRetrySafetyFact[];
  outcome_unknown: boolean;
  safe_summary: string;
}

export interface DelegationDetail {
  delegation: DelegationSummary;
  original_intent: string;
  plan_summary: string;
  actions: ActionSummary[];
  continuations: ContinuationSummary[];
  effects: EffectSummary[];
  completion_verifiers: VerifierSummary[];
  outcome_criteria: OutcomeCriterionSummary[];
  authority_snapshot_ref: string;
  immutable_intake_evidence_ref: string;
  ingress_source: string;
  internal_record_refs: string[];
  source_correlation_id: string;
  technical_resource_summary: string;
  receipt_chain_head?: string | null;
  recovery?: RecoverySummary | null;
}

export interface DelegationDetailResponse {
  detail: DelegationDetail;
}

export interface DelegationReceiptsResponse {
  delegation_id: string;
  receipts: ReceiptSummary[];
  receipt_chain_head?: string | null;
}

// ————————————————————————————————————————————————— decisions

/** HMAC decision proof returned by the native shell. Authentication evidence only. */
export interface LocalDecisionProof {
  authenticated_client_id: string;
  request_correlation_id: string;
  proof_hex: string;
}

/**
 * Exact current decision facts authenticated by a LocalDecisionProof.
 * Derive every field from authoritative server state, never request input.
 */
export interface LocalDecisionProofBinding {
  decision_id: string;
  decision_revision: number;
  normalized_intent_digest: string;
  policy_revision: number;
  canonical_manifest_digest: string;
  selected_logical_action_id: string;
  presented_action_digest: string;
  declared_consequence_digest: string;
  challenge_digest: string;
  expires_at_ms: number;
  response_selected_logical_action_id: string;
  decision_result: DecisionResult;
  idempotency_key: string;
  observed_at_ms: number;
  challenge_response_digest?: string | null;
  revision_text_digest?: string | null;
}

export interface ResolveDecisionRequest {
  idempotency_key: string;
  decision_revision: number;
  result: DecisionResult;
  local_proof: LocalDecisionProof;
  local_proof_binding: LocalDecisionProofBinding;
  challenge_response?: string | null;
  revision_text?: string | null;
}

export interface ResolveDecisionResponse {
  decision: DecisionSummary;
  delegation: DelegationSummary;
  continuation_id?: string | null;
}

// ————————————————————————————————————————————————— run control

export type RunControlOperation =
  | "global_stop"
  | "global_resume"
  | "delegation_stop"
  | "delegation_resume";

export type RunControlTarget =
  | { kind: "global" }
  | { kind: "delegation"; delegation_id: string };

/** Exact server-provided state disclosed to a human before a resume request. */
export interface RunControlResumeSnapshot {
  stopped_epoch: number;
  current_policy_revision: number;
  unresolved_effect_disclosure_digest: string;
  current_plan_revision?: number | null;
  delegation_state_revision?: number | null;
}

export interface RunControlRequestBinding {
  operation: RunControlOperation;
  target: RunControlTarget;
  idempotency_key: string;
  request_correlation_id: string;
  observed_at_ms: number;
  resume?: RunControlResumeSnapshot | null;
}

/** HMAC run-control proof returned by the native shell. */
export interface LocalRunControlProof {
  authenticated_client_id: string;
  request_correlation_id: string;
  proof_hex: string;
}

export interface RunControlRequest {
  binding: RunControlRequestBinding;
  local_proof: LocalRunControlProof;
}

export interface UnresolvedExternalEffectRef {
  continuation_id: string;
  delegation_id: string;
  logical_effect_id: string;
  state: string;
  latest_attempt_id?: string | null;
}

export interface DelegationRunControlResponse {
  delegation_id: string;
  phase: DelegationPhase;
  run_control: RunControlState;
  drain_state: string;
  state_revision: number;
  stop_epoch: number;
  policy_revision: number;
  unresolved_effect_disclosure_digest: string;
  unresolved_external_effect_refs: UnresolvedExternalEffectRef[];
  current_plan_revision?: number | null;
}

export interface StopAllStatusResponse {
  engaged: boolean;
  drain_state: string;
  stop_epoch: number;
  current_policy_revision: number;
  unresolved_effect_disclosure_digest: string;
  unresolved_external_effect_refs: UnresolvedExternalEffectRef[];
}

export interface ResumeAllResponse {
  resumed_at_ms: number;
  stop_all: StopAllStatusResponse;
}

// ————————————————————————————————————————————————— policy

export interface PolicyRule {
  rule_id: string;
  technical_resource_quotas: TechnicalResourceQuota[];
  audience_scope?: string | null;
  clarification_sensitivity?: string | null;
  connector_or_tool_identity_and_version_scope?: string | null;
  expires_at_ms?: number | null;
  parallelism_limit?: number | null;
  recovery_limit?: number | null;
  recurring_work_scope?: string | null;
  routine_scope?: string | null;
  target_scope?: string | null;
  task_or_delegation_scope?: string | null;
  workspace_scope?: string | null;
}

export interface PolicyResponse {
  policy_id: string;
  configured: boolean;
  effective_operational_summary: string;
  revision: number;
  rules: PolicyRule[];
  updated_at_ms: number;
  profile?: AutonomyProfile | null;
}

export interface PolicyUpdateRequest {
  idempotency_key: string;
  expected_policy_revision: number;
  change_summary: string;
  proposed_profile: AutonomyProfile;
  proposed_rules: PolicyRule[];
}

export interface PolicyUpdateResponse {
  policy: PolicyResponse;
  updated_at_ms: number;
}

// ————————————————————————————————————————————————— runtime host

export interface RuntimeHostStatusResponse {
  actual_state: RuntimeHostActualState;
  desired_mode: RuntimeHostDesiredMode;
  fencing_generation: number;
  health: string;
  ownership_mode: string;
  state_root_version: string;
  process_id?: number | null;
  restart_reason?: string | null;
  started_at_ms?: number | null;
}

export interface RuntimeHostConfigRequest {
  idempotency_key: string;
  expected_settings_revision: number;
  desired_mode: RuntimeHostDesiredMode;
  start_at_login: boolean;
}

export interface RuntimeHostConfigResponse {
  bounded_settings_revision: number;
  start_at_login: boolean;
  status: RuntimeHostStatusResponse;
}

// ————————————————————————————————————————————————— owner mutation proofs

export type OwnerMutationOperation = "policy_update" | "runtime_host_config_update";

export interface LocalOwnerMutationBinding {
  operation: OwnerMutationOperation;
  method: string;
  path: string;
  idempotency_key: string;
  expected_revision: number;
  canonical_body_digest: string;
  safe_snapshot_digest: string;
  request_correlation_id: string;
  created_at_ms: number;
}

export interface LocalOwnerMutationProof {
  authenticated_client_id: string;
  request_correlation_id: string;
  proof_hex: string;
}

// ————————————————————————————————————————————————— errors

export type ApiErrorCode =
  | "execass.v1.invalid_request"
  | "execass.v1.authentication_required"
  | "execass.v1.idempotency_conflict"
  | "execass.v1.authority_denied"
  | "execass.v1.decision_assurance_required"
  | "execass.v1.decision_challenge_expired"
  | "execass.v1.not_found"
  | "execass.v1.revision_conflict"
  | "execass.v1.invalid_transition"
  | "execass.v1.stop_all_engaged"
  | "execass.v1.outcome_unknown_retry_prohibited"
  | "execass.v1.technical_resource_exhausted"
  | "execass.v1.receipt_integrity_quarantined"
  | "execass.v1.decision_superseded"
  | "execass.v1.runtime_host_conflict"
  | "execass.v1.schema_replace_requires_quiescence"
  | "execass.v1.rate_limited"
  | "execass.v1.external_dependency"
  | "execass.v1.schema_version_unsupported"
  | "execass.v1.internal_safe_failure";

export interface ApiError {
  code: ApiErrorCode;
  safe_human_message: string;
  retryable: boolean;
  correlation_id: string;
  safe_for_display: boolean;
  exposes_sensitive_metadata: boolean;
}

// ————————————————————————————————————————————————— websocket frames

export type ExecassEventName =
  | "execass.v1.delegation.transitioned"
  | "execass.v1.decision.recorded"
  | "execass.v1.continuation.claimed_or_result_recorded"
  | "execass.v1.recovery.updated"
  | "execass.v1.completion.assessed"
  | "execass.v1.summary.changed"
  | "execass.v1.policy.changed"
  | "execass.v1.runtime_host.changed"
  | "execass.v1.receipt.integrity_failed"
  | "execass.v1.notification.scheduled"
  | "execass.v1.global_stop.changed";

export interface SafeEventPayload {
  summary: string;
  authoritative_deep_link?: string | null;
  decision_id?: string | null;
  delegation_id?: string | null;
  receipt_ref?: string | null;
}

export interface DurableEventEnvelope {
  event_name: ExecassEventName;
  aggregate_id: string;
  revision: number;
  correlation_id: string;
  causation_id: string;
  occurred_at_ms: number;
  schema_version: string;
  safe_payload: SafeEventPayload;
  global_sequence: number;
  duplicate_identity: string;
}

/** Client → server resume frame sent after gateway.status on /api/v1/ws. */
export interface ExecassResumeFrame {
  type: "execass.v1.resume";
  client_id: string;
  cursor: number;
}

export interface ExecassEventFrame {
  type: "execass.v1.event";
  event: DurableEventEnvelope;
}

export interface ExecassSummaryRefetchRequiredFrame {
  type: "execass.v1.summary_refetch_required";
  reason: string;
  consumer_cursor: number;
  requested_cursor: number;
  head_global_sequence: number;
}

export type ExecassWsFrame = ExecassEventFrame | ExecassSummaryRefetchRequiredFrame;
