// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  acknowledgeExecassSummary,
  engageExecassStopAll,
  ExecassApiError,
  execassIntake,
  getExecassDelegation,
  getExecassStopAllStatus,
  getExecassSummary,
  resolveExecassDecision,
} from "../../glass/execass/api";
import {
  fixtureAttentionItem,
  fixtureDecisionSummary,
  fixtureDelegationSummary,
  fixtureDelegationDetailResponse,
  fixtureIntakeConversationalResponse,
  fixtureIntakeDelegationResponse,
  fixtureResolveDecisionResponse,
  fixtureStopAllStatus,
  fixtureSummaryResponse,
} from "../../glass/execass/fixtures";
import type { ExecassWsFrame } from "../../glass/execass/types";
import {
  signExecassLocalDecision,
  signExecassLocalOwnerIntake,
  signExecassLocalRunControl,
} from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";
import {
  useExecassOfficeController,
  type ExecassOfficeController,
} from "./useExecassOfficeController";

vi.mock("../../glass/execass/api", async () => {
  const actual = await vi.importActual<
    typeof import("../../glass/execass/api")
  >("../../glass/execass/api");
  return {
    ...actual,
    getExecassSummary: vi.fn(),
    acknowledgeExecassSummary: vi.fn(),
    execassIntake: vi.fn(),
    resolveExecassDecision: vi.fn(),
    getExecassDelegation: vi.fn(),
    getExecassStopAllStatus: vi.fn(),
    engageExecassStopAll: vi.fn(),
    resumeExecassAll: vi.fn(),
    listExecassDelegationReceipts: vi.fn(),
  };
});

vi.mock("../../lib/runtime", () => ({
  signExecassLocalDecision: vi.fn(),
  signExecassLocalOwnerIntake: vi.fn(),
  signExecassLocalRunControl: vi.fn(),
  isTauriRuntime: () => true,
}));

const settings: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

