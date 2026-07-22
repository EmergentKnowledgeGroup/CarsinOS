/**
 * Deterministic, schema-shaped ExecAss fixtures.
 *
 * Shared by unit tests and the Playwright mock gateway so every consumer
 * exercises the exact wire shapes from contracts/execass/v1/schema.
 */

import type {
  AttentionItem,
  DecisionSummary,
  DelegationRunControlResponse,
  DelegationSummary,
  DurableEventEnvelope,
  ExecassEventName,
  IntakeRequest,
  IntakeResponse,
  LocalDecisionProof,
  LocalDecisionProofBinding,
  LocalOwnerIntakeProof,
  LocalOwnerMutationBinding,
  LocalOwnerMutationProof,
  NextItem,
  OwnerMutationOperation,
  PolicyResponse,
  PolicyUpdateRequest,
  ReceiptSummary,
  ResolveDecisionRequest,
  ResolveDecisionResponse,
  RunControlOperation,
  RunControlRequest,
  StopAllStatusResponse,
  SummaryResponse,
} from "./types";

const T0 = 1_753_200_000_000; // fixed fixture epoch

export function fixtureDecisionSummary(
  overrides: Partial<DecisionSummary> = {},
): DecisionSummary {
  return {
    decision_id: "dec-mailchimp",
    delegation_id: "dlg-mailchimp",
    revision: 3,
    status: "pending",
    kind: "dangerous_action_confirmation",
    assurance_required: "verified_owner_resolution",
    recommendation: "Close it. We're fully off it.",
    why_now: "Migration to the new sender finished and passed verification.",
    consequence:
      "Permanent. All 41,204 contacts exported and verified - closing deletes the account and its history.",
    alternatives: ["confirm_and_continue", "revise", "decline", "stop"],
    exact_manifest_digest: "sha256:manifest-1",
    technical_resources: [
      { kind: "connector_calls", limit: 10, reserved: 1, consumed: 0 },
    ],
    requested_at_ms: T0,
    authoritative_deep_link: "carsinos://delegations/dlg-mailchimp",
    accepted_confirmation_grant: null,
    challenge: {
      decision_revision: 3,
      exact_presented_action_or_alternative: "confirm_and_continue",
      declared_consequence:
        "Permanent. All 41,204 contacts exported and verified - closing deletes the account and its history.",
      nonce_or_token: "nonce-1",
      expires_at_ms: T0 + 15 * 60_000,
    },
    local_owner_proof_challenge: {
      decision_id: "dec-mailchimp",
      decision_revision: 3,
      normalized_intent_digest: "sha256:intent-1",
      policy_revision: 7,
      canonical_manifest_digest: "sha256:manifest-1",
      selected_logical_action_id: "act-close-account",
      presented_action_digest: "sha256:action-1",
      declared_consequence_digest: "sha256:consequence-1",
      challenge_digest: "sha256:challenge-1",
      expires_at_ms: T0 + 15 * 60_000,
    },
    resolved_at_ms: null,
    resolved_owner: null,
    result: null,
    ...overrides,
  };
}

export function fixtureAttentionItem(
  overrides: Partial<AttentionItem> = {},
): AttentionItem {
  return {
    attention_id: "att-1",
    kind: "confirmation",
    subject: {
      scope_kind: "delegation",
      delegation_id: "dlg-mailchimp",
      delegation_revision: 12,
    },
    reason: "Closing the old Mailchimp account is permanent.",
    recommendation: "Close it. We're fully off it.",
    alternatives_or_actions: ["confirm_and_continue", "revise", "decline", "stop"],
    assurance_required: "verified_owner_resolution",
    deadline_reminder_state: "none",
    authoritative_deep_link: "carsinos://delegations/dlg-mailchimp",
    deadline_at_ms: null,
    decision_id: "dec-mailchimp",
    decision_kind: "dangerous_action_confirmation",
    decision_revision: 3,
    ...overrides,
  };
}

export function fixtureDelegationSummary(
  overrides: Partial<DelegationSummary> = {},
): DelegationSummary {
  return {
    delegation_id: "dlg-retreat",
    phase: "in_motion",
    run_control: "running",
    state_revision: 4,
    intent_summary: "Plan the October team retreat",
    outcome_summary: "Drafting the agenda - section 3 of 5",
    policy_revision: 7,
    stop_epoch: 0,
    created_at_ms: T0 - 3 * 3_600_000,
    updated_at_ms: T0 - 120_000,
    authoritative_deep_link: "carsinos://delegations/dlg-retreat",
    acknowledged_at_ms: null,
    pending_decision: null,
    pending_external_wait: null,
    terminal_at_ms: null,
    ...overrides,
  };
}

