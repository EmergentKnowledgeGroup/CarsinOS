/**
 * Pure builders for the owner policy mutation.
 *
 * The gateway recomputes both proof digests from its own parsed request, so
 * these serializers must reproduce serde's struct field order (body digest)
 * and the strict canonical receipt JSON - sorted keys, NFC strings, integer
 * numbers, explicit nulls - for the safe-snapshot digest, byte for byte.
 */

import type {
  LocalOwnerMutationBinding,
  PolicyRule,
  PolicyUpdateRequest,
  TechnicalResourceQuota,
} from "./types";

const POLICY_PATH = "/api/v1/execass/policy";

type OrderedJson =
  | string
  | number
  | boolean
  | null
  | OrderedJson[]
  | { [key: string]: OrderedJson };

function orderedQuota(quota: TechnicalResourceQuota): OrderedJson {
  return {
    kind: quota.kind,
    limit: quota.limit,
    reserved: quota.reserved,
    consumed: quota.consumed,
  };
}

function orderedRule(rule: PolicyRule): OrderedJson {
  return {
    rule_id: rule.rule_id,
    task_or_delegation_scope: rule.task_or_delegation_scope ?? null,
    workspace_scope: rule.workspace_scope ?? null,
    routine_scope: rule.routine_scope ?? null,
    connector_or_tool_identity_and_version_scope:
      rule.connector_or_tool_identity_and_version_scope ?? null,
    target_scope: rule.target_scope ?? null,
    audience_scope: rule.audience_scope ?? null,
    technical_resource_quotas: rule.technical_resource_quotas.map(orderedQuota),
    expires_at_ms: rule.expires_at_ms ?? null,
    recovery_limit: rule.recovery_limit ?? null,
    parallelism_limit: rule.parallelism_limit ?? null,
    clarification_sensitivity: rule.clarification_sensitivity ?? null,
    recurring_work_scope: rule.recurring_work_scope ?? null,
  };
}

/** The exact bytes the gateway digests after re-serializing the parsed body. */
export function policyUpdateBodyJson(request: PolicyUpdateRequest): string {
  return JSON.stringify({
    idempotency_key: request.idempotency_key,
    expected_policy_revision: request.expected_policy_revision,
    proposed_profile: request.proposed_profile,
    proposed_rules: request.proposed_rules.map(orderedRule),
    change_summary: request.change_summary,
  });
}

function writeCanonicalString(value: string, out: string[]): void {
  out.push('"');
  for (const character of value.normalize("NFC")) {
    const code = character.codePointAt(0) ?? 0;
    if (character === '"') {
      out.push('\\"');
    } else if (character === "\\") {
      out.push("\\\\");
    } else if (code === 0x08) {
      out.push("\\b");
    } else if (code === 0x0c) {
      out.push("\\f");
    } else if (character === "\n") {
      out.push("\\n");
    } else if (character === "\r") {
      out.push("\\r");
    } else if (character === "\t") {
      out.push("\\t");
    } else if (code <= 0x1f) {
      out.push(`\\u${code.toString(16).padStart(4, "0")}`);
    } else {
      out.push(character);
    }
  }
  out.push('"');
}

const UTF8 = new TextEncoder();

/** Rust BTreeMap<String> orders keys by UTF-8 bytes, not UTF-16 code units. */
function compareUtf8(a: string, b: string): number {
  const left = UTF8.encode(a);
  const right = UTF8.encode(b);
  const shared = Math.min(left.length, right.length);
  for (let index = 0; index < shared; index += 1) {
    if (left[index] !== right[index]) {
      return left[index] - right[index];
    }
  }
  return left.length - right.length;
}

function writeCanonicalValue(value: OrderedJson, out: string[]): void {
  if (value === null) {
    out.push("null");
    return;
  }
  if (typeof value === "boolean") {
    out.push(value ? "true" : "false");
    return;
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) {
      throw new Error("canonical policy JSON requires safe integer numbers");
    }
    out.push(String(value));
    return;
  }
  if (typeof value === "string") {
    writeCanonicalString(value, out);
    return;
  }
  if (Array.isArray(value)) {
    out.push("[");
    value.forEach((entry, index) => {
      if (index !== 0) {
        out.push(",");
      }
      writeCanonicalValue(entry, out);
    });
    out.push("]");
    return;
  }
  const keys = Object.keys(value)
    .map((key) => key.normalize("NFC"))
    .sort(compareUtf8);
  out.push("{");
  keys.forEach((key, index) => {
    if (index !== 0) {
      out.push(",");
    }
    writeCanonicalString(key, out);
    out.push(":");
    writeCanonicalValue(value[key], out);
  });
  out.push("}");
}

/**
 * The canonical safe snapshot the gateway digests: the proposed profile and
 * complete rules plus the change summary as the effective summary, exactly as
 * `put_policy` constructs it before redaction/canonicalization.
 */
export function policySafeSnapshotCanonicalJson(
  request: PolicyUpdateRequest,
): string {
  const out: string[] = [];
  writeCanonicalValue(
    {
      configured: true,
      profile: request.proposed_profile,
      rules: request.proposed_rules.map(orderedRule),
      effective_operational_summary: request.change_summary,
    },
    out,
  );
  return out.join("");
}

export async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", UTF8.encode(text));
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

/**
 * The complete owner-proof binding for one policy PUT. The correlation ID
 * must also be sent as the request's x-request-id header - the gateway
 * rebuilds the expected binding from that header and rejects any mismatch.
 */
export async function buildPolicyUpdateAuthorizationBinding(
  request: PolicyUpdateRequest,
  options: { now: number; correlationId: string },
): Promise<LocalOwnerMutationBinding> {
  return {
    operation: "policy_update",
    method: "PUT",
    path: POLICY_PATH,
    idempotency_key: request.idempotency_key,
    expected_revision: request.expected_policy_revision,
    canonical_body_digest: await sha256Hex(policyUpdateBodyJson(request)),
    safe_snapshot_digest: await sha256Hex(policySafeSnapshotCanonicalJson(request)),
    request_correlation_id: options.correlationId,
    created_at_ms: options.now,
  };
}
