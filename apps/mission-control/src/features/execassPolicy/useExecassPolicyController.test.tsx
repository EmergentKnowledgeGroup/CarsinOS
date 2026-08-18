// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../glass/execass/api", async () => {
  const actual = await vi.importActual<
    typeof import("../../glass/execass/api")
  >("../../glass/execass/api");
  return {
    ...actual,
    getExecassPolicy: vi.fn(),
    updateExecassPolicy: vi.fn(),
  };
});

vi.mock("../../lib/runtime", () => ({
  signExecassLocalOwnerMutation: vi.fn(),
  isTauriRuntime: () => true,
}));

import {
  ExecassApiError,
  getExecassPolicy,
  updateExecassPolicy,
} from "../../glass/execass/api";
import { fixturePolicyResponse } from "../../glass/execass/fixtures";
import {
  policyUpdateBodyJson,
  policySafeSnapshotCanonicalJson,
  sha256Hex,
} from "../../glass/execass/policyActions";
import type { PolicyResponse } from "../../glass/execass/types";
import { signExecassLocalOwnerMutation } from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";
import {
  useExecassPolicyController,
  type ExecassPolicyController,
} from "./useExecassPolicyController";

const getPolicyMock = vi.mocked(getExecassPolicy);
const updatePolicyMock = vi.mocked(updateExecassPolicy);
const signMock = vi.mocked(signExecassLocalOwnerMutation);

const SETTINGS: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

const PROOF = {
  authenticated_client_id: "carsinos-desktop",
  request_correlation_id: "corr",
  proof_hex: "ab".repeat(32),
};

function conflictError(): ExecassApiError {
  return new ExecassApiError({
    kind: "http",
    path: "/api/v1/execass/policy",
    status: 409,
    apiError: {
      code: "execass.v1.revision_conflict",
      safe_human_message: "The policy revision changed before this update.",
      retryable: false,
      correlation_id: "corr-conflict",
      safe_for_display: true,
      exposes_sensitive_metadata: false,
    },
  });
}