export function fixtureNextItem(overrides: Partial<NextItem> = {}): NextItem {
  return {
    next_item_id: "next-payroll",
    kind: "routine",
    summary: "Payroll run",
    authoritative_deep_link: "carsinos://jobs/payroll",
    delegation_id: null,
    due_at_ms: null,
    scheduled_for_ms: T0 + 72 * 3_600_000,
    ...overrides,
  };
}

export function fixtureReceiptSummary(
  overrides: Partial<ReceiptSummary> = {},
): ReceiptSummary {
  return {
    receipt_id: "rcpt-8841",
    scope: {
      scope_kind: "delegation",
      delegation_id: "dlg-q3-report",
      delegation_sequence: 9,
    },
    global_sequence: 8841,
    receipt_kind: "completion",
    subject_kind: "delegation",
    subject_id: "dlg-q3-report",
    subject_revision: 9,
    occurred_at_ms: T0 - 3 * 3_600_000,
    committed_at_ms: T0 - 3 * 3_600_000 + 250,
    evidence_refs: [
      {
        authority_kind: "run",
        source_id: "run-411",
        authoritative_revision: 2,
        authority_link_id: "link-1",
        observation_digest: "sha256:obs-1",
        deep_link: "carsinos://runs/run-411",
      },
    ],
    receipt_digest: "sha256:receipt-8841",
    key_id: "key-1",
    key_generation: 1,
    integrity_tag: "hmac:tag-1",
    safe_summary: "Q3 supplier report sent and opened",
    delegation_previous_receipt_digest: null,
    global_previous_receipt_digest: "sha256:receipt-8840",
    previous_key_integrity_tag: null,
    ...overrides,
  };
}

export function fixtureSummaryResponse(
  overrides: Partial<SummaryResponse> = {},
): SummaryResponse {
  return {
    needs_you: [
      fixtureAttentionItem(),
      fixtureAttentionItem({
        attention_id: "att-2",
        kind: "clarification",
        subject: {
          scope_kind: "delegation",
          delegation_id: "dlg-retreat",
          delegation_revision: 4,
        },
        reason: "Two venues fit the budget; the meeting spaces differ.",
        recommendation: "Harbor House",
        alternatives_or_actions: ["Harbor House", "The Pines", "Surprise me"],
        assurance_required: "mechanical_resolution",
        authoritative_deep_link: "carsinos://delegations/dlg-retreat",
        decision_id: "dec-venue",
        decision_kind: "clarification",
        decision_revision: 1,
      }),
    ],
    in_motion: [
      fixtureDelegationSummary(),
      fixtureDelegationSummary({
        delegation_id: "dlg-hartline",
        phase: "waiting_external",
        intent_summary: "Get the Hartline contract signed",
        outcome_summary: "Waiting on their signature - nothing for you to do",
        pending_external_wait: "Awaiting countersignature from Hartline",
        authoritative_deep_link: "carsinos://delegations/dlg-hartline",
      }),
    ],
    done: [
      fixtureDelegationSummary({
        delegation_id: "dlg-q3-report",
        phase: "completed",
        intent_summary: "Send the Q3 supplier report",
        outcome_summary: "Sent at 6:12 and opened by Hartline",
        terminal_at_ms: T0 - 3 * 3_600_000,
        authoritative_deep_link: "carsinos://delegations/dlg-q3-report",
      }),
      fixtureDelegationSummary({
        delegation_id: "dlg-list-cleanup",
        phase: "partially_completed",
        intent_summary: "Clean up the newsletter list",
        outcome_summary: "96% done - 12 bounced addresses still re-verifying",
        terminal_at_ms: T0 - 2 * 3_600_000,
        authoritative_deep_link: "carsinos://delegations/dlg-list-cleanup",
      }),
    ],
    next: [
      fixtureNextItem(),
      fixtureNextItem({
        next_item_id: "next-nadia",
        kind: "commitment",
        summary: "Promised reply to Nadia",
        authoritative_deep_link: "carsinos://mail/nadia",
        scheduled_for_ms: null,
        due_at_ms: T0 + 24 * 3_600_000,
      }),
    ],
    receipts: [fixtureReceiptSummary()],
    displayed: {
      cursor: "cursor-412",
      displayed_at_ms: T0,
      delivered: [{ item_id: "att-1", revision: 3 }],
    },
    ...overrides,
  };
}

