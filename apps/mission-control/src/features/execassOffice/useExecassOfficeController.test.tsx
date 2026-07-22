// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  acknowledgeExecassSummary,
  engageExecassStopAll,
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
  fixtureIntakeConversationalResponse,
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

let container: HTMLDivElement;
let root: Root | null = null;
let controller: ExecassOfficeController | null = null;

function Harness(props: { active: boolean }) {
  const current = useExecassOfficeController({
    settings,
    tokenConfigured: true,
    active: props.active,
    setNotice: () => {},
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
        global_sequence: 2001,
        duplicate_identity: "dup-2001",
      },
    };
    act(() => {
      controller!.handleExecassFrame(frame);
    });
    expect(controller?.trayNotes).toHaveLength(1);
    expect(controller?.trayNotes[0]?.text).toContain("venue decision");
  });
});
