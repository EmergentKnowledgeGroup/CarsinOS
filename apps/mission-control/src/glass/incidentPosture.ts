/**
 * Pure incident-posture composition over already-authoritative facts.
 *
 * This module is the one automatic fire-alarm authority for the Glass
 * shell. It accepts only loaded/unknown facts that existing controllers
 * already own — it never consumes raw events, severity copy, clocks, or
 * inferred backend health — and returns calm, unknown, or exactly one
 * prioritized incident with plain copy and one stable-room destination.
 *
 * Priority when several authoritative problems coexist (highest first):
 * receipt integrity, runtime-host fault/recovery, delegation recovery,
 * gateway connection failure, open core breakers, faulted plugins, then
 * failed/partial work that explicitly needs the owner. One cause, one
 * alarm — duplicate alarms for the same cause are never stacked.
 */

import type {
  AttentionKind,
  DelegationPhase,
  RuntimeHostActualState,
} from "./execass/types";

export interface IncidentConnectionFacts {
  /** Gateway URL plus token are configured; nothing is knowable before this. */
  configured: boolean;
  /** Health-check truth from the one connection controller: idle/checking/up/down. */
  healthState: string;
  /** Websocket lifecycle truth: idle/connecting/connected/reconnecting/error. */
  wsState: string;
}

export interface IncidentOperationsFacts {
  /** True once the mission-control read models have loaded at least once. */
  loaded: boolean;
  openCoreBreakers: { scope: string; targetId: string }[];
  openPluginBreakers: { pluginId: string }[];
  /** Scheduler posture is context, never a trigger: idle is not an incident. */
  schedulerRunning: boolean;
  jobsDue: number;
}

export interface IncidentAttentionFact {
  attentionId: string;
  kind: AttentionKind;
  scopeKind: "delegation" | "runtime_host";
  delegationId: string | null;
  runtimeActualState: RuntimeHostActualState | null;
  reason: string;
}

export interface IncidentDelegationFact {
  delegationId: string;
  phase: DelegationPhase;
}

export interface IncidentExecassFacts {
  /** True once the authoritative summary projection has loaded at least once. */
  summaryLoaded: boolean;
  /**
   * Owner-requested global stop truth; null while unloaded. An engaged stop
   * is an explicit non-incident and never gates other authoritative facts.
   */
  stopAllEngaged: boolean | null;
  needsYou: IncidentAttentionFact[];
  /** Delegation phases from the summary's in-motion and done projections. */
  delegations: IncidentDelegationFact[];
  /** Latest uncleared receipt integrity quarantine from read or durable stream. */
  integrityFailure: { summary: string; sequence: number | null } | null;
}

export interface IncidentPostureFacts {
  connection: IncidentConnectionFacts;
  operations: IncidentOperationsFacts;
  execass: IncidentExecassFacts;
}

export type IncidentCause =
  | "receipt-integrity"
  | "runtime-host"
  | "execass-recovery"
  | "connection"
  | "core-breaker"
  | "plugin-breaker"
  | "owner-attention";

export interface IncidentTarget {
  /** Stable room id from the elevator registry — never a display label. */
  roomId: string;
  /** Human name for the walk-there action. */
  label: string;
}

export interface ComposedIncident {
  posture: "incident";
  cause: IncidentCause;
  /**
   * Stable identity for this exact cause. The same key never raises a
   * second alarm; a different key is a new problem and may re-raise.
   */
  causeKey: string;
  /** Plain-language statement of the problem. */
  message: string;
  /** Authoritative supporting fact (server reason or safe summary). */
  detail: string | null;
  target: IncidentTarget;
}

export type IncidentPosture =
  | { posture: "unknown" }
  | { posture: "calm" }
  | ComposedIncident;

const DESK_TARGET: IncidentTarget = { roomId: "desk", label: "Office desk" };
const SETUP_TARGET: IncidentTarget = { roomId: "setup", label: "Setup" };
const BREAKERS_TARGET: IncidentTarget = {
  roomId: "breakers",
  label: "Breakers & Scheduler",
};

/** Failed/partial phases whose attention items mean the owner must step in. */
const NEEDS_INTERVENTION_PHASES: readonly DelegationPhase[] = [
  "failed",
  "partially_completed",
];

