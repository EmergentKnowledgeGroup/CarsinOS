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
  DelegationDetail,
  DelegationSummary,
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
  /** Changes whenever the secure token identity is replaced or cleared. */
  authIdentityGeneration: number;
  active: boolean;
  setNotice: (
    notice: { tone: "info" | "error" | "critical"; message: string } | null,
  ) => void;
  clientId?: string;
}

/**
 * Outcome of one desk exchange. Errors come back as data so the desk can
 * log them in the conversation instead of raising a global notice.
 */
export type ConverseOutcome =
  | { kind: "conversational"; text: string }
  | { kind: "delegation"; delegation: DelegationSummary }
  | { kind: "error"; message: string };

export type AttentionResolutionOutcome =
  | { ok: true; message: string }
  | { ok: false; message: string };

export interface ExecassOfficeController {
  summary: SummaryResponse | null;
  summaryLoading: boolean;
  summaryError: string | null;
  /**
   * Increments whenever the one durable stream reports a policy change.
   * Policy consumers refetch on this signal instead of opening a second
   * websocket or event store.
   */
  policyInvalidationGeneration: number;
  /**
   * Latest receipt integrity quarantine reported by the authoritative read
   * or durable stream. It stays latched until a later authoritative summary
   * succeeds or the credential identity changes; there is no local reset.
   */
  integrityFailure: { summary: string; sequence: number | null } | null;
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
  ) => Promise<AttentionResolutionOutcome>;
  resolveAttention: (
    item: AttentionItem,
    result: DecisionResult,
    revisionText?: string,
  ) => Promise<AttentionResolutionOutcome>;
  delegate: (text: string) => Promise<void>;
  converse: (
    text: string,
    attachToDelegationId?: string | null,
  ) => Promise<ConverseOutcome>;
  loadDelegationDetail: (delegationId: string) => Promise<DelegationDetail>;
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
  const {
    settings,
    tokenConfigured,
    authIdentityGeneration,
    active,
    setNotice,
  } = options;
  const clientId = options.clientId ?? DEFAULT_CLIENT_ID;
  const gatewayUrl = settings.gateway_url.trim();
  const enabled = tokenConfigured && gatewayUrl.length > 0;
  const identity = `${gatewayUrl}|${tokenConfigured ? "token" : "no-token"}|auth:${authIdentityGeneration}`;

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
  const [policyInvalidationGeneration, setPolicyInvalidationGeneration] =
    useState(0);
  const [integrityFailure, setIntegrityFailure] = useState<{
    summary: string;
    sequence: number | null;
  } | null>(null);
  const [renderedIdentity, setRenderedIdentity] = useState(identity);

  const streamRef = useRef<StreamState>(
    initialStreamState(loadStreamCursor(clientId)),
  );
  const sendRef = useRef<((text: string) => void) | null>(null);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const summaryRefreshPromiseRef = useRef<Promise<boolean> | null>(null);
  const streamReconcilePromiseRef = useRef<Promise<void> | null>(null);
  const lastAckedCursorRef = useRef<string | null>(null);
  const identityGenerationRef = useRef(0);
  const integritySignalGenerationRef = useRef(0);
  const cursorResetIdentityRef = useRef<string | null>(null);

  // Every displayed projection is credential-identity-bound work. Invalidate
  // it on the transition render itself, before a replacement request begins.
  if (renderedIdentity !== identity) {
    setRenderedIdentity(identity);
    identityGenerationRef.current += 1;
    integritySignalGenerationRef.current += 1;
    summaryRefreshPromiseRef.current = null;
    streamReconcilePromiseRef.current = null;
    lastAckedCursorRef.current = null;
    streamRef.current = initialStreamState(0);
    cursorResetIdentityRef.current = identity;
    sendRef.current = null;
    setSummary(null);
    setSummaryError(null);
    setSummaryLoading(enabled);
    setStopAll(null);
    setIntegrityFailure(null);
  }

  const briefing = useMemo(
    () => (summary ? composeBriefing(summary) : null),
    [summary],
  );

  useEffect(() => {
    if (cursorResetIdentityRef.current === identity) {
      saveStreamCursor(clientId, 0);
      cursorResetIdentityRef.current = null;
    }
  }, [clientId, identity]);

  const acknowledgeDisplayed = useCallback(
    (loaded: SummaryResponse) => {
      if (lastAckedCursorRef.current === loaded.displayed.cursor) {
        return;
      }
      const generation = identityGenerationRef.current;
      lastAckedCursorRef.current = loaded.displayed.cursor;
      void acknowledgeExecassSummary(settings, {
        idempotency_key: newId(),
        displayed: loaded.displayed,
      }).catch(() => {
        // Ack is a courtesy to the projection; a miss only delays "seen".
        if (
          identityGenerationRef.current === generation &&
          lastAckedCursorRef.current === loaded.displayed.cursor
        ) {
          lastAckedCursorRef.current = null;
        }
      });
    },
    [settings],
  );

  const refreshSummaryAuthoritatively = useCallback((): Promise<boolean> => {
    if (!enabled) {
      return Promise.resolve(false);
    }
    if (summaryRefreshPromiseRef.current) {
      return summaryRefreshPromiseRef.current;
    }
    setSummaryLoading(true);
    const generation = identityGenerationRef.current;
    const integrityGeneration = integritySignalGenerationRef.current;
    const request = (async () => {
      try {
        const loaded = await getExecassSummary(settings);
        if (identityGenerationRef.current !== generation) {
          return false;
        }
        setSummary(loaded);
        setSummaryError(null);
        // A successful authoritative summary read proves the current receipt
        // integrity quarantine is clear. Never let a request that started
        // before a newer integrity event erase that newer alarm.
        if (integritySignalGenerationRef.current === integrityGeneration) {
          setIntegrityFailure(null);
        }
        return true;
      } catch (error: unknown) {
        if (identityGenerationRef.current !== generation) {
          return false;
        }
        if (
          error instanceof ExecassApiError &&
          error.code === "execass.v1.receipt_integrity_quarantined"
        ) {
          integritySignalGenerationRef.current += 1;
          setIntegrityFailure((current) =>
            current ?? {
              summary: error.safeMessage,
              sequence: null,
            },
          );
        }
        setSummaryError(safeErrorMessage(error));
        return false;
      } finally {
        if (identityGenerationRef.current === generation) {
          setSummaryLoading(false);
        }
      }
    })();
    summaryRefreshPromiseRef.current = request;
    void request.finally(() => {
      if (summaryRefreshPromiseRef.current === request) {
        summaryRefreshPromiseRef.current = null;
      }
    });
    return request;
  }, [enabled, settings]);

  const refreshSummary = useCallback(async () => {
    await refreshSummaryAuthoritatively();
  }, [refreshSummaryAuthoritatively]);

  const refreshStopAll = useCallback(async () => {
    if (!enabled) {
      return;
    }
    const generation = identityGenerationRef.current;
    try {
      const loaded = await getExecassStopAllStatus(settings);
      if (identityGenerationRef.current === generation) {
        setStopAll(loaded);
      }
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
    if (!enabled) {
      return;
    }
    void refreshSummary();
    void refreshStopAll();
  }, [enabled, identity, refreshSummary, refreshStopAll]);

  // Background shell reads never claim that the owner saw the projection.
  // Acknowledgement happens only while the actual Office surface is active.
  useEffect(() => {
    if (active && summary) {
      acknowledgeDisplayed(summary);
    }
  }, [acknowledgeDisplayed, active, summary]);

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

  const reconcileStreamAfterRefetch = useCallback(() => {
    if (streamReconcilePromiseRef.current) {
      return streamReconcilePromiseRef.current;
    }
    const request = (async () => {
      const refreshed = await refreshSummaryAuthoritatively();
      if (!refreshed || !streamRef.current.refetchRequired) {
        return;
      }
      streamRef.current = resumeAfterRefetch(streamRef.current);
      saveStreamCursor(clientId, streamRef.current.cursor);
      sendResume();
    })();
    streamReconcilePromiseRef.current = request;
    void request.finally(() => {
      if (streamReconcilePromiseRef.current === request) {
        streamReconcilePromiseRef.current = null;
      }
    });
    return request;
  }, [clientId, refreshSummaryAuthoritatively, sendResume]);

  const handleWsOpen = useCallback((send: (text: string) => void) => {
    sendRef.current = send;
  }, []);

  const notifyGatewayStatus = useCallback(() => {
    // Per the live-update contract the resume frame follows gateway.status.
    if (streamRef.current.refetchRequired) {
      void reconcileStreamAfterRefetch();
    } else {
      sendResume();
    }
  }, [reconcileStreamAfterRefetch, sendResume]);

  const handleExecassFrame = useCallback(
    (frame: ExecassWsFrame) => {
      const { state, effect } = reduceFrame(streamRef.current, frame);
      streamRef.current = state;
      if (effect.kind === "apply-event") {
        saveStreamCursor(clientId, state.cursor);
        const targets = invalidationTargets(effect.envelope);
        const integrityFailed = targets.includes("integrity");
        if (targets.includes("summary") && !integrityFailed) {
          queueSummaryRefresh();
        }
        if (targets.includes("stop-all")) {
          void refreshStopAll();
        }
        if (targets.includes("policy")) {
          setPolicyInvalidationGeneration((generation) => generation + 1);
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
        if (integrityFailed) {
          integritySignalGenerationRef.current += 1;
          const integrityGeneration =
            integritySignalGenerationRef.current;
          const identityGeneration = identityGenerationRef.current;
          const readAlreadyInFlight =
            summaryRefreshPromiseRef.current !== null;
          setIntegrityFailure({
            summary: effect.envelope.safe_payload.summary,
            sequence: effect.envelope.global_sequence,
          });
          setNotice({
            tone: "critical",
            message: effect.envelope.safe_payload.summary,
          });
          // Verify the durable alarm immediately against the authoritative
          // quarantine read. Ordinary projection invalidations stay debounced.
          const verification = refreshSummaryAuthoritatively();
          if (readAlreadyInFlight) {
            // The shared request predates this alarm. After it settles, force
            // exactly one post-event quarantine read (concurrent alarms
            // coalesce onto that replacement request).
            void verification.finally(() => {
              if (
                identityGenerationRef.current === identityGeneration &&
                integritySignalGenerationRef.current >= integrityGeneration
              ) {
                void refreshSummaryAuthoritatively();
              }
            });
          }
        }
        return;
      }
      if (effect.kind === "refetch-summary") {
        void reconcileStreamAfterRefetch();
      }
    },
    [
      clientId,
      queueSummaryRefresh,
      reconcileStreamAfterRefetch,
      refreshStopAll,
      refreshSummaryAuthoritatively,
      setNotice,
    ],
  );

  const resolveDecision = useCallback(
    async (
      decision: DecisionSummary,
      result: DecisionResult,
      revisionText?: string,
    ): Promise<AttentionResolutionOutcome> => {
      if (resolvingDecisionIds.includes(decision.decision_id)) {
        return {
          ok: false,
          message: "That decision is already being resolved.",
        };
      }
      try {
        const built = await buildDecisionResolution(decision, result, {
          now: Date.now(),
          ids: { idempotencyKey: newId(), correlationId: newId() },
          revisionText,
        });
        if (!built.ok) {
          setNotice({ tone: "info", message: built.reason });
          void refreshSummary();
          return { ok: false, message: built.reason };
        }
        setResolvingDecisionIds((current) => [...current, decision.decision_id]);
        const proof = await signExecassLocalDecision(
          built.binding,
          built.correlationId,
        );
        const outcome = await resolveExecassDecision(settings, decision.decision_id, {
          idempotency_key: built.binding.idempotency_key,
          decision_revision: built.binding.decision_revision,
          result,
          local_proof: proof,
          local_proof_binding: built.binding,
          challenge_response: built.challengeResponse,
          revision_text: built.revisionText,
        });
        const message = outcome.continuation_id
          ? result === "confirm_and_continue"
            ? "Confirmed - the work continues. No second nudge."
            : result === "revise"
              ? "Revision accepted - the updated work continues."
              : result === "decline"
                ? "Declined - ExecAss is continuing only the unaffected work."
                : "Stop recorded - ExecAss is continuing only to the safe boundary."
          : result === "revise"
              ? "Revision sent. ExecAss will show the updated plan when it is ready."
              : result === "decline"
                ? "Declined. ExecAss will reconcile the remaining work."
                : result === "stop"
                  ? "Stop recorded. ExecAss will hold at a safe boundary."
                  : "Decision recorded. ExecAss will show the next authoritative state.";
        setNotice({ tone: "info", message });
        await refreshSummary();
        return { ok: true, message };
      } catch (error: unknown) {
        if (
          error instanceof ExecassApiError &&
          (error.isRevisionOrIdempotencyConflict ||
            error.code === "execass.v1.decision_superseded" ||
            error.code === "execass.v1.decision_challenge_expired")
        ) {
          setNotice({ tone: "info", message: error.safeMessage });
          await refreshSummary();
          return { ok: false, message: error.safeMessage };
        } else {
          const message = safeErrorMessage(error);
          setNotice({ tone: "error", message });
          return { ok: false, message };
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
    async (
      item: AttentionItem,
      result: DecisionResult,
      revisionText?: string,
    ): Promise<AttentionResolutionOutcome> => {
      if (item.subject.scope_kind !== "delegation" || !item.decision_id) {
        const message = "This item is informational - nothing to resolve here.";
        setNotice({
          tone: "info",
          message,
        });
        return { ok: false, message };
      }
      try {
        const response = await getExecassDelegation(
          settings,
          item.subject.delegation_id,
        );
        const decision = response.detail.delegation.pending_decision;
        if (!decision || decision.decision_id !== item.decision_id) {
          const message =
            "The work changed before this decision - refreshed it for you.";
          setNotice({
            tone: "info",
            message,
          });
          await refreshSummary();
          return { ok: false, message };
        }
        return await resolveDecision(decision, result, revisionText);
      } catch (error: unknown) {
        const message = safeErrorMessage(error);
        setNotice({ tone: "error", message });
        return { ok: false, message };
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

  const converse = useCallback(
    async (
      text: string,
      attachToDelegationId?: string | null,
    ): Promise<ConverseOutcome> => {
      const trimmed = text.trim();
      if (!trimmed) {
        return { kind: "error", message: "Say something first." };
      }
      if (intakeBusy) {
        return {
          kind: "error",
          message: "One exchange at a time - the last one is still landing.",
        };
      }
      setIntakeBusy(true);
      try {
        const request = buildIntakeRequest(trimmed, {
          ids: {
            requestId: newId(),
            idempotencyKey: newId(),
            correlationId: newId(),
          },
          attachToDelegationId: attachToDelegationId ?? null,
        });
        const proof = await signExecassLocalOwnerIntake(request);
        const outcome = await execassIntake(settings, request, proof);
        if (outcome.kind === "conversational") {
          return { kind: "conversational", text: outcome.response_text };
        }
        await refreshSummary();
        return { kind: "delegation", delegation: outcome.delegation };
      } catch (error: unknown) {
        return { kind: "error", message: safeErrorMessage(error) };
      } finally {
        setIntakeBusy(false);
      }
    },
    [intakeBusy, refreshSummary, settings],
  );

  const loadDelegationDetail = useCallback(
    async (delegationId: string): Promise<DelegationDetail> => {
      const response = await getExecassDelegation(settings, delegationId);
      return response.detail;
    },
    [settings],
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
    policyInvalidationGeneration,
    integrityFailure,
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
    converse,
    loadDelegationDetail,
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
