/**
 * Typed HTTP adapter for the ExecAss v1.1 contract.
 *
 * Every mutation sends Idempotency-Key equal to the request body's
 * idempotency key (derived here so callers cannot mismatch them).
 * Owner-protected mutations carry X-ExecAss-Owner-Proof: base64url of the
 * canonical proof JSON (intake) or {binding, proof} (policy/runtime-host).
 * Errors surface only contract safe_human_message text - never raw bodies.
 */

import { getGatewayToken } from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";
import type {
  ApiError,
  ApiErrorCode,
  DelegationDetailResponse,
  DelegationListQuery,
  DelegationListResponse,
  DelegationReceiptsResponse,
  DelegationRunControlResponse,
  IntakeRequest,
  IntakeResponse,
  LocalOwnerIntakeProof,
  LocalOwnerMutationBinding,
  LocalOwnerMutationProof,
  PolicyResponse,
  PolicyUpdateRequest,
  PolicyUpdateResponse,
  ResolveDecisionRequest,
  ResolveDecisionResponse,
  ResumeAllResponse,
  RunControlRequest,
  RuntimeHostConfigRequest,
  RuntimeHostConfigResponse,
  RuntimeHostStatusResponse,
  StopAllStatusResponse,
  SummaryAckRequest,
  SummaryAckResponse,
  SummaryResponse,
} from "./types";

const API_PREFIX = "/api/v1/execass";
const REQUEST_TIMEOUT_MS = 15_000;
const GENERIC_SAFE_MESSAGE = "The gateway could not complete this request.";

export type ExecassApiErrorKind = "http" | "network" | "timeout" | "config";

export class ExecassApiError extends Error {
  readonly kind: ExecassApiErrorKind;
  readonly path: string;
  readonly status?: number;
  readonly apiError?: ApiError;

  constructor(options: {
    kind: ExecassApiErrorKind;
    path: string;
    status?: number;
    apiError?: ApiError;
    message?: string;
    cause?: unknown;
  }) {
    super(options.message ?? options.apiError?.safe_human_message ?? GENERIC_SAFE_MESSAGE);
    this.name = "ExecassApiError";
    this.kind = options.kind;
    this.path = options.path;
    this.status = options.status;
    this.apiError = options.apiError;
    if (options.cause !== undefined) {
      (this as { cause?: unknown }).cause = options.cause;
    }
  }

  get code(): ApiErrorCode | undefined {
    return this.apiError?.code;
  }

  get retryable(): boolean {
    return this.apiError?.retryable ?? (this.kind === "network" || this.kind === "timeout");
  }

  /** True for 409-class conflicts the UI should resolve by refetch-and-reconcile. */
  get isRevisionOrIdempotencyConflict(): boolean {
    return (
      this.code === "execass.v1.revision_conflict" ||
      this.code === "execass.v1.idempotency_conflict"
    );
  }

  /** The only error text safe to show a user. */
  get safeMessage(): string {
    if (this.apiError?.safe_for_display) {
      return this.apiError.safe_human_message;
    }
    return GENERIC_SAFE_MESSAGE;
  }
}

