import { describe, expect, test } from "vitest";

import {
  buildPolicyUpdateAuthorizationBinding,
  policySafeSnapshotCanonicalJson,
  policyUpdateBodyJson,
  sha256Hex,
} from "./policyActions";
import type { PolicyRule, PolicyUpdateRequest } from "./types";

const RULE: PolicyRule = {
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
};

const REQUEST: PolicyUpdateRequest = {
  idempotency_key: "idem-policy-1",
  expected_policy_revision: 7,
  change_summary: "Raise the daily token quota",
  proposed_profile: "balanced",
  proposed_rules: [RULE],
};

/**
 * The gateway digests its own serde serialization of the parsed request, so
 * the client must reproduce the exact Rust struct field order byte for byte.
 */
const EXPECTED_BODY_JSON =
  '{"idempotency_key":"idem-policy-1","expected_policy_revision":7,' +
  '"proposed_profile":"balanced","proposed_rules":[{"rule_id":"rule-1",' +
  '"task_or_delegation_scope":null,"workspace_scope":null,"routine_scope":null,' +
  '"connector_or_tool_identity_and_version_scope":null,"target_scope":null,' +
  '"audience_scope":null,"technical_resource_quotas":[{"kind":"tokens",' +
  '"limit":500000,"reserved":0,"consumed":118000}],"expires_at_ms":null,' +
  '"recovery_limit":3,"parallelism_limit":4,"clarification_sensitivity":"normal",' +
  '"recurring_work_scope":null}],"change_summary":"Raise the daily token quota"}';

/**
 * The gateway's safe snapshot is strict canonical JSON: keys sorted at every
 * level, NFC-normalized strings, integer-only numbers, explicit nulls.
 */
const EXPECTED_SNAPSHOT_JSON =
  '{"configured":true,' +
  '"effective_operational_summary":"Raise the daily token quota",' +
  '"profile":"balanced","rules":[{"audience_scope":null,' +
  '"clarification_sensitivity":"normal",' +
  '"connector_or_tool_identity_and_version_scope":null,"expires_at_ms":null,' +
  '"parallelism_limit":4,"recovery_limit":3,"recurring_work_scope":null,' +
  '"routine_scope":null,"rule_id":"rule-1","target_scope":null,' +
  '"task_or_delegation_scope":null,"technical_resource_quotas":[' +
  '{"consumed":118000,"kind":"tokens","limit":500000,"reserved":0}],' +
  '"workspace_scope":null}]}';

// "e" followed by U+0301 combining acute accent (decomposed form).
const DECOMPOSED_SUMMARY = "Café quota";
// Precomposed U+00E9 (NFC form of the same text).
const COMPOSED_SUMMARY = "Café quota";

describe("policyUpdateBodyJson", () => {
  test("serializes the exact serde struct field order compactly", () => {
    expect(policyUpdateBodyJson(REQUEST)).toBe(EXPECTED_BODY_JSON);
  });

  test("keeps strings byte-exact without unicode normalization", () => {
    const body = policyUpdateBodyJson({
      ...REQUEST,
      change_summary: DECOMPOSED_SUMMARY,
    });
    expect(body.includes(DECOMPOSED_SUMMARY)).toBe(true);
    expect(body.includes(COMPOSED_SUMMARY)).toBe(false);
  });
});

describe("policySafeSnapshotCanonicalJson", () => {
  test("builds the sorted canonical snapshot over configured/profile/rules/summary", () => {
    expect(policySafeSnapshotCanonicalJson(REQUEST)).toBe(EXPECTED_SNAPSHOT_JSON);
  });

  test("NFC-normalizes snapshot strings like the gateway canonicalizer", () => {
    const snapshot = policySafeSnapshotCanonicalJson({
      ...REQUEST,
      change_summary: DECOMPOSED_SUMMARY,
    });
    expect(snapshot).toContain(
      `"effective_operational_summary":"${COMPOSED_SUMMARY}"`,
    );
    expect(snapshot).not.toContain(DECOMPOSED_SUMMARY);
  });

  test("escapes control characters exactly like the canonical writer", () => {
    const snapshot = policySafeSnapshotCanonicalJson({
      ...REQUEST,
      change_summary: 'a\nb\t"q"',
    });
    const summary = String.raw`"effective_operational_summary":"a\nb\t\"q\""`;
    expect(snapshot).toContain(summary);
  });

  test("rejects non-integer numbers instead of silently drifting from the contract", () => {
    expect(() =>
      policySafeSnapshotCanonicalJson({
        ...REQUEST,
        proposed_rules: [
          {
            ...RULE,
            technical_resource_quotas: [
              { kind: "tokens", limit: 0.5, reserved: 0, consumed: 0 },
            ],
          },
        ],
      }),
    ).toThrow(/integer/i);
  });
});

describe("sha256Hex", () => {
  test("matches the well-known SHA-256 vector", async () => {
    await expect(sha256Hex("abc")).resolves.toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });
});

describe("buildPolicyUpdateAuthorizationBinding", () => {
  test("binds the exact operation, route, revision, correlation, and digests", async () => {
    const binding = await buildPolicyUpdateAuthorizationBinding(REQUEST, {
      now: 1_753_500_000_000,
      correlationId: "corr-policy-1",
    });
    expect(binding).toEqual({
      operation: "policy_update",
      method: "PUT",
      path: "/api/v1/execass/policy",
      idempotency_key: "idem-policy-1",
      expected_revision: 7,
      canonical_body_digest: await sha256Hex(EXPECTED_BODY_JSON),
      safe_snapshot_digest: await sha256Hex(EXPECTED_SNAPSHOT_JSON),
      request_correlation_id: "corr-policy-1",
      created_at_ms: 1_753_500_000_000,
    });
  });
});
