/**
 * Durable ExecAss websocket stream state.
 *
 * Implements the live-update contract: deduplicate by duplicate_identity,
 * order by global_sequence, persist only the last durably handled cursor,
 * and on summary_refetch_required discard speculative changes and resume
 * with the frame's exact consumer_cursor (never requested_cursor or
 * head_global_sequence). In-memory websocket counters are not authoritative.
 */

import type {
  DurableEventEnvelope,
  ExecassResumeFrame,
  ExecassWsFrame,
} from "./types";

const CURSOR_STORAGE_PREFIX = "mc-execass-cursor-v1:";
const DEDUPE_CAP = 500;

export interface StreamState {
  /** Last durably handled global_sequence. */
  cursor: number;
  /** Bounded recent duplicate identities, oldest first. */
  seenDuplicateIdentities: string[];
  refetchRequired: boolean;
  /** consumer_cursor supplied by the refetch frame; resume with exactly this. */
  resumeCursor: number | null;
}

export type StreamEffect =
  | { kind: "apply-event"; envelope: DurableEventEnvelope }
  | { kind: "refetch-summary" }
  | { kind: "ignore" };

export function initialStreamState(savedCursor: number): StreamState {
  return {
    cursor: Number.isFinite(savedCursor) && savedCursor > 0 ? savedCursor : 0,
    seenDuplicateIdentities: [],
    refetchRequired: false,
    resumeCursor: null,
  };
}

export function reduceFrame(
  state: StreamState,
  frame: ExecassWsFrame,
): { state: StreamState; effect: StreamEffect } {
  if (frame.type === "execass.v1.summary_refetch_required") {
    return {
      state: {
        ...state,
        refetchRequired: true,
        resumeCursor: frame.consumer_cursor,
      },
      effect: { kind: "refetch-summary" },
    };
  }

  if (state.refetchRequired) {
    // Between a refetch demand and the new resume, nothing is trustworthy.
    return { state, effect: { kind: "ignore" } };
  }

  const envelope = frame.event;
  if (envelope.global_sequence <= state.cursor) {
    return { state, effect: { kind: "ignore" } };
  }
  if (state.seenDuplicateIdentities.includes(envelope.duplicate_identity)) {
    return { state, effect: { kind: "ignore" } };
  }

  const seen = [...state.seenDuplicateIdentities, envelope.duplicate_identity];
  if (seen.length > DEDUPE_CAP) {
    seen.splice(0, seen.length - DEDUPE_CAP);
  }
  return {
    state: {
      ...state,
      cursor: envelope.global_sequence,
      seenDuplicateIdentities: seen,
    },
    effect: { kind: "apply-event", envelope },
  };
}

export function resumeAfterRefetch(state: StreamState): StreamState {
  return {
    ...state,
    cursor: state.resumeCursor ?? state.cursor,
    refetchRequired: false,
    resumeCursor: null,
    seenDuplicateIdentities: [],
  };
}

export function buildResumeFrame(
  clientId: string,
  cursor: number,
): ExecassResumeFrame {
  return {
    type: "execass.v1.resume",
    client_id: clientId,
    cursor,
  };
}

export function loadStreamCursor(
  clientId: string,
  storage: Storage = localStorage,
): number {
  try {
    const raw = storage.getItem(`${CURSOR_STORAGE_PREFIX}${clientId}`);
    if (!raw) return 0;
    const parsed = Number(raw);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
  } catch {
    return 0;
  }
}

export function saveStreamCursor(
  clientId: string,
  cursor: number,
  storage: Storage = localStorage,
): void {
  try {
    storage.setItem(`${CURSOR_STORAGE_PREFIX}${clientId}`, String(cursor));
  } catch {
    // Cursor persistence is best-effort; a lost cursor only causes a replay.
  }
}

export type InvalidationTarget =
  | "summary"
  | "delegation-detail"
  | "policy"
  | "runtime-host"
  | "stop-all"
  | "notifications"
  | "integrity";

/**
 * Events are invalidation/reconciliation signals only - the summary and
 * detail endpoints stay authoritative. This maps each event family to the
 * surfaces that must refetch.
 */
export function invalidationTargets(
  envelope: DurableEventEnvelope,
): InvalidationTarget[] {
  switch (envelope.event_name) {
    case "execass.v1.summary.changed":
      return ["summary"];
    case "execass.v1.delegation.transitioned":
    case "execass.v1.decision.recorded":
    case "execass.v1.continuation.claimed_or_result_recorded":
    case "execass.v1.recovery.updated":
    case "execass.v1.completion.assessed":
      return ["summary", "delegation-detail"];
    case "execass.v1.policy.changed":
      return ["policy"];
    case "execass.v1.runtime_host.changed":
      return ["runtime-host", "summary"];
    case "execass.v1.global_stop.changed":
      return ["stop-all", "summary"];
    case "execass.v1.receipt.integrity_failed":
      return ["integrity", "summary"];
    case "execass.v1.notification.scheduled":
      return ["notifications"];
  }
}