const PROOF = {
  authenticated_client_id: "carsinos-desktop",
  request_correlation_id: "corr",
  proof_hex: "ab".repeat(32),
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function integrityQuarantineError() {
  return new ExecassApiError({
    kind: "http",
    path: "/api/v1/execass/summary",
    status: 503,
    apiError: {
      code: "execass.v1.receipt_integrity_quarantined",
      safe_human_message:
        "Receipt integrity is quarantined until verification recovers.",
      retryable: true,
      correlation_id: "corr-integrity",
      safe_for_display: true,
      exposes_sensitive_metadata: false,
    },
  });
}

let container: HTMLDivElement;
let root: Root | null = null;
let controller: ExecassOfficeController | null = null;
let notices: Array<{ tone: "info" | "error" | "critical"; message: string } | null> = [];

function Harness(props: {
  active: boolean;
  settingsOverride?: RuntimeConnectionSettings;
  authIdentityGeneration?: number;
}) {
  const current = useExecassOfficeController({
    settings: props.settingsOverride ?? settings,
    tokenConfigured: true,
    authIdentityGeneration: props.authIdentityGeneration ?? 0,
    active: props.active,
    setNotice: (notice) => notices.push(notice),
  });
  useEffect(() => {
    controller = current;
  });
  return null;
}

async function mount(active = true) {
  container = document.createElement("div");
  document.body.appendChild(container);
  await act(async () => {
    root = createRoot(container);
    root.render(<Harness active={active} />);
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  notices = [];
  vi.mocked(getExecassSummary).mockResolvedValue(fixtureSummaryResponse());
  vi.mocked(acknowledgeExecassSummary).mockResolvedValue({
    acknowledged: true,
    displayed: fixtureSummaryResponse().displayed,
    acknowledged_at_ms: 1,
  });
  vi.mocked(getExecassStopAllStatus).mockResolvedValue(fixtureStopAllStatus());
});

afterEach(async () => {
  await act(async () => {
    root?.unmount();
  });
  container.remove();
  controller = null;
});

describe("useExecassOfficeController", () => {
  it("loads the authoritative summary on mount and composes the briefing", async () => {
    await mount();
    expect(getExecassSummary).toHaveBeenCalled();
    expect(controller?.summary?.needs_you).toHaveLength(2);
    expect(controller?.briefing?.needsCount).toBe(2);
  });

  it("acknowledges the displayed summary revision after load", async () => {
    await mount();
    const ack = vi.mocked(acknowledgeExecassSummary).mock.calls[0]?.[1];
    expect(ack?.displayed.cursor).toBe("cursor-412");
    expect(ack?.idempotency_key).toBeTruthy();
  });

  it("loads cross-floor facts in the background without claiming they were displayed", async () => {
    await mount(false);
    expect(getExecassSummary).toHaveBeenCalledTimes(1);
    expect(getExecassStopAllStatus).toHaveBeenCalledTimes(1);
    expect(controller?.summary).not.toBeNull();
    expect(acknowledgeExecassSummary).not.toHaveBeenCalled();

    await act(async () => {
      root!.render(<Harness active />);
    });
    expect(acknowledgeExecassSummary).toHaveBeenCalledTimes(1);
  });

  it("invalidates same-url facts on auth replacement and ignores the old identity response", async () => {
    const oldRead = deferred<ReturnType<typeof fixtureSummaryResponse>>();
    const replacement = fixtureSummaryResponse();
    replacement.displayed.cursor = "cursor-replacement";
    vi.mocked(getExecassSummary)
      .mockReturnValueOnce(oldRead.promise)
      .mockResolvedValueOnce(replacement);

    await mount(false);
    expect(controller?.summary).toBeNull();

    await act(async () => {
      root!.render(<Harness active={false} authIdentityGeneration={1} />);
    });
    expect(controller?.summary?.displayed.cursor).toBe("cursor-replacement");

    const stale = fixtureSummaryResponse();
    stale.displayed.cursor = "cursor-stale";
    await act(async () => {
      oldRead.resolve(stale);
      await oldRead.promise;
    });
    expect(controller?.summary?.displayed.cursor).toBe("cursor-replacement");
    expect(acknowledgeExecassSummary).not.toHaveBeenCalled();
  });

  it("resolves a decision with a server-derived binding and refetches", async () => {
    vi.mocked(signExecassLocalDecision).mockResolvedValue(PROOF);
    vi.mocked(resolveExecassDecision).mockResolvedValue(
      fixtureResolveDecisionResponse(),
    );
    await mount();
    const decision = fixtureDecisionSummary();
    await act(async () => {
      await controller!.resolveDecision(decision, "confirm_and_continue");
    });
    const [binding] = vi.mocked(signExecassLocalDecision).mock.calls[0]!;
    expect(binding.decision_id).toBe("dec-mailchimp");
    expect(binding.challenge_digest).toBe(
      decision.local_owner_proof_challenge!.challenge_digest,
    );
    const [, decisionId, request] = vi.mocked(resolveExecassDecision).mock
      .calls[0]!;
    expect(decisionId).toBe("dec-mailchimp");
    expect(request.local_proof).toEqual(PROOF);
    expect(request.local_proof_binding).toEqual(binding);
    expect(request.challenge_response).toBe("nonce-1");
    expect(binding.challenge_response_digest).toBe(
      "9e3f156324d42f0ea4b6f4fce81d56fbd64a2143a3fdd60a130d9c90e5b4d688",
    );
    expect(notices.at(-1)?.message).toMatch(/work continues/i);
    // work continues: the summary is refetched, never trusted from memory
    expect(vi.mocked(getExecassSummary).mock.calls.length).toBeGreaterThan(1);
  });

  it("delegates an outcome through the signed intake path", async () => {
    vi.mocked(signExecassLocalOwnerIntake).mockResolvedValue({
      ...PROOF,
      request_id: "r",
      idempotency_key: "i",
      attach_to_delegation_id: null,
      normalized_intent_digest: "d",
      instruction_digest: "d2",
    });
    vi.mocked(execassIntake).mockResolvedValue(
      fixtureIntakeConversationalResponse(),
    );
    await mount();
    await act(async () => {
      await controller!.delegate("Are the invoices paid?");
    });
    const [request] = vi.mocked(signExecassLocalOwnerIntake).mock.calls[0]!;
    expect(request.text).toBe("Are the invoices paid?");
    expect(vi.mocked(execassIntake)).toHaveBeenCalled();
    expect(controller?.conversationalReply).toContain("already paid");
  });

  it("engages the freeze switch through the run-control proof path", async () => {
    vi.mocked(signExecassLocalRunControl).mockResolvedValue(PROOF);
    vi.mocked(engageExecassStopAll).mockResolvedValue(
      fixtureStopAllStatus({ engaged: true, drain_state: "draining" }),
    );
    await mount();
    await act(async () => {
      await controller!.freezeAll();
    });
    const [binding] = vi.mocked(signExecassLocalRunControl).mock.calls[0]!;
    expect(binding.operation).toBe("global_stop");
    expect(controller?.stopAll?.engaged).toBe(true);
  });

  it("on summary_refetch_required: refetches, then resumes with the exact consumer cursor", async () => {
    await mount();
    const sent: string[] = [];
    act(() => {
      controller!.handleWsOpen((text) => sent.push(text));
      controller!.notifyGatewayStatus();
    });
    // initial resume after gateway.status
    expect(sent).toHaveLength(1);
    expect(JSON.parse(sent[0]!)).toMatchObject({
      type: "execass.v1.resume",
      cursor: 0,
    });

    const refetch: ExecassWsFrame = {
      type: "execass.v1.summary_refetch_required",
      reason: "gap",
      consumer_cursor: 941,
      requested_cursor: 970,
      head_global_sequence: 999,
    };
    await act(async () => {
      controller!.handleExecassFrame(refetch);
    });
    // refetched the summary...
    expect(vi.mocked(getExecassSummary).mock.calls.length).toBeGreaterThan(1);
    // ...and resumed with consumer_cursor 941, never 970 or 999
    const resume = JSON.parse(sent[sent.length - 1]!);
    expect(resume).toMatchObject({ type: "execass.v1.resume", cursor: 941 });
  });

  it("starts the replacement auth identity at cursor zero instead of reusing the prior stream", async () => {
    await mount(false);
    const firstIdentitySent: string[] = [];
    act(() => {
      controller!.handleWsOpen((text) => firstIdentitySent.push(text));
    });
    await act(async () => {
      controller!.handleExecassFrame({
        type: "execass.v1.summary_refetch_required",
        reason: "gap",
        consumer_cursor: 941,
        requested_cursor: 970,
        head_global_sequence: 999,
      });
    });
    expect(JSON.parse(firstIdentitySent.at(-1)!)).toMatchObject({
      cursor: 941,
    });

    await act(async () => {
      root!.render(
        <Harness active={false} authIdentityGeneration={1} />,
      );
    });
    const replacementIdentitySent: string[] = [];
    act(() => {
      controller!.handleWsOpen((text) =>
        replacementIdentitySent.push(text),
      );
      controller!.notifyGatewayStatus();
    });
    expect(JSON.parse(replacementIdentitySent[0]!)).toMatchObject({
      type: "execass.v1.resume",
      cursor: 0,
    });
    expect(
      localStorage.getItem(
        "mc-execass-cursor-v1:mission-control-desktop",
      ),
    ).toBe("0");
  });

  it("does not persist or resume a consumer cursor when authoritative refetch fails", async () => {
    await mount();
    const sent: string[] = [];
    act(() => {
      controller!.handleWsOpen((text) => sent.push(text));
      controller!.notifyGatewayStatus();
    });
    vi.mocked(getExecassSummary).mockRejectedValueOnce(new Error("offline"));
    await act(async () => {
      controller!.handleExecassFrame({
        type: "execass.v1.summary_refetch_required",
        reason: "gap",
        consumer_cursor: 941,
        requested_cursor: 970,
        head_global_sequence: 999,
      });
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(sent).toHaveLength(1);
    expect(localStorage.getItem("mc-execass-cursor-v1:mission-control-desktop")).toBeNull();
  });

  it("serializes overlapping refetch demands into one authoritative summary request", async () => {
    await mount();
    const sent: string[] = [];
    let release!: (value: ReturnType<typeof fixtureSummaryResponse>) => void;
    const pending = new Promise<ReturnType<typeof fixtureSummaryResponse>>((resolve) => {
      release = resolve;
    });
    vi.mocked(getExecassSummary).mockReturnValueOnce(pending);
    act(() => {
      controller!.handleWsOpen((text) => sent.push(text));
      controller!.handleExecassFrame({
        type: "execass.v1.summary_refetch_required",
        reason: "gap",
        consumer_cursor: 940,
        requested_cursor: 970,
        head_global_sequence: 999,
      });
      controller!.handleExecassFrame({
        type: "execass.v1.summary_refetch_required",
        reason: "gap",
        consumer_cursor: 941,
        requested_cursor: 970,
        head_global_sequence: 999,
      });
    });
    expect(vi.mocked(getExecassSummary)).toHaveBeenCalledTimes(2);
    await act(async () => {
      release(fixtureSummaryResponse());
      await pending;
    });
    expect(JSON.parse(sent.at(-1)!)).toMatchObject({ cursor: 941 });
    expect(vi.mocked(getExecassSummary)).toHaveBeenCalledTimes(2);
  });

  it("does not claim continuation when a recorded decision has no continuation id", async () => {
    vi.mocked(signExecassLocalDecision).mockResolvedValue(PROOF);
    vi.mocked(resolveExecassDecision).mockResolvedValue({
      ...fixtureResolveDecisionResponse(),
      continuation_id: null,
    });
    await mount();
    await act(async () => {
      await controller!.resolveDecision(fixtureDecisionSummary(), "decline");
    });
    expect(notices.at(-1)?.message).toMatch(/declined/i);
    expect(notices.at(-1)?.message).not.toMatch(/continues/i);
  });

  it("resolves an attention item via the authoritative pending decision", async () => {
    vi.mocked(signExecassLocalDecision).mockResolvedValue(PROOF);
    vi.mocked(resolveExecassDecision).mockResolvedValue(
      fixtureResolveDecisionResponse(),
    );
    vi.mocked(getExecassDelegation).mockResolvedValue({
      detail: {
        delegation: fixtureDelegationSummary({
          delegation_id: "dlg-mailchimp",
          phase: "waiting_for_user",
          pending_decision: fixtureDecisionSummary(),
        }),
        original_intent: "Close the old Mailchimp account",
        plan_summary: "Close after verified export",
        actions: [],
        continuations: [],
        effects: [],
        completion_verifiers: [],
        outcome_criteria: [],
        authority_snapshot_ref: "auth-1",
        immutable_intake_evidence_ref: "evid-1",
        ingress_source: "local",
        internal_record_refs: [],
        source_correlation_id: "corr",
        technical_resource_summary: "light",
        receipt_chain_head: null,
        recovery: null,
      },
    });
    await mount();
    await act(async () => {
      await controller!.resolveAttention(
        fixtureAttentionItem(),
        "confirm_and_continue",
      );
    });
    expect(getExecassDelegation).toHaveBeenCalledWith(
      settings,
      "dlg-mailchimp",
    );
    expect(resolveExecassDecision).toHaveBeenCalled();
  });

  it("refetches instead of resolving when the pending decision changed", async () => {
    vi.mocked(getExecassDelegation).mockResolvedValue({
      detail: {
        delegation: fixtureDelegationSummary({
          delegation_id: "dlg-mailchimp",
          pending_decision: fixtureDecisionSummary({
            decision_id: "dec-DIFFERENT",
          }),
        }),
        original_intent: "x",
        plan_summary: "x",
        actions: [],
        continuations: [],
        effects: [],
        completion_verifiers: [],
        outcome_criteria: [],
        authority_snapshot_ref: "a",
        immutable_intake_evidence_ref: "e",
        ingress_source: "local",
        internal_record_refs: [],
        source_correlation_id: "c",
        technical_resource_summary: "t",
        receipt_chain_head: null,
        recovery: null,
      },
    });
    await mount();
    await act(async () => {
      await controller!.resolveAttention(
        fixtureAttentionItem(),
        "confirm_and_continue",
      );
    });
    expect(resolveExecassDecision).not.toHaveBeenCalled();
    expect(vi.mocked(getExecassSummary).mock.calls.length).toBeGreaterThan(1);
  });

  it("collects tray notes from scheduled notifications", async () => {
    await mount();
    const frame: ExecassWsFrame = {
      type: "execass.v1.event",
      event: {
        event_name: "execass.v1.notification.scheduled",
        aggregate_id: "n",
        revision: 1,
        correlation_id: "c",
        causation_id: "c",
        occurred_at_ms: 42,
        schema_version: "v1",
        safe_payload: {
          summary: "Reminder: venue decision tonight",
          authoritative_deep_link: null,
          decision_id: null,
          delegation_id: null,
          receipt_ref: null,
        },
        global_sequence: 1,
        duplicate_identity: "dup-1",
      },
    };
    act(() => {
      controller!.handleExecassFrame(frame);
    });
    expect(controller?.trayNotes).toHaveLength(1);
    expect(controller?.trayNotes[0]?.text).toContain("venue decision");
  });

  it("bumps the policy invalidation generation only for policy.changed events", async () => {
    await mount();
    expect(controller?.policyInvalidationGeneration).toBe(0);
    const policyChanged: ExecassWsFrame = {
      type: "execass.v1.event",
      event: {
        event_name: "execass.v1.policy.changed",
        aggregate_id: "policy",
        revision: 8,
        correlation_id: "c",
        causation_id: "c",
        occurred_at_ms: 42,
        schema_version: "v1",
        safe_payload: {
          summary: "Policy changed",
          authoritative_deep_link: null,
          decision_id: null,
          delegation_id: null,
          receipt_ref: null,
        },
        global_sequence: 1,
        duplicate_identity: "dup-policy-1",
      },
    };
    act(() => {
      controller!.handleExecassFrame(policyChanged);
    });
    expect(controller?.policyInvalidationGeneration).toBe(1);
    const summaryChanged: ExecassWsFrame = {
      type: "execass.v1.event",
      event: {
        event_name: "execass.v1.summary.changed",
        aggregate_id: "summary",
        revision: 9,
        correlation_id: "c",
        causation_id: "c",
        occurred_at_ms: 43,
        schema_version: "v1",
        safe_payload: {
          summary: "Summary changed",
          authoritative_deep_link: null,
          decision_id: null,
          delegation_id: null,
          receipt_ref: null,
        },
        global_sequence: 2,
        duplicate_identity: "dup-summary-2",
      },
    };
    act(() => {
      controller!.handleExecassFrame(summaryChanged);
    });
    expect(controller?.policyInvalidationGeneration).toBe(1);
  });
});

describe("desk conversation (converse)", () => {
  it("passes the attached delegation through intake and returns the reply", async () => {
    vi.mocked(signExecassLocalOwnerIntake).mockResolvedValue({
      ...PROOF,
      request_id: "req",
      idempotency_key: "idem",
      attach_to_delegation_id: "dlg-retreat",
      normalized_intent_digest: "sha256:x",
      instruction_digest: "sha256:y",
    });
    vi.mocked(execassIntake).mockResolvedValue(
      fixtureIntakeConversationalResponse(),
    );
    await mount();
    let outcome: Awaited<ReturnType<ExecassOfficeController["converse"]>>;
    await act(async () => {
      outcome = await controller!.converse("how is the retreat going?", "dlg-retreat");
    });
    expect(outcome!).toMatchObject({ kind: "conversational" });
    if (outcome!.kind === "conversational") {
      expect(outcome!.text).toContain("already paid");
    }
    const [, request] = vi.mocked(execassIntake).mock.calls[0]!;
    expect(request.attach_to_delegation_id).toBe("dlg-retreat");
    expect(controller?.conversationalReply).toBeNull();
  });

  it("returns the created delegation and refreshes the summary", async () => {
    vi.mocked(signExecassLocalOwnerIntake).mockResolvedValue({
      ...PROOF,
      request_id: "req",
      idempotency_key: "idem",
      attach_to_delegation_id: null,
      normalized_intent_digest: "sha256:x",
      instruction_digest: "sha256:y",
    });
    vi.mocked(execassIntake).mockResolvedValue(
      fixtureIntakeDelegationResponse(),
    );
    await mount();
    const summaryCallsBefore = vi.mocked(getExecassSummary).mock.calls.length;
    let outcome: Awaited<ReturnType<ExecassOfficeController["converse"]>>;
    await act(async () => {
      outcome = await controller!.converse("chase the invoices");
    });
    expect(outcome!.kind).toBe("delegation");
    if (outcome!.kind === "delegation") {
      expect(outcome!.delegation.delegation_id).toBe("dlg-invoices");
    }
    expect(vi.mocked(getExecassSummary).mock.calls.length).toBeGreaterThan(
      summaryCallsBefore,
    );
  });

  it("returns a safe error outcome instead of a global notice", async () => {
    vi.mocked(signExecassLocalOwnerIntake).mockResolvedValue({
      ...PROOF,
      request_id: "req",
      idempotency_key: "idem",
      attach_to_delegation_id: null,
      normalized_intent_digest: "sha256:x",
      instruction_digest: "sha256:y",
    });
    vi.mocked(execassIntake).mockRejectedValue(new Error("boom"));
    await mount();
    const noticesBefore = notices.length;
    let outcome: Awaited<ReturnType<ExecassOfficeController["converse"]>>;
    await act(async () => {
      outcome = await controller!.converse("hello?");
    });
    expect(outcome!.kind).toBe("error");
    if (outcome!.kind === "error") {
      expect(outcome!.message).toContain("could not complete");
    }
    expect(notices.length).toBe(noticesBefore);
  });
});

describe("receipt integrity latch", () => {
  function integrityFrame(
    sequence: number,
    summary: string,
  ): ExecassWsFrame {
    return {
      type: "execass.v1.event",
      event: {
        event_name: "execass.v1.receipt.integrity_failed",
        aggregate_id: "receipts",
        revision: sequence,
        correlation_id: "c",
        causation_id: "c",
        occurred_at_ms: 42,
        schema_version: "v1",
        safe_payload: {
          summary,
          authoritative_deep_link: null,
          decision_id: null,
          delegation_id: null,
          receipt_ref: null,
        },
        global_sequence: sequence,
        duplicate_identity: `dup-integrity-${sequence}`,
      },
    };
  }

  it("latches the latest receipt integrity failure from the durable stream", async () => {
    await mount();
    vi.mocked(getExecassSummary).mockRejectedValue(integrityQuarantineError());
    expect(controller?.integrityFailure).toBeNull();
    act(() => {
      controller!.handleExecassFrame(
        integrityFrame(1, "Receipt chain verification failed."),
      );
    });
    expect(controller?.integrityFailure).toEqual({
      summary: "Receipt chain verification failed.",
      sequence: 1,
    });
    expect(
      notices.some(
        (notice) =>
          notice?.tone === "critical" &&
          notice.message.includes("Receipt chain verification failed."),
      ),
    ).toBe(true);
    act(() => {
      controller!.handleExecassFrame(
        integrityFrame(2, "A second receipt failed verification."),
      );
    });
    expect(controller?.integrityFailure).toEqual({
      summary: "A second receipt failed verification.",
      sequence: 2,
    });
  });

  it("discovers an already-active integrity quarantine from the authoritative summary read", async () => {
    vi.mocked(getExecassSummary).mockRejectedValue(integrityQuarantineError());
    await mount(false);
    expect(controller?.summary).toBeNull();
    expect(controller?.integrityFailure).toEqual({
      summary: "Receipt integrity is quarantined until verification recovers.",
      sequence: null,
    });
    expect(acknowledgeExecassSummary).not.toHaveBeenCalled();
  });

  it("clears the latch only after a later authoritative summary read succeeds", async () => {
    await mount(false);
    vi.mocked(getExecassSummary).mockRejectedValue(integrityQuarantineError());
    act(() => {
      controller!.handleExecassFrame(
        integrityFrame(1, "Receipt chain verification failed."),
      );
    });
    expect(controller?.integrityFailure).not.toBeNull();

    await act(async () => {
      await controller!.refreshSummary();
    });
    expect(controller?.integrityFailure).not.toBeNull();

    vi.mocked(getExecassSummary).mockResolvedValue(fixtureSummaryResponse());
    await act(async () => {
      await controller!.refreshSummary();
    });
    expect(controller?.integrityFailure).toBeNull();
  });

  it("follows an older in-flight read with a post-event quarantine verification", async () => {
    await mount(false);
    await act(async () => {
      await Promise.resolve();
    });
    const pendingRead = deferred<ReturnType<typeof fixtureSummaryResponse>>();
    vi.mocked(getExecassSummary)
      .mockReturnValueOnce(pendingRead.promise)
      .mockRejectedValueOnce(integrityQuarantineError());

    let refresh!: Promise<void>;
    act(() => {
      refresh = controller!.refreshSummary();
    });
    expect(getExecassSummary).toHaveBeenCalledTimes(2);
    act(() => {
      controller!.handleExecassFrame(
        integrityFrame(1, "A newer receipt failed verification."),
      );
    });
    await act(async () => {
      pendingRead.resolve(fixtureSummaryResponse());
      await refresh;
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(getExecassSummary).toHaveBeenCalledTimes(3);
    expect(controller?.integrityFailure).toEqual({
      summary: "A newer receipt failed verification.",
      sequence: 1,
    });
  });

  it("clears the latched integrity failure when the gateway identity changes", async () => {
    await mount();
    vi.mocked(getExecassSummary).mockRejectedValue(integrityQuarantineError());
    act(() => {
      controller!.handleExecassFrame(
        integrityFrame(1, "Receipt chain verification failed."),
      );
    });
    expect(controller?.integrityFailure).not.toBeNull();
    vi.mocked(getExecassSummary).mockResolvedValue(fixtureSummaryResponse());
    await act(async () => {
      root!.render(
        <Harness
          active
          settingsOverride={{ gateway_url: "http://127.0.0.1:28789" }}
        />,
      );
    });
    expect(controller?.integrityFailure).toBeNull();
  });

  it("clears the latch when the secure token identity changes at the same gateway", async () => {
    await mount();
    vi.mocked(getExecassSummary).mockRejectedValue(integrityQuarantineError());
    act(() => {
      controller!.handleExecassFrame(
        integrityFrame(1, "Receipt chain verification failed."),
      );
    });
    expect(controller?.integrityFailure).not.toBeNull();
    vi.mocked(getExecassSummary).mockResolvedValue(fixtureSummaryResponse());
    await act(async () => {
      root!.render(<Harness active authIdentityGeneration={1} />);
    });
    expect(controller?.integrityFailure).toBeNull();
  });
});

describe("loadDelegationDetail", () => {
  it("returns the authoritative detail for the over-the-shoulder pane", async () => {
    vi.mocked(getExecassDelegation).mockResolvedValue(
      fixtureDelegationDetailResponse(),
    );
    await mount();
    let detail: Awaited<ReturnType<ExecassOfficeController["loadDelegationDetail"]>>;
    await act(async () => {
      detail = await controller!.loadDelegationDetail("dlg-retreat");
    });
    expect(detail!.plan_summary).toContain("venue");
    expect(detail!.delegation.delegation_id).toBe("dlg-retreat");
  });
});
