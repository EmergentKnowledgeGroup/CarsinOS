/**
 * The one App-owned ExecAss policy controller.
 *
 * Reads are bound to the configured gateway/auth identity and to a read
 * generation: an identity change invalidates displayed facts on the
 * transition render itself, and any in-flight response from an older
 * generation is discarded rather than replacing newer truth. Websocket
 * policy invalidations arrive as a generation counter from the one durable
 * Office stream consumer - this controller never opens a second socket.
 *
 * Updates hold one synchronous lock, carry the complete intended
 * profile/ruleset at the authoritative current revision with a native owner
 * proof, and never claim success, advance the revision, or clear the
 * owner's draft before the authoritative response. Failures and revision
 * conflicts retain the draft; conflicts also refetch authoritative truth.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import {
  ExecassApiError,
  getExecassPolicy,
  updateExecassPolicy,
} from "../../glass/execass/api";
import { buildPolicyUpdateAuthorizationBinding } from "../../glass/execass/policyActions";
import type {
  AutonomyProfile,
  PolicyResponse,
  PolicyRule,
} from "../../glass/execass/types";
import { signExecassLocalOwnerMutation } from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";

export type PolicyPhase = "never-loaded" | "loading" | "loaded" | "error";

export interface PolicyDraft {
  /** null only while the fresh-root bootstrap has no owner profile yet. */
  profile: AutonomyProfile | null;
  rules: PolicyRule[];
  changeSummary: string;
  /** The authoritative revision this draft was seeded from. */
  baseRevision: number;
  /** Baseline used to distinguish owner edits from concurrent policy edits. */
  baseProfile: AutonomyProfile | null;
  baseRules: PolicyRule[];
}

export interface ExecassPolicyControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  /** Changes whenever the secure token identity is replaced or cleared. */
  authIdentityGeneration: number;
  active: boolean;
  /** Policy invalidation counter from the one durable Office stream. */
  policyInvalidationGeneration: number;
  setNotice: (
    notice: { tone: "info" | "error" | "critical"; message: string } | null,
  ) => void;
}

export type PolicyApplyOutcome =
  | { ok: true; message: string }
  | { ok: false; message: string };

