import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { getGatewayToken } from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";
import {
  ExecassApiError,
  encodeOwnerProofHeader,
  execassIntake,
  getExecassPolicy,
  getExecassSummary,
  listExecassDelegations,
  resolveExecassDecision,
  stopExecassDelegation,
  updateExecassPolicy,
} from "./api";
import {
  fixtureIntakeDelegationResponse,
  fixtureIntakeProof,
  fixtureIntakeRequest,
  fixtureMutationAuthorization,
  fixturePolicyResponse,
  fixturePolicyUpdateRequest,
  fixtureResolveDecisionRequest,
  fixtureResolveDecisionResponse,
  fixtureRunControlRequest,
  fixtureRunControlResponse,
  fixtureSummaryResponse,
} from "./fixtures";

vi.mock("../../lib/runtime", () => ({
  getGatewayToken: vi.fn(),
}));

const SETTINGS: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
} as RuntimeConnectionSettings;

function okResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.mocked(getGatewayToken).mockResolvedValue("token-123");
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe("encodeOwnerProofHeader", () => {
  test("encodes JSON as base64url without padding", () => {
    const encoded = encodeOwnerProofHeader({ a: "b" });
    expect(encoded).not.toMatch(/[+/=]/);
    const decoded = JSON.parse(
      atob(encoded.replace(/-/g, "+").replace(/_/g, "/")),
    );
    expect(decoded).toEqual({ a: "b" });
  });
});

describe("getExecassSummary", () => {
  test("GETs the summary route with bearer auth and a request id", async () => {
    fetchMock.mockResolvedValue(okResponse(fixtureSummaryResponse()));
    const summary = await getExecassSummary(SETTINGS);
    expect(summary.needs_you.length).toBeGreaterThan(0);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("http://127.0.0.1:18789/api/v1/execass/summary");
    const headers = init.headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer token-123");
    expect(headers["x-request-id"]).toMatch(/[0-9a-f-]{36}/);
    expect(init.method).toBe("GET");
  });

  test("throws a config error when no token is configured", async () => {
    vi.mocked(getGatewayToken).mockResolvedValue(null);
    await expect(getExecassSummary(SETTINGS)).rejects.toMatchObject({
      kind: "config",
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("rejects a malformed success body before it reaches controller state", async () => {
    fetchMock.mockResolvedValue(
      okResponse({ needs_you: [], raw_secret: "must-not-be-trusted" }),
    );
    await expect(getExecassSummary(SETTINGS)).rejects.toMatchObject({
      kind: "http",
      status: 502,
    });
  });
});

describe("execassIntake", () => {
  test("POSTs with matching Idempotency-Key and base64url owner proof header", async () => {
    fetchMock.mockResolvedValue(okResponse(fixtureIntakeDelegationResponse()));
    const request = fixtureIntakeRequest();
    const proof = fixtureIntakeProof();
    const outcome = await execassIntake(SETTINGS, request, proof);
    expect(outcome.kind).toBe("delegation");
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("http://127.0.0.1:18789/api/v1/execass/intake");
    const headers = init.headers as Record<string, string>;
    expect(headers["Idempotency-Key"]).toBe(request.idempotency_key);
    expect(headers["X-ExecAss-Owner-Proof"]).toBe(encodeOwnerProofHeader(proof));
    expect(JSON.parse(String(init.body))).toEqual(request);
  });
});

describe("resolveExecassDecision", () => {
  test("POSTs to the decision route with the body idempotency key", async () => {
    fetchMock.mockResolvedValue(okResponse(fixtureResolveDecisionResponse()));
    const request = fixtureResolveDecisionRequest();
    await resolveExecassDecision(SETTINGS, "dec-1", request);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(
      "http://127.0.0.1:18789/api/v1/execass/decisions/dec-1/resolve",
    );
    const headers = init.headers as Record<string, string>;
    expect(headers["Idempotency-Key"]).toBe(request.idempotency_key);
    expect(headers["X-ExecAss-Owner-Proof"]).toBeUndefined();
  });
});

describe("stopExecassDelegation", () => {
  test("uses the binding idempotency key as the header", async () => {
    fetchMock.mockResolvedValue(okResponse(fixtureRunControlResponse()));
    const request = fixtureRunControlRequest("delegation_stop");
    await stopExecassDelegation(SETTINGS, "dlg-1", request);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(
      "http://127.0.0.1:18789/api/v1/execass/delegations/dlg-1/stop",
    );
    const headers = init.headers as Record<string, string>;
    expect(headers["Idempotency-Key"]).toBe(request.binding.idempotency_key);
  });
});

describe("updateExecassPolicy", () => {
  test("PUTs with the mutation authorization encoded in the proof header", async () => {
    fetchMock.mockResolvedValue(
      okResponse({ policy: fixturePolicyResponse(), updated_at_ms: 1 }),
    );
    const request = fixturePolicyUpdateRequest();
    const authorization = fixtureMutationAuthorization("policy_update");
    await updateExecassPolicy(SETTINGS, request, authorization);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("http://127.0.0.1:18789/api/v1/execass/policy");
    expect(init.method).toBe("PUT");
    const headers = init.headers as Record<string, string>;
    expect(headers["X-ExecAss-Owner-Proof"]).toBe(
      encodeOwnerProofHeader(authorization),
    );
  });
});

describe("listExecassDelegations", () => {
  test("appends only provided query params", async () => {
    fetchMock.mockResolvedValue(okResponse({ items: [], next_cursor: null }));
    await listExecassDelegations(SETTINGS, { phase: "in_motion", limit: 10 });
    const [url] = fetchMock.mock.calls[0] as [string];
    const parsed = new URL(url);
    expect(parsed.pathname).toBe("/api/v1/execass/delegations");
    expect(parsed.searchParams.get("phase")).toBe("in_motion");
    expect(parsed.searchParams.get("limit")).toBe("10");
    expect(parsed.searchParams.has("cursor")).toBe(false);
  });
});

describe("error handling", () => {
  test("maps a contract ApiError body to ExecassApiError with the safe message", async () => {
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({
          code: "execass.v1.revision_conflict",
          safe_human_message: "The work changed before this request could be applied.",
          retryable: true,
          correlation_id: "corr-1",
          safe_for_display: true,
          exposes_sensitive_metadata: false,
        }),
        { status: 409 },
      ),
    );
    let caught: unknown;
    try {
      await getExecassPolicy(SETTINGS);
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(ExecassApiError);
    const apiError = caught as ExecassApiError;
    expect(apiError.code).toBe("execass.v1.revision_conflict");
    expect(apiError.status).toBe(409);
    expect(apiError.retryable).toBe(true);
    expect(apiError.isRevisionOrIdempotencyConflict).toBe(true);
    expect(apiError.safeMessage).toBe(
      "The work changed before this request could be applied.",
    );
  });

  test("falls back to a generic safe message for non-contract error bodies", async () => {
    fetchMock.mockResolvedValue(
      new Response("<html>secret stack trace</html>", { status: 500 }),
    );
    let caught: unknown;
    try {
      await getExecassSummary(SETTINGS);
    } catch (error) {
      caught = error;
    }
    const apiError = caught as ExecassApiError;
    expect(apiError).toBeInstanceOf(ExecassApiError);
    expect(apiError.safeMessage).not.toContain("stack trace");
    expect(apiError.status).toBe(500);
  });

  test("wraps network failures without leaking internals", async () => {
    fetchMock.mockRejectedValue(new TypeError("fetch failed"));
    await expect(getExecassSummary(SETTINGS)).rejects.toMatchObject({
      kind: "network",
    });
  });
});