function runtimeHostIncident(
  needsYou: readonly IncidentAttentionFact[],
): ComposedIncident | null {
  for (const item of needsYou) {
    if (item.scopeKind !== "runtime_host") {
      continue;
    }
    const faulted = item.runtimeActualState === "faulted";
    const recovery = item.kind === "recovery_choice";
    // An intentional pause is an explicit non-incident unless the host is
    // actually faulted — the authoritative state outranks the pause label.
    if (!faulted && !recovery) {
      continue;
    }
    return {
      posture: "incident",
      cause: "runtime-host",
      causeKey: `runtime-host:${item.attentionId}`,
      message: faulted
        ? "The runtime host reported a fault and needs recovery attention."
        : "The runtime host needs a recovery decision from you.",
      detail: item.reason || null,
      target: DESK_TARGET,
    };
  }
  return null;
}

function delegationRecoveryIncident(
  needsYou: readonly IncidentAttentionFact[],
): ComposedIncident | null {
  for (const item of needsYou) {
    if (item.scopeKind !== "delegation" || item.kind !== "recovery_choice") {
      continue;
    }
    return {
      posture: "incident",
      cause: "execass-recovery",
      causeKey: `execass-recovery:${item.attentionId}`,
      message: "Recovery needs your decision before that work can continue.",
      detail: item.reason || null,
      target: DESK_TARGET,
    };
  }
  return null;
}

function connectionIncident(
  connection: IncidentConnectionFacts,
): ComposedIncident | null {
  if (!connection.configured) {
    return null;
  }
  const healthDown = connection.healthState === "down";
  const wsFailed = connection.wsState === "error";
  if (!healthDown && !wsFailed) {
    return null;
  }
  return {
    posture: "incident",
    cause: "connection",
    causeKey: "connection:gateway",
    message: "CarsinOS cannot reach the gateway right now.",
    detail: healthDown
      ? "The gateway health check is failing."
      : "The live event connection ended in an error.",
    target: SETUP_TARGET,
  };
}

function coreBreakerIncident(
  operations: IncidentOperationsFacts,
): ComposedIncident | null {
  if (!operations.loaded || operations.openCoreBreakers.length === 0) {
    return null;
  }
  const names = operations.openCoreBreakers
    .map((breaker) => `${breaker.scope}:${breaker.targetId}`)
    .sort((a, b) => a.localeCompare(b));
  const count = names.length;
  return {
    posture: "incident",
    cause: "core-breaker",
    causeKey: `core-breaker:${names.join("|")}`,
    message:
      count === 1
        ? "A circuit breaker is open, so part of the system is paused."
        : `${count} circuit breakers are open, so parts of the system are paused.`,
    detail: `Open: ${names.join(", ")}.`,
    target: BREAKERS_TARGET,
  };
}

function pluginBreakerIncident(
  operations: IncidentOperationsFacts,
): ComposedIncident | null {
  if (!operations.loaded || operations.openPluginBreakers.length === 0) {
    return null;
  }
  const names = operations.openPluginBreakers
    .map((breaker) => breaker.pluginId)
    .sort((a, b) => a.localeCompare(b));
  const count = names.length;
  return {
    posture: "incident",
    cause: "plugin-breaker",
    causeKey: `plugin-breaker:${names.join("|")}`,
    message:
      count === 1
        ? "A plugin faulted and is paused."
        : `${count} plugins faulted and are paused.`,
    detail: `Faulted: ${names.join(", ")}.`,
    target: BREAKERS_TARGET,
  };
}

function ownerAttentionIncident(
  execass: IncidentExecassFacts,
): ComposedIncident | null {
  const phases = new Map(
    execass.delegations.map((item) => [item.delegationId, item.phase]),
  );
  for (const item of execass.needsYou) {
    if (item.scopeKind !== "delegation" || item.delegationId === null) {
      continue;
    }
    const phase = phases.get(item.delegationId);
    if (phase === undefined || !NEEDS_INTERVENTION_PHASES.includes(phase)) {
      continue;
    }
    return {
      posture: "incident",
      cause: "owner-attention",
      causeKey: `owner-attention:${item.attentionId}`,
      message: "Work finished incomplete and needs your decision.",
      detail: item.reason || null,
      target: DESK_TARGET,
    };
  }
  return null;
}