export function fixtureIntakeRequest(
  overrides: Partial<IntakeRequest> = {},
): IntakeRequest {
  return {
    request_id: "req-1",
    idempotency_key: "idem-intake-1",
    text: "Chase the two unpaid invoices from June, politely",
    source_correlation_id: "corr-intake-1",
    attach_to_delegation_id: null,
    ...overrides,
  };
}

export function fixtureIntakeProof(
  overrides: Partial<LocalOwnerIntakeProof> = {},
): LocalOwnerIntakeProof {
  return {
    authenticated_client_id: "carsinos-desktop",
    request_correlation_id: "corr-intake-1",
    request_id: "req-1",
    idempotency_key: "idem-intake-1",
    attach_to_delegation_id: null,
    normalized_intent_digest: "sha256:intent-2",
    instruction_digest: "sha256:instruction-1",
    proof_hex: "ab".repeat(32),
    ...overrides,
  };
}

export function fixtureIntakeDelegationResponse(): IntakeResponse {
  return {
    kind: "delegation",
    delegation: fixtureDelegationSummary({
      delegation_id: "dlg-invoices",
      phase: "accepted",
      intent_summary: "Chase the two unpaid invoices from June, politely",
      outcome_summary: "Accepted - planning next",
      authoritative_deep_link: "carsinos://delegations/dlg-invoices",
    }),
    created: true,
  };
}

export function fixtureIntakeConversationalResponse(): IntakeResponse {
  return {
    kind: "conversational",
    response_text: "Both June invoices are already paid - nothing to chase.",
    request_audit_ref: "audit-1",
  };
}

export function fixtureDecisionProofBinding(
  overrides: Partial<LocalDecisionProofBinding> = {},
): LocalDecisionProofBinding {
  return {
    decision_id: "dec-mailchimp",
    decision_revision: 3,
    normalized_intent_digest: "sha256:intent-1",
    policy_revision: 7,
    canonical_manifest_digest: "sha256:manifest-1",
    selected_logical_action_id: "act-close-account",
    presented_action_digest: "sha256:action-1",
    declared_consequence_digest: "sha256:consequence-1",
    challenge_digest: "sha256:challenge-1",
    expires_at_ms: T0 + 15 * 60_000,
    response_selected_logical_action_id: "act-close-account",
    decision_result: "confirm_and_continue",
    idempotency_key: "idem-decision-1",
    observed_at_ms: T0 + 60_000,
    challenge_response_digest: null,
    revision_text_digest: null,
    ...overrides,
  };
}

export function fixtureDecisionProof(
  overrides: Partial<LocalDecisionProof> = {},
): LocalDecisionProof {
  return {
    authenticated_client_id: "carsinos-desktop",
    request_correlation_id: "corr-decision-1",
    proof_hex: "cd".repeat(32),
    ...overrides,
  };
}

export function fixtureResolveDecisionRequest(
  overrides: Partial<ResolveDecisionRequest> = {},
): ResolveDecisionRequest {
  return {
    idempotency_key: "idem-decision-1",
    decision_revision: 3,
    result: "confirm_and_continue",
    local_proof: fixtureDecisionProof(),
    local_proof_binding: fixtureDecisionProofBinding(),
    challenge_response: "nonce-1",
    revision_text: null,
    ...overrides,
  };
}

export function fixtureResolveDecisionResponse(): ResolveDecisionResponse {
  return {
    decision: fixtureDecisionSummary({
      status: "resolved",
      result: "confirm_and_continue",
      resolved_at_ms: T0 + 61_000,
      resolved_owner: {
        ingress: "local_owner_session",
        verified_evidence_ref: "evidence-1",
      },
    }),
    delegation: fixtureDelegationSummary({
      delegation_id: "dlg-mailchimp",
      phase: "in_motion",
      intent_summary: "Close the old Mailchimp account",
      outcome_summary: "Confirmed - closing the account now",
      authoritative_deep_link: "carsinos://delegations/dlg-mailchimp",
    }),
    continuation_id: "cont-1",
  };
}

