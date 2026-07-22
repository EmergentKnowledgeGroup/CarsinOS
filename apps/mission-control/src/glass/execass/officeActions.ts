/**
 * Pure builders for Office actions. Every proof binding is derived from
 * server-supplied challenges - never from client-remembered state - and
 * identifier/time inputs are injected so callers (and tests) control them.
 */

import type {
  DecisionResult,
  DecisionSummary,
  DurableEventEnvelope,
  IntakeRequest,
  LocalDecisionProofBinding,
  RunControlOperation,
  RunControlRequestBinding,
  RunControlResumeSnapshot,
  RunControlTarget,
  StopAllStatusResponse,
} from "./types";

export interface ActionIds {
  idempotencyKey: string;
  correlationId: string;
}

export type DecisionResolutionBuild =
  | {
      ok: true;
      binding: LocalDecisionProofBinding;
      correlationId: string;
      challengeResponse: string | null;
      revisionText: string | null;
    }
  | { ok: false; reason: string };

export function buildDecisionResolution(
  decision: DecisionSummary,
  result: DecisionResult,
  options: { now: number; ids: ActionIds; revisionText?: string },
): DecisionResolutionBuild {
  const serverChallenge = decision.local_owner_proof_challenge;
  if (!serverChallenge) {
    return {
      ok: false,
      reason:
        "This decision has no local proof challenge yet - refresh and try again.",
    };
  }
  if (options.now >= serverChallenge.expires_at_ms) {
    return {
      ok: false,
      reason: "This decision challenge expired - it will be presented again.",
    };
  }
  const binding: LocalDecisionProofBinding = {
    decision_id: serverChallenge.decision_id,
    decision_revision: serverChallenge.decision_revision,
    normalized_intent_digest: serverChallenge.normalized_intent_digest,
    policy_revision: serverChallenge.policy_revision,
    canonical_manifest_digest: serverChallenge.canonical_manifest_digest,
    selected_logical_action_id: serverChallenge.selected_logical_action_id,
    presented_action_digest: serverChallenge.presented_action_digest,
    declared_consequence_digest: serverChallenge.declared_consequence_digest,
    challenge_digest: serverChallenge.challenge_digest,
    expires_at_ms: serverChallenge.expires_at_ms,
    response_selected_logical_action_id:
      serverChallenge.selected_logical_action_id,
    decision_result: result,
    idempotency_key: options.ids.idempotencyKey,
    observed_at_ms: options.now,
    challenge_response_digest: null,
    revision_text_digest: null,
  };
  return {
    ok: true,
    binding,
    correlationId: options.ids.correlationId,
    challengeResponse: decision.challenge?.nonce_or_token ?? null,
    revisionText: options.revisionText ?? null,
  };
}

export function buildIntakeRequest(
  text: string,
  options: {
    ids: { requestId: string; idempotencyKey: string; correlationId: string };
    attachToDelegationId?: string | null;
  },
): IntakeRequest {
  return {
    request_id: options.ids.requestId,
    idempotency_key: options.ids.idempotencyKey,
    text: text.trim(),
    source_correlation_id: options.ids.correlationId,
    attach_to_delegation_id: options.attachToDelegationId ?? null,
  };
}

export function buildRunControlBinding(
  operation: RunControlOperation,
  target: RunControlTarget,
  options: { now: number; ids: ActionIds },
  resume?: RunControlResumeSnapshot,
): RunControlRequestBinding {
  return {
    operation,
    target,
    idempotency_key: options.ids.idempotencyKey,
    request_correlation_id: options.ids.correlationId,
    observed_at_ms: options.now,
    resume: resume ?? null,
  };
}

/** Server-disclosed facts a human resume request must carry back exactly. */
export function buildResumeSnapshotFromStatus(
  status: StopAllStatusResponse,
): RunControlResumeSnapshot {
  return {
    stopped_epoch: status.stop_epoch,
    current_policy_revision: status.current_policy_revision,
    unresolved_effect_disclosure_digest:
      status.unresolved_effect_disclosure_digest,
    current_plan_revision: null,
    delegation_state_revision: null,
  };
}

export interface TrayNote {
  id: string;
  atMs: number;
  text: string;
  deepLink: string | null;
}

/** "While you were out": notes ExecAss leaves on the desk, never a red badge. */
export function trayNoteFromEnvelope(
  envelope: DurableEventEnvelope,
): TrayNote | null {
  if (envelope.event_name !== "execass.v1.notification.scheduled") {
    return null;
  }
  return {
    id: envelope.duplicate_identity,
    atMs: envelope.occurred_at_ms,
    text: envelope.safe_payload.summary,
    deepLink: envelope.safe_payload.authoritative_deep_link ?? null,
  };
}
