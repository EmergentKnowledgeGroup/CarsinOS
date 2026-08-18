import { describe, expect, it } from "vitest";

import {
  INITIAL_INCIDENT_MODE_AUTO_STATE,
  applyOperatorIncidentToggle,
  composeIncidentPosture,
  decideAutomaticIncidentMode,
  type IncidentModeAutoState,
  type IncidentPosture,
  type IncidentPostureFacts,
} from "./incidentPosture";

/**
 * Truth table for the pure incident-posture composer.
 *
 * Every positive trigger and every negative boundary from the P6 slice is
 * proven separately. The composer accepts only already-authoritative
 * loaded/unknown facts; it never consumes raw events, severity copy, or
 * clocks.
 */

function calmFacts(): IncidentPostureFacts {
  return {
    connection: {
      configured: true,
      healthState: "up",
      wsState: "connected",
    },
    operations: {
      loaded: true,
      openCoreBreakers: [],
      openPluginBreakers: [],
      schedulerRunning: true,
      jobsDue: 0,
    },
    execass: {
      summaryLoaded: true,
      stopAllEngaged: false,
      needsYou: [],
      delegations: [],
      integrityFailure: null,
    },
  };
}

function attention(
  overrides: Partial<IncidentPostureFacts["execass"]["needsYou"][number]> = {},
): IncidentPostureFacts["execass"]["needsYou"][number] {
  return {
    attentionId: "att-1",
    kind: "confirmation",
    scopeKind: "delegation",
    delegationId: "dlg-1",
    runtimeActualState: null,
    reason: "Confirm before continuing.",
    ...overrides,
  };
}

