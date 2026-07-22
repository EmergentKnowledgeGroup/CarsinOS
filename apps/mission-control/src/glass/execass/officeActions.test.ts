import { describe, expect, test } from "vitest";

import {
  buildDecisionResolution,
  buildIntakeRequest,
  buildRunControlBinding,
  buildResumeSnapshotFromStatus,
  trayNoteFromEnvelope,
} from "./officeActions";
import {
  fixtureDecisionSummary,
  fixtureEventEnvelope,
  fixtureStopAllStatus,
} from "./fixtures";

const IDS = { idempotencyKey: "idem-t", correlationId: "corr-t" };
const NOW = 1_753_200_100_000;

describe("buildDecisionResolution", () => {
  test("derives every binding field from the server proof challenge, never client state", () => {
    const decision = fixtureDecisionSummary();
    const built = buildDecisionResolution(decision, "confirm_and_continue", {
      now: NOW,
      ids: IDS,
    });
    expect(built.ok).toBe(true);
    if (!built.ok) return;
    const { binding, challengeResponse } = built;
    const serverChallenge = decision.local_owner_proof_challenge!;
    expect(binding.decision_id).toBe(serverChallenge.decision_id);
    expect(binding.decision_revision).toBe(serverChallenge.decision_revision);
    expect(binding.normalized_intent_digest).toBe(
      serverChallenge.normalized_intent_digest,
    );
    expect(binding.policy_revision).toBe(serverChallenge.policy_revision);
    expect(binding.canonical_manifest_digest).toBe(
      serverChallenge.canonical_manifest_digest,
    );
    expect(binding.selected_logical_action_id).toBe(
      serverChallenge.selected_logical_action_id,
    );
    expect(binding.presented_action_digest).toBe(
      serverChallenge.presented_action_digest,
    );
    expect(binding.declared_consequence_digest).toBe(
      serverChallenge.declared_consequence_digest,
    );
    expect(binding.challenge_digest).toBe(serverChallenge.challenge_digest);
    expect(binding.expires_at_ms).toBe(serverChallenge.expires_at_ms);
    expect(binding.response_selected_logical_action_id).toBe(
      serverChallenge.selected_logical_action_id,
    );
    expect(binding.decision_result).toBe("confirm_and_continue");
    expect(binding.idempotency_key).toBe("idem-t");
    expect(binding.observed_at_ms).toBe(NOW);
    expect(challengeResponse).toBe(decision.challenge!.nonce_or_token);
  });

  test("carries revision text for a revise result", () => {
    const built = buildDecisionResolution(fixtureDecisionSummary(), "revise", {
      now: NOW,
      ids: IDS,
      revisionText: "Only close it after exporting the templates too",
    });
    expect(built.ok).toBe(true);
    if (!built.ok) return;
    expect(built.revisionText).toBe(
      "Only close it after exporting the templates too",
    );
  });

  test("refuses when the server challenge is missing instead of inventing one", () => {
    const decision = fixtureDecisionSummary({
      local_owner_proof_challenge: null,
    });
    const built = buildDecisionResolution(decision, "confirm_and_continue", {
      now: NOW,
      ids: IDS,
    });
    expect(built.ok).toBe(false);
    if (built.ok) return;
    expect(built.reason).toMatch(/challenge/i);
  });

  test("refuses an expired challenge so the UI re-presents instead of failing server-side", () => {
    const decision = fixtureDecisionSummary();
    const built = buildDecisionResolution(decision, "confirm_and_continue", {
      now: decision.local_owner_proof_challenge!.expires_at_ms + 1,
      ids: IDS,
    });
    expect(built.ok).toBe(false);
  });
});

describe("buildIntakeRequest", () => {
  test("trims the text and stamps the provided identifiers", () => {
    const request = buildIntakeRequest("  Chase the invoices  ", {
      ids: {
        requestId: "req-9",
        idempotencyKey: "idem-9",
        correlationId: "corr-9",
      },
    });
    expect(request).toEqual({
      request_id: "req-9",
      idempotency_key: "idem-9",
      text: "Chase the invoices",
      source_correlation_id: "corr-9",
      attach_to_delegation_id: null,
    });
  });
});

describe("buildRunControlBinding", () => {
  test("builds a global stop binding without a resume snapshot", () => {
    const binding = buildRunControlBinding("global_stop", { kind: "global" }, {
      now: NOW,
      ids: IDS,
    });
    expect(binding.operation).toBe("global_stop");
    expect(binding.target).toEqual({ kind: "global" });
    expect(binding.resume).toBeNull();
    expect(binding.observed_at_ms).toBe(NOW);
  });

  test("a resume operation requires the server-disclosed snapshot", () => {
    const snapshot = buildResumeSnapshotFromStatus(
      fixtureStopAllStatus({ engaged: true, stop_epoch: 3 }),
    );
    const binding = buildRunControlBinding(
      "global_resume",
      { kind: "global" },
      { now: NOW, ids: IDS },
      snapshot,
    );
    expect(binding.resume).toEqual({
      stopped_epoch: 3,
      current_policy_revision: 7,
      unresolved_effect_disclosure_digest: "sha256:disclosure-0",
      current_plan_revision: null,
      delegation_state_revision: null,
    });
  });
});

describe("trayNoteFromEnvelope", () => {
  test("turns a scheduled notification into a desk note", () => {
    const envelope = fixtureEventEnvelope({
      event_name: "execass.v1.notification.scheduled",
      safe_payload: {
        summary: "Reminder: venue decision expires tonight",
        delegation_id: "dlg-retreat",
        decision_id: null,
        receipt_ref: null,
        authoritative_deep_link: "carsinos://delegations/dlg-retreat",
      },
    });
    const note = trayNoteFromEnvelope(envelope);
    expect(note).toEqual({
      id: "dup-1001",
      atMs: envelope.occurred_at_ms,
      text: "Reminder: venue decision expires tonight",
      deepLink: "carsinos://delegations/dlg-retreat",
    });
  });

  test("returns null for non-notification events", () => {
    expect(trayNoteFromEnvelope(fixtureEventEnvelope())).toBeNull();
  });
});