export function encodeOwnerProofHeader(value: unknown): string {
  const bytes = new TextEncoder().encode(JSON.stringify(value));
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function resolveUrl(settings: RuntimeConnectionSettings, path: string): string {
  const trimmed = settings.gateway_url.trim();
  const withScheme =
    trimmed.startsWith("http://") || trimmed.startsWith("https://")
      ? trimmed
      : `http://${trimmed}`;
  const base = new URL(withScheme).origin;
  return `${base}${path}`;
}

function isApiError(value: unknown): value is ApiError {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<ApiError>;
  return (
    typeof candidate.code === "string" &&
    typeof candidate.safe_human_message === "string" &&
    typeof candidate.retryable === "boolean" &&
    typeof candidate.correlation_id === "string" &&
    typeof candidate.safe_for_display === "boolean" &&
    typeof candidate.exposes_sensitive_metadata === "boolean"
  );
}

interface ExecassRequestOptions {
  method?: "GET" | "POST" | "PUT";
  body?: unknown;
  /** Set as the Idempotency-Key header; must equal the body's key. */
  idempotencyKey?: string;
  /** Pre-encoded owner-proof header value. */
  ownerProof?: string;
}

async function execassRequest<T>(
  settings: RuntimeConnectionSettings,
  path: string,
  options: ExecassRequestOptions = {},
): Promise<T> {
  const token = await getGatewayToken();
  if (!token) {
    throw new ExecassApiError({
      kind: "config",
      path,
      message: "Gateway token is not configured.",
    });
  }

  const headers: Record<string, string> = {
    Authorization: `Bearer ${token}`,
    "x-request-id":
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `req-${Math.random().toString(16).slice(2)}`,
  };
  if (options.body !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  if (options.idempotencyKey !== undefined) {
    headers["Idempotency-Key"] = options.idempotencyKey;
  }
  if (options.ownerProof !== undefined) {
    headers["X-ExecAss-Owner-Proof"] = options.ownerProof;
  }

  const controller = new AbortController();
  const timeoutId = globalThis.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  let response: Response;
  try {
    response = await fetch(resolveUrl(settings, path), {
      method: options.method ?? "GET",
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: controller.signal,
    });
  } catch (error: unknown) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new ExecassApiError({
        kind: "timeout",
        path,
        message: `Gateway request timed out after ${REQUEST_TIMEOUT_MS}ms.`,
        cause: error,
      });
    }
    throw new ExecassApiError({
      kind: "network",
      path,
      message: "The gateway could not be reached.",
      cause: error,
    });
  } finally {
    globalThis.clearTimeout(timeoutId);
  }

  if (!response.ok) {
    let apiError: ApiError | undefined;
    try {
      const parsed: unknown = await response.json();
      if (isApiError(parsed)) apiError = parsed;
    } catch {
      apiError = undefined;
    }
    throw new ExecassApiError({
      kind: "http",
      path,
      status: response.status,
      apiError,
    });
  }

  return (await response.json()) as T;
}

// ————————————————————————————————————————————————— operations (16)

export async function execassIntake(
  settings: RuntimeConnectionSettings,
  request: IntakeRequest,
  proof: LocalOwnerIntakeProof,
): Promise<IntakeResponse> {
  return execassRequest(settings, `${API_PREFIX}/intake`, {
    method: "POST",
    body: request,
    idempotencyKey: request.idempotency_key,
    ownerProof: encodeOwnerProofHeader(proof),
  });
}

export async function getExecassSummary(
  settings: RuntimeConnectionSettings,
): Promise<SummaryResponse> {
  return execassRequest(settings, `${API_PREFIX}/summary`);
}

export async function acknowledgeExecassSummary(
  settings: RuntimeConnectionSettings,
  request: SummaryAckRequest,
): Promise<SummaryAckResponse> {
  return execassRequest(settings, `${API_PREFIX}/summary/ack`, {
    method: "POST",
    body: request,
    idempotencyKey: request.idempotency_key,
  });
}

export async function listExecassDelegations(
  settings: RuntimeConnectionSettings,
  query: DelegationListQuery = {},
): Promise<DelegationListResponse> {
  const params = new URLSearchParams();
  if (query.cursor) params.set("cursor", query.cursor);
  if (query.limit !== undefined && query.limit !== null) {
    params.set("limit", String(query.limit));
  }
  if (query.phase) params.set("phase", query.phase);
  if (query.run_control) params.set("run_control", query.run_control);
  const suffix = params.toString() ? `?${params.toString()}` : "";
  return execassRequest(settings, `${API_PREFIX}/delegations${suffix}`);
}

export async function getExecassDelegation(
  settings: RuntimeConnectionSettings,
  delegationId: string,
): Promise<DelegationDetailResponse> {
  return execassRequest(
    settings,
    `${API_PREFIX}/delegations/${encodeURIComponent(delegationId)}`,
  );
}