describe("composeIncidentPosture positive triggers", () => {
  it("raises a receipt-integrity incident targeting the Office desk", () => {
    const facts = calmFacts();
    facts.execass.integrityFailure = {
      summary: "Receipt chain verification failed for delegation dlg-9.",
      sequence: 41,
    };
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("receipt-integrity");
    expect(posture.target.roomId).toBe("desk");
    expect(posture.causeKey).toBe("receipt-integrity:41");
    expect(posture.message.toLowerCase()).toContain("receipt");
    expect(posture.detail).toBe(
      "Receipt chain verification failed for delegation dlg-9.",
    );
  });

  it("uses a stable current-quarantine key when discovery has no stream sequence", () => {
    const facts = calmFacts();
    facts.execass.integrityFailure = {
      summary: "Receipt integrity is quarantined.",
      sequence: null,
    };
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.causeKey).toBe("receipt-integrity:current");
  });

  it("raises a runtime-host incident when the runtime host is faulted", () => {
    const facts = calmFacts();
    facts.execass.needsYou = [
      attention({
        attentionId: "att-rt",
        kind: "reply",
        scopeKind: "runtime_host",
        delegationId: null,
        runtimeActualState: "faulted",
        reason: "The runtime host stopped unexpectedly.",
      }),
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("runtime-host");
    expect(posture.target.roomId).toBe("desk");
    expect(posture.causeKey).toBe("runtime-host:att-rt");
    expect(posture.detail).toBe("The runtime host stopped unexpectedly.");
  });

  it("raises a runtime-host incident for runtime recovery attention", () => {
    const facts = calmFacts();
    facts.execass.needsYou = [
      attention({
        attentionId: "att-rtrec",
        kind: "recovery_choice",
        scopeKind: "runtime_host",
        delegationId: null,
        runtimeActualState: "stopped",
        reason: "Choose how the runtime should recover.",
      }),
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("runtime-host");
  });

  it("raises an execass-recovery incident for delegation recovery attention", () => {
    const facts = calmFacts();
    facts.execass.needsYou = [
      attention({
        attentionId: "att-rec",
        kind: "recovery_choice",
        reason: "Recovery needs a decision before retrying the export.",
      }),
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("execass-recovery");
    expect(posture.target.roomId).toBe("desk");
    expect(posture.causeKey).toBe("execass-recovery:att-rec");
  });

  it("raises a connection incident when the gateway health check is down", () => {
    const facts = calmFacts();
    facts.connection.healthState = "down";
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("connection");
    expect(posture.target.roomId).toBe("setup");
    expect(posture.causeKey).toBe("connection:gateway");
  });

  it("raises a connection incident when the websocket ends in error", () => {
    const facts = calmFacts();
    facts.connection.wsState = "error";
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("connection");
    expect(posture.target.roomId).toBe("setup");
  });

  it("raises a connection incident even while other sources are unloaded", () => {
    const facts = calmFacts();
    facts.connection.healthState = "down";
    facts.operations.loaded = false;
    facts.execass.summaryLoaded = false;
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("connection");
  });

  it("raises a core-breaker incident naming the open scope", () => {
    const facts = calmFacts();
    facts.operations.openCoreBreakers = [
      { scope: "provider", targetId: "anthropic" },
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("core-breaker");
    expect(posture.target.roomId).toBe("breakers");
    expect(posture.causeKey).toBe("core-breaker:provider:anthropic");
    expect(posture.detail).toContain("provider:anthropic");
  });

  it("merges multiple open core breakers into one deduplicated incident", () => {
    const facts = calmFacts();
    facts.operations.openCoreBreakers = [
      { scope: "provider", targetId: "openai" },
      { scope: "channel", targetId: "discord" },
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("core-breaker");
    expect(posture.causeKey).toBe(
      "core-breaker:channel:discord|provider:openai",
    );
    expect(posture.message).toContain("2");
  });

  it("raises a plugin-breaker incident targeting the breakers room", () => {
    const facts = calmFacts();
    facts.operations.openPluginBreakers = [{ pluginId: "webhook-bridge" }];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("plugin-breaker");
    expect(posture.target.roomId).toBe("breakers");
    expect(posture.causeKey).toBe("plugin-breaker:webhook-bridge");
  });

  it("raises owner-attention for a failed delegation whose projection needs a human", () => {
    const facts = calmFacts();
    facts.execass.delegations = [{ delegationId: "dlg-7", phase: "failed" }];
    facts.execass.needsYou = [
      attention({
        attentionId: "att-7",
        kind: "clarification",
        delegationId: "dlg-7",
        reason: "The export failed; choose the fallback format.",
      }),
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("owner-attention");
    expect(posture.target.roomId).toBe("desk");
    expect(posture.causeKey).toBe("owner-attention:att-7");
  });

  it("raises owner-attention for a partially completed delegation with attention", () => {
    const facts = calmFacts();
    facts.execass.delegations = [
      { delegationId: "dlg-8", phase: "partially_completed" },
    ];
    facts.execass.needsYou = [
      attention({
        attentionId: "att-8",
        kind: "confirmation",
        delegationId: "dlg-8",
        reason: "Two of five uploads did not finish.",
      }),
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("owner-attention");
  });
});

describe("composeIncidentPosture negative boundaries", () => {
  it("stays calm for ordinary waiting_external work", () => {
    const facts = calmFacts();
    facts.execass.delegations = [
      { delegationId: "dlg-w", phase: "waiting_external" },
    ];
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("stays calm during a user-requested global stop", () => {
    const facts = calmFacts();
    facts.execass.stopAllEngaged = true;
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("stays calm for an intentionally paused runtime", () => {
    const facts = calmFacts();
    facts.execass.needsYou = [
      attention({
        attentionId: "att-pause",
        kind: "runtime_paused",
        scopeKind: "runtime_host",
        delegationId: null,
        runtimeActualState: "stopped",
        reason: "You paused the runtime.",
      }),
    ];
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("still alarms when a paused runtime is actually faulted", () => {
    const facts = calmFacts();
    facts.execass.needsYou = [
      attention({
        attentionId: "att-pf",
        kind: "runtime_paused",
        scopeKind: "runtime_host",
        delegationId: null,
        runtimeActualState: "faulted",
        reason: "The runtime host faulted while paused.",
      }),
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("runtime-host");
  });

  it("stays calm when the scheduler is idle with no due work", () => {
    const facts = calmFacts();
    facts.operations.schedulerRunning = false;
    facts.operations.jobsDue = 0;
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("stays calm when the scheduler simply has due work", () => {
    const facts = calmFacts();
    facts.operations.jobsDue = 12;
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("stays calm for a failed delegation with no intervention fact", () => {
    const facts = calmFacts();
    facts.execass.delegations = [{ delegationId: "dlg-f", phase: "failed" }];
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("stays calm for ordinary confirmation attention on healthy work", () => {
    const facts = calmFacts();
    facts.execass.delegations = [{ delegationId: "dlg-1", phase: "in_motion" }];
    facts.execass.needsYou = [attention()];
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("stays calm for reply attention on completed work", () => {
    const facts = calmFacts();
    facts.execass.delegations = [{ delegationId: "dlg-c", phase: "completed" }];
    facts.execass.needsYou = [
      attention({ attentionId: "att-c", kind: "reply", delegationId: "dlg-c" }),
    ];
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("stays calm through transient connecting and reconnecting states", () => {
    const facts = calmFacts();
    facts.connection.healthState = "checking";
    facts.connection.wsState = "reconnecting";
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });
});

describe("composeIncidentPosture unknown truth", () => {
  it("is unknown when nothing is configured, even with a websocket error", () => {
    const facts = calmFacts();
    facts.connection.configured = false;
    facts.connection.wsState = "error";
    facts.connection.healthState = "idle";
    facts.operations.loaded = false;
    facts.execass.summaryLoaded = false;
    expect(composeIncidentPosture(facts)).toEqual({ posture: "unknown" });
  });

  it("is unknown while operations facts have never loaded", () => {
    const facts = calmFacts();
    facts.operations.loaded = false;
    expect(composeIncidentPosture(facts)).toEqual({ posture: "unknown" });
  });

  it("is unknown while the ExecAss summary has never loaded", () => {
    const facts = calmFacts();
    facts.execass.summaryLoaded = false;
    expect(composeIncidentPosture(facts)).toEqual({ posture: "unknown" });
  });

  it("still reports loaded incidents while other sources are unknown", () => {
    const facts = calmFacts();
    facts.execass.summaryLoaded = false;
    facts.operations.openCoreBreakers = [
      { scope: "tool", targetId: "exec" },
    ];
    const posture = composeIncidentPosture(facts);
    expect(posture.posture).toBe("incident");
    if (posture.posture !== "incident") return;
    expect(posture.cause).toBe("core-breaker");
  });

  it("treats an unloaded stop-all status as neutral, never blocking calm", () => {
    const facts = calmFacts();
    facts.execass.stopAllEngaged = null;
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });
});

describe("composeIncidentPosture prioritization and determinism", () => {
  function everythingWrong(): IncidentPostureFacts {
    const facts = calmFacts();
    facts.execass.integrityFailure = { summary: "Chain broken.", sequence: 9 };
    facts.execass.needsYou = [
      attention({
        attentionId: "att-rt",
        kind: "reply",
        scopeKind: "runtime_host",
        delegationId: null,
        runtimeActualState: "faulted",
        reason: "Runtime fault.",
      }),
      attention({
        attentionId: "att-rec",
        kind: "recovery_choice",
        delegationId: "dlg-r",
        reason: "Recovery decision needed.",
      }),
      attention({
        attentionId: "att-own",
        kind: "confirmation",
        delegationId: "dlg-fail",
        reason: "Failed work needs you.",
      }),
    ];
    facts.execass.delegations = [
      { delegationId: "dlg-r", phase: "recovering" },
      { delegationId: "dlg-fail", phase: "failed" },
    ];
    facts.connection.healthState = "down";
    facts.operations.openCoreBreakers = [
      { scope: "provider", targetId: "anthropic" },
    ];
    facts.operations.openPluginBreakers = [{ pluginId: "bridge" }];
    return facts;
  }

  it("prioritizes integrity, runtime recovery, connection, breakers, then owner attention", () => {
    const facts = everythingWrong();
    const causes: string[] = [];
    for (let step = 0; step < 7; step += 1) {
      const posture = composeIncidentPosture(facts);
      if (posture.posture !== "incident") break;
      causes.push(posture.cause);
      switch (posture.cause) {
        case "receipt-integrity":
          facts.execass.integrityFailure = null;
          break;
        case "runtime-host":
          facts.execass.needsYou = facts.execass.needsYou.filter(
            (item) => item.scopeKind !== "runtime_host",
          );
          break;
        case "execass-recovery":
          facts.execass.needsYou = facts.execass.needsYou.filter(
            (item) => item.kind !== "recovery_choice",
          );
          break;
        case "connection":
          facts.connection.healthState = "up";
          break;
        case "core-breaker":
          facts.operations.openCoreBreakers = [];
          break;
        case "plugin-breaker":
          facts.operations.openPluginBreakers = [];
          break;
        case "owner-attention":
          facts.execass.needsYou = [];
          break;
      }
    }
    expect(causes).toEqual([
      "receipt-integrity",
      "runtime-host",
      "execass-recovery",
      "connection",
      "core-breaker",
      "plugin-breaker",
      "owner-attention",
    ]);
    expect(composeIncidentPosture(facts)).toEqual({ posture: "calm" });
  });

  it("returns exactly one incident for one cause with a stable dedupe key", () => {
    const facts = calmFacts();
    facts.execass.needsYou = [
      attention({
        attentionId: "att-rec",
        kind: "recovery_choice",
        reason: "Recovery decision needed.",
      }),
      attention({
        attentionId: "att-rec-2",
        kind: "recovery_choice",
        delegationId: "dlg-2",
        reason: "Second recovery decision.",
      }),
    ];
    const first = composeIncidentPosture(facts);
    const second = composeIncidentPosture(facts);
    expect(first).toEqual(second);
    expect(first.posture).toBe("incident");
    if (first.posture !== "incident") return;
    expect(first.causeKey).toBe("execass-recovery:att-rec");
  });

  it("is a pure function of its facts", () => {
    const facts = everythingWrong();
    const snapshot = JSON.stringify(facts);
    const first = composeIncidentPosture(facts);
    const second = composeIncidentPosture(JSON.parse(snapshot));
    expect(first).toEqual(second);
    expect(JSON.stringify(facts)).toBe(snapshot);
  });
});

describe("decideAutomaticIncidentMode", () => {
  const incident: IncidentPosture = {
    posture: "incident",
    cause: "core-breaker",
    causeKey: "core-breaker:provider:anthropic",
    message: "A circuit breaker is open, so part of the system is paused.",
    detail: "Open: provider:anthropic.",
    target: { roomId: "breakers", label: "Breakers & Scheduler" },
  };
  const calm: IncidentPosture = { posture: "calm" };
  const unknown: IncidentPosture = { posture: "unknown" };

  it("raises incident mode once for a new cause and announces it", () => {
    const first = decideAutomaticIncidentMode({
      posture: incident,
      autoEnabled: true,
      currentMode: false,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    expect(first.incidentMode).toBe(true);
    expect(first.announce).toEqual({ kind: "raised", message: incident.message });
    const second = decideAutomaticIncidentMode({
      posture: incident,
      autoEnabled: true,
      currentMode: true,
      state: first.nextState,
    });
    expect(second.incidentMode).toBeNull();
    expect(second.announce).toBeNull();
  });

  it("does nothing when the automatic trigger control is off", () => {
    const decision = decideAutomaticIncidentMode({
      posture: incident,
      autoEnabled: false,
      currentMode: false,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    expect(decision.incidentMode).toBeNull();
    expect(decision.announce).toBeNull();
    expect(decision.nextState).toEqual(INITIAL_INCIDENT_MODE_AUTO_STATE);
  });

  it("holds the current mode while posture is unknown", () => {
    const decision = decideAutomaticIncidentMode({
      posture: unknown,
      autoEnabled: true,
      currentMode: true,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    expect(decision.incidentMode).toBeNull();
    expect(decision.announce).toBeNull();
  });

  it("respects an operator off-override for the same cause only", () => {
    const toggledOff = applyOperatorIncidentToggle({
      next: false,
      posture: incident,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    expect(toggledOff.incidentMode).toBe(false);
    const suppressed = decideAutomaticIncidentMode({
      posture: incident,
      autoEnabled: true,
      currentMode: false,
      state: toggledOff.nextState,
    });
    expect(suppressed.incidentMode).toBeNull();
    const newCause: IncidentPosture = {
      ...incident,
      causeKey: "core-breaker:tool:exec",
      detail: "Open: tool:exec.",
    };
    const reRaised = decideAutomaticIncidentMode({
      posture: newCause,
      autoEnabled: true,
      currentMode: false,
      state: suppressed.nextState,
    });
    expect(reRaised.incidentMode).toBe(true);
    expect(reRaised.announce?.kind).toBe("raised");
  });

  it("clears incident mode without a click when trouble clears", () => {
    const raised = decideAutomaticIncidentMode({
      posture: incident,
      autoEnabled: true,
      currentMode: false,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    const cleared = decideAutomaticIncidentMode({
      posture: calm,
      autoEnabled: true,
      currentMode: true,
      state: raised.nextState,
    });
    expect(cleared.incidentMode).toBe(false);
    expect(cleared.announce?.kind).toBe("cleared");
    const idle = decideAutomaticIncidentMode({
      posture: calm,
      autoEnabled: true,
      currentMode: false,
      state: cleared.nextState,
    });
    expect(idle.incidentMode).toBeNull();
    expect(idle.announce).toBeNull();
  });

  it("keeps an operator on-override through calm and drops off-override on calm", () => {
    const toggledOn = applyOperatorIncidentToggle({
      next: true,
      posture: calm,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    expect(toggledOn.incidentMode).toBe(true);
    const heldOn = decideAutomaticIncidentMode({
      posture: calm,
      autoEnabled: true,
      currentMode: true,
      state: toggledOn.nextState,
    });
    expect(heldOn.incidentMode).toBeNull();

    const toggledOff = applyOperatorIncidentToggle({
      next: false,
      posture: incident,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    const calmed = decideAutomaticIncidentMode({
      posture: calm,
      autoEnabled: true,
      currentMode: false,
      state: toggledOff.nextState,
    });
    expect(calmed.nextState.override).toBeNull();
    expect(calmed.nextState.suppressedCauseKey).toBeNull();
    const relapse = decideAutomaticIncidentMode({
      posture: incident,
      autoEnabled: true,
      currentMode: false,
      state: calmed.nextState,
    });
    expect(relapse.incidentMode).toBe(true);
  });

  it("re-announces when the incident cause changes while mode is already on", () => {
    const raised = decideAutomaticIncidentMode({
      posture: incident,
      autoEnabled: true,
      currentMode: false,
      state: INITIAL_INCIDENT_MODE_AUTO_STATE,
    });
    const escalated: IncidentPosture = {
      posture: "incident",
      cause: "receipt-integrity",
      causeKey: "receipt-integrity:9",
      message: "A work receipt failed its integrity check and needs your review.",
      detail: "Chain broken.",
      target: { roomId: "desk", label: "Office desk" },
    };
    const decision = decideAutomaticIncidentMode({
      posture: escalated,
      autoEnabled: true,
      currentMode: true,
      state: raised.nextState,
    });
    expect(decision.incidentMode).toBeNull();
    expect(decision.announce).toEqual({
      kind: "raised",
      message: escalated.message,
    });
  });

  it("operator toggle state transitions are pure data", () => {
    const state: IncidentModeAutoState = {
      override: null,
      suppressedCauseKey: null,
      announcedCauseKey: null,
    };
    const on = applyOperatorIncidentToggle({ next: true, posture: incident, state });
    expect(on.nextState.override).toBe("on");
    expect(on.nextState.suppressedCauseKey).toBeNull();
    const off = applyOperatorIncidentToggle({
      next: false,
      posture: incident,
      state: on.nextState,
    });
    expect(off.nextState.override).toBe("off");
    expect(off.nextState.suppressedCauseKey).toBe(incident.causeKey);
    expect(state).toEqual({
      override: null,
      suppressedCauseKey: null,
      announcedCauseKey: null,
    });
  });
});