export function composeIncidentPosture(
  facts: IncidentPostureFacts,
): IncidentPosture {
  const { connection, operations, execass } = facts;

  if (execass.integrityFailure !== null) {
    return {
      posture: "incident",
      cause: "receipt-integrity",
      causeKey: `receipt-integrity:${execass.integrityFailure.sequence ?? "current"}`,
      message:
        "A work receipt failed its integrity check and needs your review.",
      detail: execass.integrityFailure.summary || null,
      target: DESK_TARGET,
    };
  }

  const incident =
    runtimeHostIncident(execass.needsYou) ??
    delegationRecoveryIncident(execass.needsYou) ??
    connectionIncident(connection) ??
    coreBreakerIncident(operations) ??
    pluginBreakerIncident(operations) ??
    ownerAttentionIncident(execass);
  if (incident) {
    return incident;
  }

  // No authoritative trouble. Calm may only be claimed when every trigger
  // source has actually loaded; unknown sources are neither healthy nor
  // alarming. Stop-all is a non-trigger, so its absence never blocks calm.
  const everythingKnown =
    connection.configured && operations.loaded && execass.summaryLoaded;
  return everythingKnown ? { posture: "calm" } : { posture: "unknown" };
}

// ————————————————————————————————————— automatic incident-mode policy

/**
 * Presentation-mode policy over the composed posture. The operator toggle
 * is an explicit override: "off" suppresses exactly the suppressed cause,
 * "on" holds the mode through calm. Everything here is pure data so the
 * shell effect stays a one-line application of the decision.
 */
export interface IncidentModeAutoState {
  override: "on" | "off" | null;
  /** causeKey the operator explicitly silenced; a new cause outranks it. */
  suppressedCauseKey: string | null;
  /** causeKey already announced, so one cause never toasts twice. */
  announcedCauseKey: string | null;
}

export const INITIAL_INCIDENT_MODE_AUTO_STATE: IncidentModeAutoState = {
  override: null,
  suppressedCauseKey: null,
  announcedCauseKey: null,
};

export interface IncidentModeDecision {
  /** New incident-mode value, or null to leave the mode untouched. */
  incidentMode: boolean | null;
  nextState: IncidentModeAutoState;
  announce: { kind: "raised" | "cleared"; message: string } | null;
}

export function applyOperatorIncidentToggle(args: {
  next: boolean;
  posture: IncidentPosture;
  state: IncidentModeAutoState;
}): IncidentModeDecision {
  const { next, posture, state } = args;
  if (next) {
    return {
      incidentMode: true,
      nextState: { ...state, override: "on", suppressedCauseKey: null },
      announce: null,
    };
  }
  return {
    incidentMode: false,
    nextState: {
      ...state,
      override: "off",
      suppressedCauseKey:
        posture.posture === "incident" ? posture.causeKey : null,
    },
    announce: null,
  };
}

export function decideAutomaticIncidentMode(args: {
  posture: IncidentPosture;
  autoEnabled: boolean;
  currentMode: boolean;
  state: IncidentModeAutoState;
}): IncidentModeDecision {
  const { posture, autoEnabled, currentMode, state } = args;
  const noChange: IncidentModeDecision = {
    incidentMode: null,
    nextState: state,
    announce: null,
  };
  if (!autoEnabled) {
    return noChange;
  }
  if (posture.posture === "unknown") {
    // Unknown sources are neither healthy nor alarming; hold the mode.
    return noChange;
  }
  if (posture.posture === "incident") {
    if (state.override === "on") {
      return noChange;
    }
    if (
      state.override === "off" &&
      state.suppressedCauseKey === posture.causeKey
    ) {
      return noChange;
    }
    const announce =
      state.announcedCauseKey === posture.causeKey
        ? null
        : ({ kind: "raised", message: posture.message } as const);
    return {
      incidentMode: currentMode ? null : true,
      nextState: {
        override: null,
        suppressedCauseKey: null,
        announcedCauseKey: posture.causeKey,
      },
      announce,
    };
  }
  // Calm: authoritative trouble cleared. Drop any off-override so the
  // toggle returns to automatic, and keep an explicit on-override.
  if (state.override === "on") {
    return {
      incidentMode: null,
      nextState: { ...state, suppressedCauseKey: null, announcedCauseKey: null },
      announce: null,
    };
  }
  const cleared: IncidentModeAutoState = {
    override: null,
    suppressedCauseKey: null,
    announcedCauseKey: null,
  };
  if (currentMode) {
    return {
      incidentMode: false,
      nextState: cleared,
      announce: {
        kind: "cleared",
        message: "Authoritative trouble cleared — back to calm.",
      },
    };
  }
  return { incidentMode: null, nextState: cleared, announce: null };
}