/** Drain the digest/sign/update async chain when the promise is held open. */
async function flushAsync(turns = 6) {
  await act(async () => {
    for (let turn = 0; turn < turns; turn += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

let container: HTMLDivElement;
let root: Root | null = null;
let controller: ExecassPolicyController | null = null;
let notices: Array<{ tone: string; message: string } | null> = [];

interface HarnessProps {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  authIdentityGeneration: number;
  active: boolean;
  policyInvalidationGeneration: number;
}

function Harness(props: HarnessProps) {
  const current = useExecassPolicyController({
    settings: props.settings,
    tokenConfigured: props.tokenConfigured,
    authIdentityGeneration: props.authIdentityGeneration,
    active: props.active,
    policyInvalidationGeneration: props.policyInvalidationGeneration,
    setNotice: (notice) => notices.push(notice),
  });
  useEffect(() => {
    controller = current;
  });
  return null;
}

async function mount(overrides: Partial<HarnessProps> = {}) {
  const props: HarnessProps = {
    settings: SETTINGS,
    tokenConfigured: true,
    authIdentityGeneration: 0,
    active: true,
    policyInvalidationGeneration: 0,
    ...overrides,
  };
  await act(async () => {
    root ??= createRoot(container);
    root.render(<Harness {...props} />);
  });
  return props;
}

async function rerender(props: HarnessProps) {
  await act(async () => {
    root!.render(<Harness {...props} />);
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  notices = [];
  controller = null;
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  getPolicyMock.mockResolvedValue(fixturePolicyResponse());
  signMock.mockResolvedValue(PROOF);
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
});

describe("useExecassPolicyController reads", () => {
  it("stays never-loaded and fetches nothing until the identity is configured", async () => {
    await mount({ tokenConfigured: false });
    expect(getPolicyMock).not.toHaveBeenCalled();
    expect(controller?.phase).toBe("never-loaded");
    expect(controller?.policy).toBeNull();
  });

  it("loads the authoritative policy once active with a configured identity", async () => {
    await mount();
    expect(getPolicyMock).toHaveBeenCalledTimes(1);
    expect(controller?.phase).toBe("loaded");
    expect(controller?.policy?.revision).toBe(7);
    expect(controller?.error).toBeNull();
  });

  it("shows honest error truth when the read fails", async () => {
    getPolicyMock.mockRejectedValueOnce(
      new ExecassApiError({ kind: "network", path: "/api/v1/execass/policy" }),
    );
    await mount();
    expect(controller?.phase).toBe("error");
    expect(controller?.policy).toBeNull();
    expect(controller?.error).toMatch(/gateway/i);
  });

  it("invalidates old facts on identity change and rejects the late stale read", async () => {
    const stale = deferred<PolicyResponse>();
    getPolicyMock.mockReturnValueOnce(stale.promise);
    const props = await mount();
    expect(controller?.phase).toBe("loading");

    const fresh = deferred<PolicyResponse>();
    getPolicyMock.mockReturnValueOnce(fresh.promise);
    await rerender({
      ...props,
      settings: { gateway_url: "http://10.0.0.9:18789" },
    });
    // The transition render must not display old-identity facts.
    expect(controller?.policy).toBeNull();
    expect(controller?.phase).toBe("loading");

    await act(async () => {
      stale.resolve(fixturePolicyResponse({ revision: 99 }));
    });
    expect(controller?.policy).toBeNull();

    await act(async () => {
      fresh.resolve(fixturePolicyResponse({ revision: 7 }));
    });
    expect(controller?.phase).toBe("loaded");
    expect(controller?.policy?.revision).toBe(7);
  });

  it("treats same-gateway secure-token replacement as an identity change", async () => {
    const stale = deferred<PolicyResponse>();
    getPolicyMock.mockReturnValueOnce(stale.promise);
    const props = await mount();
    expect(controller?.phase).toBe("loading");

    const fresh = deferred<PolicyResponse>();
    getPolicyMock.mockReturnValueOnce(fresh.promise);
    await rerender({ ...props, authIdentityGeneration: 1 });
    expect(controller?.policy).toBeNull();
    expect(controller?.phase).toBe("loading");

    await act(async () => {
      stale.resolve(fixturePolicyResponse({ revision: 99 }));
    });
    expect(controller?.policy).toBeNull();

    await act(async () => {
      fresh.resolve(fixturePolicyResponse({ revision: 8 }));
    });
    expect(controller?.policy?.revision).toBe(8);
  });

  it("refetches on the one stream invalidation generation and keeps the draft", async () => {
    const props = await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftChangeSummary("Owner draft in progress");
    });
    getPolicyMock.mockResolvedValueOnce(fixturePolicyResponse({ revision: 8 }));
    await rerender({ ...props, policyInvalidationGeneration: 1 });
    expect(getPolicyMock).toHaveBeenCalledTimes(2);
    expect(controller?.policy?.revision).toBe(8);
    expect(controller?.draft).not.toBeNull();
    expect(controller?.draft?.changeSummary).toBe("Owner draft in progress");
    expect(controller?.conflict).toBe(true);
  });
});

describe("useExecassPolicyController draft and apply", () => {
  it("seeds the draft from authoritative truth preserving every rule field", async () => {
    await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftProfile("full_send");
    });
    expect(controller?.draft?.profile).toBe("full_send");
    expect(controller?.draft?.rules).toEqual(fixturePolicyResponse().rules);
    expect(controller?.conflict).toBe(false);
  });

  it("applies once for same-tick duplicate submissions", async () => {
    const pending = deferred<{
      policy: PolicyResponse;
      updated_at_ms: number;
    }>();
    updatePolicyMock.mockReturnValue(pending.promise);
    await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftChangeSummary("Raise the token limit");
    });
    await act(async () => {
      void controller!.applyDraft();
      void controller!.applyDraft();
    });
    await flushAsync();
    expect(signMock).toHaveBeenCalledTimes(1);
    expect(updatePolicyMock).toHaveBeenCalledTimes(1);
    await act(async () => {
      pending.resolve({
        policy: fixturePolicyResponse({ revision: 8 }),
        updated_at_ms: 2,
      });
    });
    await flushAsync();
  });

  it("PUTs the complete ruleset with the authoritative revision and exact proof binding", async () => {
    const pending = deferred<{
      policy: PolicyResponse;
      updated_at_ms: number;
    }>();
    updatePolicyMock.mockReturnValue(pending.promise);
    await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftProfile("full_send");
      controller!.setDraftChangeSummary("Full send from the owner");
    });
    await act(async () => {
      void controller!.applyDraft();
    });
    await flushAsync();

    const [, request, authorization] = updatePolicyMock.mock.calls[0]!;
    expect(request.expected_policy_revision).toBe(7);
    expect(request.proposed_profile).toBe("full_send");
    expect(request.proposed_rules).toEqual(fixturePolicyResponse().rules);
    expect(request.change_summary).toBe("Full send from the owner");
    expect(request.idempotency_key).toBeTruthy();

    const [binding] = signMock.mock.calls[0]!;
    expect(authorization.binding).toBe(binding);
    expect(authorization.proof).toBe(PROOF);
    expect(binding.operation).toBe("policy_update");
    expect(binding.method).toBe("PUT");
    expect(binding.path).toBe("/api/v1/execass/policy");
    expect(binding.idempotency_key).toBe(request.idempotency_key);
    expect(binding.expected_revision).toBe(7);
    expect(binding.canonical_body_digest).toBe(
      await sha256Hex(policyUpdateBodyJson(request)),
    );
    expect(binding.safe_snapshot_digest).toBe(
      await sha256Hex(policySafeSnapshotCanonicalJson(request)),
    );

    // Success is claimed only after the authoritative response.
    expect(controller?.draft).not.toBeNull();
    expect(controller?.policy?.revision).toBe(7);
    await act(async () => {
      pending.resolve({
        policy: fixturePolicyResponse({ revision: 8, profile: "full_send" }),
        updated_at_ms: 2,
      });
    });
    await flushAsync();
    expect(controller?.draft).toBeNull();
    expect(controller?.policy?.revision).toBe(8);
    expect(controller?.updateBusy).toBe(false);
  });

  it("does not mistake the update's own stream echo for a stale identity", async () => {
    // The gateway broadcasts policy.changed for the PUT itself; the racing
    // refetch must not suppress the authoritative success or retain the
    // draft as a phantom conflict.
    const pending = deferred<{
      policy: PolicyResponse;
      updated_at_ms: number;
    }>();
    updatePolicyMock.mockReturnValue(pending.promise);
    const props = await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftChangeSummary("Echo race");
    });
    await act(async () => {
      void controller!.applyDraft();
    });
    await flushAsync();
    expect(updatePolicyMock).toHaveBeenCalledTimes(1);

    // The invalidation echo arrives and refetches while the PUT is open.
    getPolicyMock.mockResolvedValueOnce(fixturePolicyResponse({ revision: 8 }));
    await rerender({ ...props, policyInvalidationGeneration: 1 });
    expect(getPolicyMock).toHaveBeenCalledTimes(2);

    await act(async () => {
      pending.resolve({
        policy: fixturePolicyResponse({ revision: 8 }),
        updated_at_ms: 2,
      });
    });
    await flushAsync();
    expect(controller?.draft).toBeNull();
    expect(controller?.conflict).toBe(false);
    expect(controller?.policy?.revision).toBe(8);
  });

  it("keeps the draft and shows honest recovery on failure", async () => {
    updatePolicyMock.mockRejectedValue(
      new ExecassApiError({ kind: "network", path: "/api/v1/execass/policy" }),
    );
    await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftChangeSummary("Doomed update");
    });
    let outcome: Awaited<ReturnType<ExecassPolicyController["applyDraft"]>>;
    await act(async () => {
      outcome = await controller!.applyDraft();
    });
    expect(outcome!.ok).toBe(false);
    expect(controller?.draft?.changeSummary).toBe("Doomed update");
    expect(controller?.policy?.revision).toBe(7);
    expect(controller?.updateBusy).toBe(false);
  });

  it("refetches on conflict, blocks stale retry, and reconciles only owner-edited fields onto latest truth", async () => {
    updatePolicyMock.mockRejectedValue(conflictError());
    await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftProfile("full_send");
      controller!.setDraftRule(0, {
        ...fixturePolicyResponse().rules[0]!,
        parallelism_limit: 6,
      });
      controller!.setDraftChangeSummary("Conflicted update");
    });
    const latest = fixturePolicyResponse({
      revision: 8,
      rules: [
        {
          ...fixturePolicyResponse().rules[0]!,
          clarification_sensitivity: "strict",
        },
        {
          ...fixturePolicyResponse().rules[0]!,
          rule_id: "concurrent-rule",
          parallelism_limit: 2,
        },
      ],
    });
    getPolicyMock.mockResolvedValueOnce(latest);
    let outcome: Awaited<ReturnType<ExecassPolicyController["applyDraft"]>>;
    await act(async () => {
      outcome = await controller!.applyDraft();
    });
    expect(outcome!.ok).toBe(false);
    expect(getPolicyMock).toHaveBeenCalledTimes(2);
    expect(controller?.policy?.revision).toBe(8);
    expect(controller?.draft?.changeSummary).toBe("Conflicted update");
    expect(controller?.conflict).toBe(true);

    updatePolicyMock.mockClear();
    signMock.mockClear();
    await act(async () => {
      outcome = await controller!.applyDraft();
    });
    expect(outcome!.ok).toBe(false);
    expect(signMock).not.toHaveBeenCalled();
    expect(updatePolicyMock).not.toHaveBeenCalled();

    act(() => {
      outcome = controller!.reconcileDraft();
    });
    expect(outcome!.ok).toBe(true);
    expect(controller?.conflict).toBe(false);
    expect(controller?.draft?.baseRevision).toBe(8);
    expect(controller?.draft?.profile).toBe("full_send");
    expect(controller?.draft?.rules).toEqual([
      {
        ...latest.rules[0],
        parallelism_limit: 6,
      },
      latest.rules[1],
    ]);
  });

  it("rejects an old-scope PUT after same-gateway token replacement", async () => {
    const pending = deferred<{
      policy: PolicyResponse;
      updated_at_ms: number;
    }>();
    updatePolicyMock.mockReturnValue(pending.promise);
    const props = await mount();
    act(() => {
      controller!.beginDraft();
      controller!.setDraftChangeSummary("Old token update");
    });
    let outcomePromise!: Promise<
      Awaited<ReturnType<ExecassPolicyController["applyDraft"]>>
    >;
    act(() => {
      outcomePromise = controller!.applyDraft();
    });
    await flushAsync();
    expect(updatePolicyMock).toHaveBeenCalledTimes(1);

    getPolicyMock.mockReturnValueOnce(new Promise<PolicyResponse>(() => {}));
    await rerender({ ...props, authIdentityGeneration: 1 });
    expect(controller?.policy).toBeNull();
    expect(controller?.draft).toBeNull();

    await act(async () => {
      pending.resolve({
        policy: fixturePolicyResponse({ revision: 8 }),
        updated_at_ms: 2,
      });
    });
    await expect(outcomePromise).resolves.toEqual({
      ok: false,
      message: "The gateway identity changed.",
    });
    expect(controller?.policy).toBeNull();
  });
});
