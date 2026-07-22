/**
 * The ExecAss Office controller: sole consumer of the executive projection.
 *
 * Summary/detail responses are authoritative; websocket events are only
 * invalidation/reconciliation signals. There is no frontend lifecycle,
 * approval, or danger logic here - decisions are rendered as typed
 * attention and resolved through the exact server challenge plus a native
 * proof that is requested, submitted once, and discarded.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  ExecassApiError,
  acknowledgeExecassSummary,
  engageExecassStopAll,
  execassIntake,
  getExecassDelegation,
  getExecassStopAllStatus,
  getExecassSummary,
  listExecassDelegationReceipts,
  resolveExecassDecision,
  resumeExecassAll,
} from "../../glass/execass/api";
import { composeBriefing, type Briefing } from "../../glass/execass/briefing";
import {
  buildDecisionResolution,
  buildIntakeRequest,
  buildRunControlBinding,
  buildResumeSnapshotFromStatus,
  trayNoteFromEnvelope,
  type TrayNote,
} from "../../glass/execass/officeActions";
import {
  buildResumeFrame,
  initialStreamState,
  invalidationTargets,
  loadStreamCursor,
  reduceFrame,
  resumeAfterRefetch,
  saveStreamCursor,
  type StreamState,
} from "../../glass/execass/stream";
import type {
  AttentionItem,
  DecisionResult,
  DecisionSummary,
  ExecassWsFrame,
  ReceiptSummary,
  StopAllStatusResponse,
  SummaryResponse,
} from "../../glass/execass/types";
import {
  signExecassLocalDecision,
  signExecassLocalOwnerIntake,
  signExecassLocalRunControl,
} from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";

const DEFAULT_CLIENT_ID = "mission-control-desktop";
const SUMMARY_REFRESH_DEBOUNCE_MS = 250;
const TRAY_CAP = 20;

export interface ExecassOfficeControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  active: boolean;
  setNotice: (
    notice: { tone: "info" | "error" | "critical"; message: string } | null,
  ) => void;
  clientId?: string;
}

export interface ExecassOfficeController {
  summary: SummaryResponse | null;
  summaryLoading: boolean;
  summaryError: string | null;
  briefing: Briefing | null;
  stopAll: StopAllStatusResponse | null;
  trayNotes: TrayNote[];
  resolvingDecisionIds: string[];
  intakeBusy: boolean;
  freezeBusy: boolean;
  conversationalReply: string | null;
  refreshSummary: () => Promise<void>;
  resolveDecision: (
    decision: DecisionSummary,
    result: DecisionResult,
    revisionText?: string,
  ) => Promise<void>;
  resolveAttention: (
    item: AttentionItem,
    result: DecisionResult,
    revisionText?: string,
  ) => Promise<void>;
  delegate: (text: string) => Promise<void>;
  clearConversationalReply: () => void;
  freezeAll: () => Promise<void>;
  resumeAllWork: () => Promise<void>;
  loadReceipts: (delegationId: string) => Promise<ReceiptSummary[]>;
  dismissTrayNote: (id: string) => void;
  handleExecassFrame: (frame: ExecassWsFrame) => void;
  handleWsOpen: (send: (text: string) => void) => void;
  notifyGatewayStatus: () => void;
}

function safeErrorMessage(error: unknown): string {
  if (error instanceof ExecassApiError) {
    return error.safeMessage;
  }
  return "The gateway could not complete this request.";
}

function newId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `id-${Math.random().toString(16).slice(2)}`;
}

export function useExecassOfficeController(
  options: ExecassOfficeControllerOptions,
): ExecassOfficeController {
  const { settings, tokenConfigured, active, setNotice } = options;
  const clientId = options.clientId ?? DEFAULT_CLIENT_ID;

  const [summary, setSummary] = useState<SummaryResponse | null>(null);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [stopAll, setStopAll] = useState<StopAllStatusResponse | null>(null);
  const [trayNotes, setTrayNotes] = useState<TrayNote[]>([]);
  const [resolvingDecisionIds, setResolvingDecisionIds] = useState<string[]>([]);
  const [intakeBusy, setIntakeBusy] = useState(false);
  const [freezeBusy, setFreezeBusy] = useState(false);
  const [conversationalReply, setConversationalReply] = useState<string | null>(
    null,
  );

  const streamRef = useRef<StreamState>(
    initialStreamState(loadStreamCursor(clientId)),
  );
  const sendRef = useRef<((text: string) => void) | null>(null);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef(false);
  const lastAckedCursorRef = useRef<string | null>(null);
  const enabled = tokenConfigured && settings.gateway_url.trim().length > 0;

  const briefing = useMemo(
    () => (summary ? composeBriefing(summary) : null),
    [summary],
  );

  const acknowledgeDisplayed = useCallback(
    (loaded: SummaryResponse) => {
      if (lastAckedCursorRef.current === loaded.displayed.cursor) {
        return;
      }
      lastAckedCursorRef.current = loaded.displayed.cursor;
      void acknowledgeExecassSummary(settings, {
        idempotency_key: newId(),
        displayed: loaded.displayed,
      }).catch(() => {
        // Ack is a courtesy to the projection; a miss only delays "seen".
        lastAckedCursorRef.current = null;
      });
    },
    [settings],
  );

  const refreshSummary = useCallback(async () => {
    if (!enabled || inFlightRef.current) {
      return;
    }
    inFlightRef.current = true;
    setSummaryLoading(true);
    try {
      const loaded = await getExecassSummary(settings);
      setSummary(loaded);
      setSummaryError(null);
      acknowledgeDisplayed(loaded);
    } catch (error: unknown) {
      setSummaryError(safeErrorMessage(error));
    } finally {
      inFlightRef.current = false;
      setSummaryLoading(false);
    }
  }, [acknowledgeDisplayed, enabled, settings]);

  const refreshStopAll = useCallback(async () => {
    if (!enabled) {
      return;
    }
    try {
      setStopAll(await getExecassStopAllStatus(settings));
    } catch {
      // Run-control status is re-fetched on the next signal.
    }
  }, [enabled, settings]);

  const queueSummaryRefresh = useCallback(() => {
    if (refreshTimerRef.current !== null) {
      return;
    }
    refreshTimerRef.current = setTimeout(() => {
      refreshTimerRef.current = null;
      void refreshSummary();
    }, SUMMARY_REFRESH_DEBOUNCE_MS);
  }, [refreshSummary]);

  useEffect(() => {
    if (!active || !enabled) {
      return;
    }
    void refreshSummary();
    void refreshStopAll();
  }, [active, enabled, refreshSummary, refreshStopAll]);

  useEffect(
    () => () => {
      if (refreshTimerRef.current !== null) {
        clearTimeout(refreshTimerRef.current);
      }
    },
    [],
  );

  const sendResume = useCallback(() => {
    const send = sendRef.current;
    if (!send) {
      return;
    }
    try {
      send(JSON.stringify(buildResumeFrame(clientId, streamRef.current.cursor)));
    } catch {
      // The socket owner handles reconnects; the next open resumes again.
    }
  }, [clientId]);

  const handleWsOpen = useCallback((send: (text: string) => void) => {
    sendRef.current = send;
  }, []);

  const notifyGatewayStatus = useCallback(() => {
    // Per the live-update contract the resume frame follows gateway.status.
    sendResume();
  }, [sendResume]);

  const handleExecassFrame = useCallback(
    (frame: ExecassWsFrame) => {
      const { state, effect } = reduceFrame(streamRef.current, frame);
      streamRef.current = state;
      if (effect.kind === "apply-event") {
        saveStreamCursor(clientId, state.cursor);
        const targets = invalidationTargets(effect.envelope);
        if (targets.includes("summary")) {
          queueSummaryRefresh();
        }
        if (targets.includes("stop-all")) {
          void refreshStopAll();
        }
        if (targets.includes("notifications")) {
          const note = trayNoteFromEnvelope(effect.envelope);
          if (note) {
            setTrayNotes((current) =>
              [note, ...current.filter((n) => n.id !== note.id)].slice(
                0,
                TRAY_CAP,
              ),
            );
          }
        }
        if (targets.includes("integrity")) {
          setNotice({
            tone: "critical",
            message: effect.envelope.safe_payload.summary,
          });
        }
        return;
      }
      if (effect.kind === "refetch-summary") {
        void (async () => {
          await refreshSummary();
          streamRef.current = resumeAfterRefetch(streamRef.current);
          saveStreamCursor(clientId, streamRef.current.cursor);
          sendResume();
        })();
      }
    },
    [clientId, queueSummaryRefresh, refreshStopAll, refreshSummary, sendResume, setNotice],
  );

  const resolveDecision = useCallback(
    async (
      decision: DecisionSummary,
      result: DecisionResult,
      revisionText?: string,
    ) => {
      if (resolvingDecisionIds.includes(decision.decision_id)) {
        return;
      }
      const built = buildDecisionResolution(decision, result, {
        now: Date.now(),
        ids: { idempotencyKey: newId(), correlationId: newId() },
        revisionText,
      });
      if (!built.ok) {
        setNotice({ tone: "info", message: built.reason });
        void refreshSummary();
        return;
      }
      setResolvingDecisionIds((current) => [...current, decision.decision_id]);
      try {
        const proof = await signExecassLocalDecision(
          built.binding,
          built.correlationId,
        );
        await resolveExecassDecision(settings, decision.decision_id, {
          idempotency_key: built.binding.idempotency_key,
          decision_revision: built.binding.decision_revision,
          result,
          local_proof: proof,
          local_proof_binding: built.binding,
          challenge_response: built.challengeResponse,
          revision_text: built.revisionText,
        });
        setNotice({
          tone: "info",
          message: "Done - the work continues. No second nudge.",
        });
        await refreshSummary();
      } catch (error: unknown) {
        if (
          error instanceof ExecassApiError &&
          (error.isRevisionOrIdempotencyConflict ||
            error.code === "execass.v1.decision_superseded" ||
            error.code === "execass.v1.decision_challenge_expired")
        ) {
          setNotice({ tone: "info", message: error.safeMessage });
          await refreshSummary();
        } else {
          setNotice({ tone: "error", message: safeErrorMessage(error) });
        }
      } finally {
        setResolvingDecisionIds((current) =>
          current.filter((id) => id !== decision.decision_id),
        );
      }
    },
    [refreshSummary, resolvingDecisionIds, setNotice, settings],
  );

  const resolveAttention = useCallback(
    async (item: AttentionItem, result: DecisionResult, revisionText?: string) => {
      if (item.subject.scope_kind !== "delegation" || !item.decision_id) {
        setNotice({
          tone: "info",
          message: "This item is informational - nothing to resolve here.",
        });
        return;
      }
      try {
        const response = await getExecassDelegation(
          settings,
          item.subject.delegation_id,
        );
        const decision = response.detail.delegation.pending_decision;
        if (!decision || decision.decision_id !== item.decision_id) {
          setNotice({
            tone: "info",
            message: "The work changed before this decision - refreshed it for you.",
          });
          await refreshSummary();
          return;
        }
        await resolveDecision(decision, result, revisionText);
      } catch (error: unknown) {
        setNotice({ tone: "error", message: safeErrorMessage(error) });
      }
    },
    [refreshSummary, resolveDecision, setNotice, settings],
  );

  const delegate = useCallback(
    async (text: string) => {
      const trimmed = text.trim();
      if (!trimmed || intakeBusy) {
        return;
      }
      setIntakeBusy(true);
      try {
        const request = buildIntakeRequest(trimmed, {
          ids: {
            requestId: newId(),
            idempotencyKey: newId(),
            correlationId: newId(),
          },
        });
        const proof = await signExecassLocalOwnerIntake(request);
        const outcome = await execassIntake(settings, request, proof);
        if (outcome.kind === "conversational") {
          setConversationalReply(outcome.response_text);
        } else {
          setConversationalReply(null);
          setNotice({
            tone: "info",
            message: `Delegated: ${outcome.delegation.intent_summary}`,
          });
          await refreshSummary();
        }
      } catch (error: unknown) {
        setNotice({ tone: "error", message: safeErrorMessage(error) });
      } finally {
        setIntakeBusy(false);
      }
    },
    [intakeBusy, refreshSummary, setNotice, settings],
  );

  const clearConversationalReply = useCallback(() => {
    setConversationalReply(null);
  }, []);

  const freezeAll = useCallback(async () => {
    if (freezeBusy) {
      return;
    }
    setFreezeBusy(true);
    try {
      const binding = buildRunControlBinding(
        "global_stop",
        { kind: "global" },
        { now: Date.now(), ids: { idempotencyKey: newId(), correlationId: newId() } },
      );
      const proof = await signExecassLocalRunControl(binding);
      const status = await engageExecassStopAll(settings, {
        binding,
        local_proof: proof,
      });
      setStopAll(status);
      setNotice({
        tone: "info",
        message: "Everybody froze. Work drains to a safe boundary and holds.",
      });
      await refreshSummary();
    } catch (error: unknown) {
      setNotice({ tone: "error", message: safeErrorMessage(error) });
    } finally {
      setFreezeBusy(false);
    }
  }, [freezeBusy, refreshSummary, setNotice, settings]);

  const resumeAllWork = useCallback(async () => {
    if (freezeBusy) {
      return;
    }
    setFreezeBusy(true);
    try {
      const current = await getExecassStopAllStatus(settings);
      const binding = buildRunControlBinding(
        "global_resume",
        { kind: "global" },
        { now: Date.now(), ids: { idempotencyKey: newId(), correlationId: newId() } },
        buildResumeSnapshotFromStatus(current),
      );
      const proof = await signExecassLocalRunControl(binding);
      const resumed = await resumeExecassAll(settings, {
        binding,
        local_proof: proof,
      });
      setStopAll(resumed.stop_all);
      setNotice({ tone: "info", message: "The floor is moving again." });
      await refreshSummary();
    } catch (error: unknown) {
      setNotice({ tone: "error", message: safeErrorMessage(error) });
    } finally {
      setFreezeBusy(false);
    }
  }, [freezeBusy, refreshSummary, setNotice, settings]);

  const loadReceipts = useCallback(
    async (delegationId: string) => {
      const response = await listExecassDelegationReceipts(
        settings,
        delegationId,
      );
      return response.receipts;
    },
    [settings],
  );

  const dismissTrayNote = useCallback((id: string) => {
    setTrayNotes((current) => current.filter((note) => note.id !== id));
  }, []);

  return {
    summary,
    summaryLoading,
    summaryError,
    briefing,
    stopAll,
    trayNotes,
    resolvingDecisionIds,
    intakeBusy,
    freezeBusy,
    conversationalReply,
    refreshSummary,
    resolveDecision,
    resolveAttention,
    delegate,
    clearConversationalReply,
    freezeAll,
    resumeAllWork,
    loadReceipts,
    dismissTrayNote,
    handleExecassFrame,
    handleWsOpen,
    notifyGatewayStatus,
  };
}