export async function listExecassDelegationReceipts(
  settings: RuntimeConnectionSettings,
  delegationId: string,
): Promise<DelegationReceiptsResponse> {
  return execassRequest(
    settings,
    `${API_PREFIX}/delegations/${encodeURIComponent(delegationId)}/receipts`,
  );
}

export async function resolveExecassDecision(
  settings: RuntimeConnectionSettings,
  decisionId: string,
  request: ResolveDecisionRequest,
): Promise<ResolveDecisionResponse> {
  return execassRequest(
    settings,
    `${API_PREFIX}/decisions/${encodeURIComponent(decisionId)}/resolve`,
    {
      method: "POST",
      body: request,
      idempotencyKey: request.idempotency_key,
    },
  );
}

export async function stopExecassDelegation(
  settings: RuntimeConnectionSettings,
  delegationId: string,
  request: RunControlRequest,
): Promise<DelegationRunControlResponse> {
  return execassRequest(
    settings,
    `${API_PREFIX}/delegations/${encodeURIComponent(delegationId)}/stop`,
    {
      method: "POST",
      body: request,
      idempotencyKey: request.binding.idempotency_key,
    },
  );
}

export async function resumeExecassDelegation(
  settings: RuntimeConnectionSettings,
  delegationId: string,
  request: RunControlRequest,
): Promise<DelegationRunControlResponse> {
  return execassRequest(
    settings,
    `${API_PREFIX}/delegations/${encodeURIComponent(delegationId)}/resume`,
    {
      method: "POST",
      body: request,
      idempotencyKey: request.binding.idempotency_key,
    },
  );
}

export async function getExecassStopAllStatus(
  settings: RuntimeConnectionSettings,
): Promise<StopAllStatusResponse> {
  return execassRequest(settings, `${API_PREFIX}/stop-all`);
}

export async function engageExecassStopAll(
  settings: RuntimeConnectionSettings,
  request: RunControlRequest,
): Promise<StopAllStatusResponse> {
  return execassRequest(settings, `${API_PREFIX}/stop-all`, {
    method: "POST",
    body: request,
    idempotencyKey: request.binding.idempotency_key,
  });
}

export async function resumeExecassAll(
  settings: RuntimeConnectionSettings,
  request: RunControlRequest,
): Promise<ResumeAllResponse> {
  return execassRequest(settings, `${API_PREFIX}/resume-all`, {
    method: "POST",
    body: request,
    idempotencyKey: request.binding.idempotency_key,
  });
}

export async function getExecassPolicy(
  settings: RuntimeConnectionSettings,
): Promise<PolicyResponse> {
  return execassRequest(settings, `${API_PREFIX}/policy`);
}

export interface OwnerMutationAuthorization {
  binding: LocalOwnerMutationBinding;
  proof: LocalOwnerMutationProof;
}

export async function updateExecassPolicy(
  settings: RuntimeConnectionSettings,
  request: PolicyUpdateRequest,
  authorization: OwnerMutationAuthorization,
): Promise<PolicyUpdateResponse> {
  return execassRequest(settings, `${API_PREFIX}/policy`, {
    method: "PUT",
    body: request,
    idempotencyKey: request.idempotency_key,
    ownerProof: encodeOwnerProofHeader(authorization),
  });
}

export async function getExecassRuntimeHost(
  settings: RuntimeConnectionSettings,
): Promise<RuntimeHostStatusResponse> {
  return execassRequest(settings, `${API_PREFIX}/runtime-host`);
}

export async function configureExecassRuntimeHost(
  settings: RuntimeConnectionSettings,
  request: RuntimeHostConfigRequest,
  authorization: OwnerMutationAuthorization,
): Promise<RuntimeHostConfigResponse> {
  return execassRequest(settings, `${API_PREFIX}/runtime-host`, {
    method: "PUT",
    body: request,
    idempotencyKey: request.idempotency_key,
    ownerProof: encodeOwnerProofHeader(authorization),
  });
}