export interface ExecassPolicyController {
  phase: PolicyPhase;
  policy: PolicyResponse | null;
  error: string | null;
  draft: PolicyDraft | null;
  /** True when authoritative truth moved beneath a seeded draft. */
  conflict: boolean;
  updateBusy: boolean;
  refresh: () => Promise<void>;
  beginDraft: () => void;
  setDraftProfile: (profile: AutonomyProfile) => void;
  setDraftRule: (index: number, rule: PolicyRule) => void;
  setDraftChangeSummary: (text: string) => void;
  discardDraft: () => void;
  /** Reapply only owner-edited fields onto the latest complete ruleset. */
  reconcileDraft: () => PolicyApplyOutcome;
  applyDraft: () => Promise<PolicyApplyOutcome>;
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

function cloneRules(rules: readonly PolicyRule[]): PolicyRule[] {
  return rules.map((rule) => ({
    ...rule,
    technical_resource_quotas: rule.technical_resource_quotas.map((quota) => ({
      ...quota,
    })),
  }));
}

export function useExecassPolicyController(
  options: ExecassPolicyControllerOptions,
): ExecassPolicyController {
  const {
    settings,
    tokenConfigured,
    authIdentityGeneration,
    active,
    policyInvalidationGeneration,
    setNotice,
  } = options;

  const gatewayUrl = settings.gateway_url.trim();
  const enabled = tokenConfigured && gatewayUrl.length > 0;
  const identity = `${gatewayUrl}|${tokenConfigured ? "token" : "no-token"}|auth:${authIdentityGeneration}`;

  const [phase, setPhase] = useState<PolicyPhase>("never-loaded");
  const [policy, setPolicy] = useState<PolicyResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<PolicyDraft | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [renderedIdentity, setRenderedIdentity] = useState(identity);

  const readGenerationRef = useRef(0);
  // Identity changes are the only thing that invalidates an in-flight
  // update. A read superseding another read (readGenerationRef) must not:
  // the gateway echoes policy.changed for the update itself, and that
  // racing refetch is expected, not a stale scope.
  const identityGenerationRef = useRef(0);
  const applyLockRef = useRef(false);

  // Displayed facts are identity-bound work: invalidate them on the
  // transition render itself, before any replacement read begins.
  if (renderedIdentity !== identity) {
    setRenderedIdentity(identity);
    readGenerationRef.current += 1;
    identityGenerationRef.current += 1;
    setPolicy(null);
    setError(null);
    setDraft(null);
    setPhase(enabled ? "loading" : "never-loaded");
  }

  const loadPolicy = useCallback(async (): Promise<void> => {
    if (!enabled) {
      return;
    }
    readGenerationRef.current += 1;
    const generation = readGenerationRef.current;
    setPhase((current) => (current === "loaded" ? "loaded" : "loading"));
    try {
      const loaded = await getExecassPolicy(settings);
      if (readGenerationRef.current !== generation) {
        return;
      }
      setPolicy(loaded);
      setError(null);
      setPhase("loaded");
    } catch (caught: unknown) {
      if (readGenerationRef.current !== generation) {
        return;
      }
      setError(safeErrorMessage(caught));
      setPhase((current) => (current === "loaded" ? "loaded" : "error"));
    }
  }, [enabled, settings]);

  const loadPolicyRef = useRef(loadPolicy);
  loadPolicyRef.current = loadPolicy;

  useEffect(() => {
    if (!active || !enabled) {
      return;
    }
    void loadPolicyRef.current();
    // The identity string - not the settings object - names the read scope.
  }, [active, enabled, identity]);

  const lastInvalidationRef = useRef(policyInvalidationGeneration);
  useEffect(() => {
    if (policyInvalidationGeneration === lastInvalidationRef.current) {
      return;
    }
    lastInvalidationRef.current = policyInvalidationGeneration;
    if (!active || !enabled) {
      return;
    }
    void loadPolicyRef.current();
  }, [active, enabled, policyInvalidationGeneration]);

  const refresh = useCallback(async () => {
    await loadPolicy();
  }, [loadPolicy]);

  const beginDraft = useCallback(() => {
    setDraft((current) => {
      if (current) {
        return current;
      }
      if (!policy) {
        return null;
      }
      return {
        profile: policy.profile ?? null,
        rules: cloneRules(policy.rules),
        changeSummary: "",
        baseRevision: policy.revision,
        baseProfile: policy.profile ?? null,
        baseRules: cloneRules(policy.rules),
      };
    });
  }, [policy]);

  const setDraftProfile = useCallback((profile: AutonomyProfile) => {
    setDraft((current) => (current ? { ...current, profile } : current));
  }, []);

  const setDraftRule = useCallback((index: number, rule: PolicyRule) => {
    setDraft((current) => {
      if (!current || index < 0 || index >= current.rules.length) {
        return current;
      }
      const rules = current.rules.slice();
      rules[index] = rule;
      return { ...current, rules };
    });
  }, []);

  const setDraftChangeSummary = useCallback((text: string) => {
    setDraft((current) =>
      current ? { ...current, changeSummary: text } : current,
    );
  }, []);

  const discardDraft = useCallback(() => {
    setDraft(null);
  }, []);

  const draftRef = useRef(draft);
  draftRef.current = draft;
  const policyRef = useRef(policy);
  policyRef.current = policy;

  const reconcileDraft = useCallback((): PolicyApplyOutcome => {
    const currentDraft = draftRef.current;
    const currentPolicy = policyRef.current;
    if (!currentDraft || !currentPolicy) {
      return { ok: false, message: "There is no policy draft to reconcile." };
    }
    if (currentDraft.baseRevision === currentPolicy.revision) {
      return { ok: true, message: "The draft already uses the latest policy." };
    }

    const draftRules = new Map(
      currentDraft.rules.map((rule) => [rule.rule_id, rule] as const),
    );
    const latestRuleIds = new Set(
      currentPolicy.rules.map((rule) => rule.rule_id),
    );
    for (const baseRule of currentDraft.baseRules) {
      const editedRule = draftRules.get(baseRule.rule_id);
      if (
        editedRule &&
        !latestRuleIds.has(baseRule.rule_id) &&
        (editedRule.parallelism_limit !== baseRule.parallelism_limit ||
          editedRule.recovery_limit !== baseRule.recovery_limit)
      ) {
        const message =
          "A rule you edited was removed by the newer policy. Your draft is still kept; discard it only after reviewing the latest rules.";
        setNotice({ tone: "error", message });
        return { ok: false, message };
      }
    }

    const baseRules = new Map(
      currentDraft.baseRules.map((rule) => [rule.rule_id, rule] as const),
    );
    const rebasedRules = cloneRules(currentPolicy.rules).map((latestRule) => {
      const baseRule = baseRules.get(latestRule.rule_id);
      const editedRule = draftRules.get(latestRule.rule_id);
      if (!baseRule || !editedRule) {
        return latestRule;
      }
      return {
        ...latestRule,
        parallelism_limit:
          editedRule.parallelism_limit !== baseRule.parallelism_limit
            ? editedRule.parallelism_limit
            : latestRule.parallelism_limit,
        recovery_limit:
          editedRule.recovery_limit !== baseRule.recovery_limit
            ? editedRule.recovery_limit
            : latestRule.recovery_limit,
      };
    });
    const ownerChangedProfile =
      currentDraft.profile !== currentDraft.baseProfile;
    setDraft({
      profile: ownerChangedProfile
        ? currentDraft.profile
        : (currentPolicy.profile ?? null),
      rules: rebasedRules,
      changeSummary: currentDraft.changeSummary,
      baseRevision: currentPolicy.revision,
      baseProfile: currentPolicy.profile ?? null,
      baseRules: cloneRules(currentPolicy.rules),
    });
    return {
      ok: true,
      message:
        "Your edits were placed onto the latest policy. Review the complete change again before confirming.",
    };
  }, [setNotice]);

  const applyDraft = useCallback(async (): Promise<PolicyApplyOutcome> => {
    if (applyLockRef.current) {
      return { ok: false, message: "That policy update is already in flight." };
    }
    const currentDraft = draftRef.current;
    const currentPolicy = policyRef.current;
    if (!currentDraft || !currentPolicy) {
      return { ok: false, message: "There is no policy draft to apply." };
    }
    if (currentDraft.baseRevision !== currentPolicy.revision) {
      return {
        ok: false,
        message:
          "The policy changed while you were editing. Reconcile your draft with the latest rules before reviewing and confirming again.",
      };
    }
    if (!currentDraft.profile) {
      return { ok: false, message: "Choose an autonomy profile first." };
    }
    const changeSummary = currentDraft.changeSummary.trim();
    if (!changeSummary) {
      return {
        ok: false,
        message: "Describe the change in a sentence before applying.",
      };
    }
    applyLockRef.current = true;
    setUpdateBusy(true);
    const identityGeneration = identityGenerationRef.current;
    try {
      const request = {
        idempotency_key: newId(),
        expected_policy_revision: currentDraft.baseRevision,
        change_summary: changeSummary,
        proposed_profile: currentDraft.profile,
        proposed_rules: currentDraft.rules,
      };
      const binding = await buildPolicyUpdateAuthorizationBinding(request, {
        now: Date.now(),
        correlationId: newId(),
      });
      const proof = await signExecassLocalOwnerMutation(binding);
      const response = await updateExecassPolicy(settings, request, {
        binding,
        proof,
      });
      if (identityGenerationRef.current !== identityGeneration) {
        // The identity changed while the update was in flight; the response
        // belongs to the old scope and must not replace newer truth.
        return { ok: false, message: "The gateway identity changed." };
      }
      // The racing invalidation refetch may already hold newer truth than
      // this response; never regress the authoritative revision.
      setPolicy((current) =>
        current && current.revision > response.policy.revision
          ? current
          : response.policy,
      );
      setError(null);
      setPhase("loaded");
      setDraft(null);
      const message = "Policy updated - the new ground rules are in effect.";
      setNotice({ tone: "info", message });
      return { ok: true, message };
    } catch (caught: unknown) {
      if (identityGenerationRef.current !== identityGeneration) {
        return { ok: false, message: "The gateway identity changed." };
      }
      const message = safeErrorMessage(caught);
      if (
        caught instanceof ExecassApiError &&
        caught.isRevisionOrIdempotencyConflict
      ) {
        setNotice({ tone: "info", message });
        void loadPolicyRef.current();
      } else {
        setNotice({ tone: "error", message });
      }
      return { ok: false, message };
    } finally {
      applyLockRef.current = false;
      setUpdateBusy(false);
    }
  }, [setNotice, settings]);

  const conflict =
    draft !== null && policy !== null && draft.baseRevision !== policy.revision;

  return {
    phase,
    policy,
    error,
    draft,
    conflict,
    updateBusy,
    refresh,
    beginDraft,
    setDraftProfile,
    setDraftRule,
    setDraftChangeSummary,
    discardDraft,
    reconcileDraft,
    applyDraft,
  };
}