export function fixtureRunControlRequest(
  operation: RunControlOperation,
): RunControlRequest {
  const target =
    operation === "global_stop" || operation === "global_resume"
      ? ({ kind: "global" } as const)
      : ({ kind: "delegation", delegation_id: "dlg-retreat" } as const);
  return {
    binding: {
      operation,
      target,
      idempotency_key: `idem-${operation}-1`,
      request_correlation_id: `corr-${operation}-1`,
      observed_at_ms: T0 + 30_000,
      resume:
        operation === "global_resume" || operation === "delegation_resume"
          ? {
              stopped_epoch: 2,
              current_policy_revision: 7,
              unresolved_effect_disclosure_digest: "sha256:disclosure-1",
              current_plan_revision: 4,
              delegation_state_revision: 4,
            }
          : null,
    },
    local_proof: {
      authenticated_client_id: "carsinos-desktop",
      request_correlation_id: `corr-${operation}-1`,
      proof_hex: "ef".repeat(32),
    },
  };
}

export function fixtureRunControlResponse(): DelegationRunControlResponse {
  return {
    delegation_id: "dlg-retreat",
    phase: "in_motion",
    run_control: "stop_requested",
    drain_state: "draining",
    state_revision: 5,
    stop_epoch: 1,
    policy_revision: 7,
    unresolved_effect_disclosure_digest: "sha256:disclosure-1",
    unresolved_external_effect_refs: [],
    current_plan_revision: 4,
  };
}

export function fixtureStopAllStatus(
  overrides: Partial<StopAllStatusResponse> = {},
): StopAllStatusResponse {
  return {
    engaged: false,
    drain_state: "disengaged",
    stop_epoch: 0,
    current_policy_revision: 7,
    unresolved_effect_disclosure_digest: "sha256:disclosure-0",
    unresolved_external_effect_refs: [],
    ...overrides,
  };
}

export function fixturePolicyResponse(
  overrides: Partial<PolicyResponse> = {},
): PolicyResponse {
  return {
    policy_id: "policy-1",
    configured: true,
    effective_operational_summary:
      "Balanced autonomy - ordinary work proceeds, dangerous actions get one confirmation.",
    revision: 7,
    rules: [
      {
        rule_id: "rule-1",
        technical_resource_quotas: [
          { kind: "tokens", limit: 500_000, reserved: 0, consumed: 118_000 },
        ],
        workspace_scope: null,
        target_scope: null,
        audience_scope: null,
        clarification_sensitivity: "normal",
        connector_or_tool_identity_and_version_scope: null,
        expires_at_ms: null,
        parallelism_limit: 4,
        recovery_limit: 3,
        recurring_work_scope: null,
        routine_scope: null,
        task_or_delegation_scope: null,
      },
    ],
    updated_at_ms: T0 - 24 * 3_600_000,
    profile: "balanced",
    ...overrides,
  };
}

export function fixturePolicyUpdateRequest(): PolicyUpdateRequest {
  return {
    idempotency_key: "idem-policy-1",
    expected_policy_revision: 7,
    change_summary: "Raise the daily token quota",
    proposed_profile: "balanced",
    proposed_rules: fixturePolicyResponse().rules,
  };
}

export function fixtureMutationAuthorization(
  operation: OwnerMutationOperation,
): { binding: LocalOwnerMutationBinding; proof: LocalOwnerMutationProof } {
  return {
    binding: {
      operation,
      method: operation === "policy_update" ? "PUT" : "PUT",
      path:
        operation === "policy_update"
          ? "/api/v1/execass/policy"
          : "/api/v1/execass/runtime-host",
      idempotency_key: "idem-policy-1",
      expected_revision: 7,
      canonical_body_digest: "sha256:body-1",
      safe_snapshot_digest: "sha256:snapshot-1",
      request_correlation_id: "corr-mutation-1",
      created_at_ms: T0 + 10_000,
    },
    proof: {
      authenticated_client_id: "carsinos-desktop",
      request_correlation_id: "corr-mutation-1",
      proof_hex: "12".repeat(32),
    },
  };
}

export function fixtureEventEnvelope(
  overrides: Partial<DurableEventEnvelope> = {},
): DurableEventEnvelope {
  return {
    event_name: "execass.v1.summary.changed" as ExecassEventName,
    aggregate_id: "summary",
    revision: 412,
    correlation_id: "corr-evt-1",
    causation_id: "cause-evt-1",
    occurred_at_ms: T0 + 5_000,
    schema_version: "v1",
    safe_payload: {
      summary: "Summary changed",
      authoritative_deep_link: null,
      decision_id: null,
      delegation_id: null,
      receipt_ref: null,
    },
    global_sequence: 1001,
    duplicate_identity: "dup-1001",
    ...overrides,
  };
}
