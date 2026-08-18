import { useState } from "react";

import type {
  AutonomyProfile,
  PolicyRule,
  TechnicalResourceQuota,
} from "../../glass/execass/types";
import { Chip } from "../../ui/Chip";
import { Surface } from "../../ui/Surface";
import { EmptyState } from "../../ui/EmptyState";
import { PinRoomToOffice } from "../execassOffice/PinRoomToOffice";
import type { ExecassPolicyController } from "./useExecassPolicyController";

/**
 * The Basement Policy room: plain-language ExecAss autonomy settings.
 *
 * This surface explains and edits how ExecAss handles derived or unattended
 * work. It is not a forbidden-actions catalog and not a financial control:
 * exact owner directions are never policed, dangerous or destructive work
 * gets exactly one consequence confirmation, and quotas are technical
 * execution limits. One review step, then one confirmation, applies a
 * complete profile/ruleset through the owner-proof policy contract.
 */

interface ProfileCard {
  id: AutonomyProfile;
  name: string;
  tagline: string;
}

const PROFILE_CARDS: readonly ProfileCard[] = [
  {
    id: "locked_down",
    name: "Locked down",
    tagline:
      "Nothing moves on its own. Work ExecAss infers or schedules waits for your go-ahead.",
  },
  {
    id: "balanced",
    name: "Balanced",
    tagline:
      "Routine derived work proceeds. Anything unusual, wide-reaching, or irreversible comes to you first.",
  },
  {
    id: "full_send",
    name: "Full send",
    tagline:
      "Derived and unattended work runs at full speed. Dangerous actions still get their one confirmation.",
  },
  {
    id: "custom",
    name: "Custom",
    tagline: "Your own mix - tuned rule by rule in the advanced details below.",
  },
];

const QUOTA_KIND_LABEL: Record<TechnicalResourceQuota["kind"], string> = {
  tokens: "Tokens",
  time_ms: "Time (ms)",
  connector_calls: "Connector calls",
  resource_units: "Resource units",
};

function profileName(profile: AutonomyProfile | null | undefined): string {
  return (
    PROFILE_CARDS.find((card) => card.id === profile)?.name ?? "Not set yet"
  );
}

function formatCount(value: number): string {
  return value.toLocaleString("en-US");
}

/** Owner-facing labels for the bounded technical scope facts on a rule. */
const RULE_SCOPE_LABELS: ReadonlyArray<{
  key: keyof PolicyRule;
  label: string;
}> = [
  { key: "task_or_delegation_scope", label: "Tasks / delegations" },
  { key: "workspace_scope", label: "Workspaces" },
  { key: "routine_scope", label: "Routines" },
  { key: "recurring_work_scope", label: "Recurring work" },
  { key: "target_scope", label: "Targets" },
  { key: "audience_scope", label: "Audience" },
  {
    key: "connector_or_tool_identity_and_version_scope",
    label: "Connectors / tools",
  },
  { key: "clarification_sensitivity", label: "Clarification sensitivity" },
];

function RuleQuotas({ quotas }: { quotas: readonly TechnicalResourceQuota[] }) {
  if (quotas.length === 0) {
    return (
      <p className="mc-policy-quota-caption">
        No technical execution limits on this rule.
      </p>
    );
  }
  return (
    <div className="mc-policy-quotas">
      <p className="mc-policy-quota-caption">
        Technical execution limits - capacity, not money. Nothing here is a
        budget or spending control.
      </p>
      <ul className="mc-policy-quota-list">
        {quotas.map((quota) => (
          <li key={quota.kind} className="mc-policy-quota-row">
            <span className="mc-policy-quota-kind">
              {QUOTA_KIND_LABEL[quota.kind]}
            </span>
            <span className="mc-policy-quota-figures">
              {formatCount(quota.consumed)} used of {formatCount(quota.limit)}{" "}
              limit
              {quota.reserved > 0
                ? ` (${formatCount(quota.reserved)} reserved)`
                : ""}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

interface PolicyRoomPageProps {
  controller: ExecassPolicyController;
}

export function PolicyRoomPage({ controller }: PolicyRoomPageProps) {
  const [reviewOpen, setReviewOpen] = useState(false);
  const { policy, draft, phase } = controller;

  const headerRow = (
    <div className="mc-room-tabs-row mc-policy-room-row">
      <div className="mc-policy-room-status" data-testid="policy-room-status">
        {policy ? (
          <>
            <Chip
              label={
                policy.configured
                  ? `Profile: ${profileName(draft?.profile ?? policy.profile)}`
                  : "No ground rules yet"
              }
              tone={policy.configured ? "connected" : "warning"}
            />
            <Chip label={`Revision ${policy.revision}`} tone="" />
          </>
        ) : (
          <Chip label="Policy" tone="" />
        )}
        {draft ? <Chip label="Editing" tone="warning" /> : null}
      </div>
      {phase === "loaded" && policy ? (
        <PinRoomToOffice roomId="policy" />
      ) : null}
    </div>
  );

  if (phase === "never-loaded" || phase === "loading") {
    return (
      <section className="mc-policy-room" data-testid="policy-room-page">
        {headerRow}
        <Surface
          className="mc-policy-room-panel"
          title="Ground rules"
          subtitle="Checking the current ground rules with the gateway."
        >
          <div data-testid="policy-room-loading">
            <EmptyState message="Reading how your assistant is allowed to run unattended…" />
          </div>
        </Surface>
      </section>
    );
  }

  if (phase === "error" || !policy) {
    return (
      <section className="mc-policy-room" data-testid="policy-room-page">
        {headerRow}
        <Surface
          className="mc-policy-room-panel"
          title="Ground rules unavailable"
          subtitle="The current policy could not be read. Nothing was changed."
        >
          <div className="mc-policy-room-error" data-testid="policy-room-error">
            <p>{controller.error ?? "The policy could not be loaded."}</p>
            <button
              type="button"
              className="mc-btn"
              data-testid="policy-room-retry"
              onClick={() => void controller.refresh()}
            >
              Try again
            </button>
          </div>
        </Surface>
      </section>
    );
  }

  const activeProfile = draft ? draft.profile : (policy.profile ?? null);
  const rules = draft ? draft.rules : policy.rules;

  const selectProfile = (profile: AutonomyProfile) => {
    controller.beginDraft();
    controller.setDraftProfile(profile);
  };

  const setParallelism = (index: number, rule: PolicyRule, raw: string) => {
    const parsed = raw.trim() === "" ? null : Number.parseInt(raw, 10);
    if (parsed !== null && (!Number.isInteger(parsed) || parsed < 0)) {
      return;
    }
    controller.setDraftRule(index, { ...rule, parallelism_limit: parsed });
  };

  const setRecoveryLimit = (index: number, rule: PolicyRule, raw: string) => {
    const parsed = raw.trim() === "" ? null : Number.parseInt(raw, 10);
    if (parsed !== null && (!Number.isInteger(parsed) || parsed < 0)) {
      return;
    }
    controller.setDraftRule(index, { ...rule, recovery_limit: parsed });
  };

  const changeSummaryReady = Boolean(draft?.changeSummary.trim());

  return (
    <section className="mc-policy-room" data-testid="policy-room-page">
      {headerRow}
      {controller.conflict ? (
        <div
          className="mc-policy-conflict"
          data-testid="policy-conflict"
          role="alert"
        >
          <span>
            The ground rules changed while you were editing - they are now at
            revision {policy.revision}. Your edits are kept.
          </span>
          <button
            type="button"
            className="mc-btn"
            data-testid="policy-reconcile"
            onClick={() => {
              const outcome = controller.reconcileDraft();
              if (outcome.ok) {
                setReviewOpen(false);
              }
            }}
          >
            Reconcile with latest
          </button>
        </div>
      ) : null}
      {controller.error ? (
        <div className="mc-policy-stale-warning" role="status">
          {controller.error} The facts below are the last loaded truth.
        </div>
      ) : null}
      <div className="mc-policy-room-grid">
        <Surface
          className="mc-policy-room-panel mc-policy-deal"
          title="The deal"
          subtitle="What this page does and does not govern."
        >
          <p className="mc-policy-now" data-testid="policy-effective-summary">
            {policy.configured
              ? policy.effective_operational_summary
              : "Your assistant hasn't set ground rules yet. Until you choose a profile, ExecAss keeps its cautious fresh-start defaults - it will not invent a policy for you."}
          </p>
          <ul className="mc-policy-commitments">
            <li>
              <strong>Your word is the instruction.</strong> When you tell
              ExecAss to do something, it does it. Ordinary requests are never
              policed or second-guessed.
            </li>
            <li>
              <strong>Dangerous work gets exactly one confirmation.</strong>{" "}
              ExecAss states the concrete consequence, asks once, and carries
              your yes forward - no repeat warnings, no pushback afterwards.
            </li>
            <li>
              <strong>This page governs everything else</strong> - the work
              ExecAss infers, schedules, delegates, or continues without a
              fresh instruction from you.
            </li>
          </ul>
        </Surface>
        <Surface
          className="mc-policy-room-panel"
          title="Autonomy profile"
          subtitle="How much unattended work proceeds on its own."
        >
          <div className="mc-policy-profiles">
            {PROFILE_CARDS.map((card) => (
              <button
                key={card.id}
                type="button"
                className={
                  "mc-policy-profile" +
                  (activeProfile === card.id ? " is-current" : "")
                }
                data-testid={`policy-profile-${card.id}`}
                aria-pressed={activeProfile === card.id}
                onClick={() => selectProfile(card.id)}
              >
                <span className="mc-policy-profile-name">{card.name}</span>
                <span className="mc-policy-profile-tagline">
                  {card.tagline}
                </span>
              </button>
            ))}
          </div>
        </Surface>
      </div>
      <Surface
        className="mc-policy-room-panel"
        title="Advanced rules"
        subtitle="The bounded technical details behind the profile. Editing one field keeps everything else exactly as it is."
      >
        {rules.length === 0 ? (
          <EmptyState message="No advanced rules yet - the profile alone governs unattended work." />
        ) : (
          <ul className="mc-policy-rules">
            {rules.map((rule, index) => (
              <li key={rule.rule_id} className="mc-policy-rule">
                <details
                  data-testid={`policy-rule-advanced-${rule.rule_id}`}
                  className="mc-policy-rule-details"
                >
                  <summary>
                    <span className="mc-policy-rule-id">{rule.rule_id}</span>
                    <span className="mc-policy-rule-brief">
                      {rule.parallelism_limit !== null &&
                      rule.parallelism_limit !== undefined
                        ? `up to ${rule.parallelism_limit} in parallel`
                        : "no parallel cap"}
                    </span>
                  </summary>
                  <div className="mc-policy-rule-body">
                    <dl className="mc-policy-rule-scopes">
                      {RULE_SCOPE_LABELS.flatMap(({ key, label }) => {
                        const value = rule[key];
                        return typeof value === "string" && value.length > 0
                          ? [
                              <div
                                key={key}
                                className="mc-policy-rule-scope"
                              >
                                <dt>{label}</dt>
                                <dd>{value}</dd>
                              </div>,
                            ]
                          : [];
                      })}
                    </dl>
                    {draft ? (
                      <div className="mc-policy-rule-edits">
                        <label className="mc-policy-rule-edit">
                          <span>Parallel work limit</span>
                          <input
                            type="number"
                            min={0}
                            inputMode="numeric"
                            data-testid={`policy-rule-parallelism-${rule.rule_id}`}
                            value={rule.parallelism_limit ?? ""}
                            onInput={(event) =>
                              setParallelism(
                                index,
                                rule,
                                event.currentTarget.value,
                              )
                            }
                            onChange={() => {}}
                          />
                        </label>
                        <label className="mc-policy-rule-edit">
                          <span>Recovery attempts</span>
                          <input
                            type="number"
                            min={0}
                            inputMode="numeric"
                            data-testid={`policy-rule-recovery-${rule.rule_id}`}
                            value={rule.recovery_limit ?? ""}
                            onInput={(event) =>
                              setRecoveryLimit(
                                index,
                                rule,
                                event.currentTarget.value,
                              )
                            }
                            onChange={() => {}}
                          />
                        </label>
                      </div>
                    ) : null}
                    <RuleQuotas quotas={rule.technical_resource_quotas} />
                  </div>
                </details>
              </li>
            ))}
          </ul>
        )}
      </Surface>
      {draft && !reviewOpen ? (
        <div className="mc-policy-draft-bar" data-testid="policy-draft-bar">
          <span>
            You're editing the ground rules. Nothing changes until you review
            and confirm.
          </span>
          <div className="mc-policy-draft-actions">
            <button
              type="button"
              className="mc-btn mc-btn-accent"
              data-testid="policy-review-open"
              onClick={() => setReviewOpen(true)}
            >
              Review change
            </button>
            <button
              type="button"
              className="mc-btn"
              data-testid="policy-discard"
              onClick={() => {
                controller.discardDraft();
                setReviewOpen(false);
              }}
            >
              Discard
            </button>
          </div>
        </div>
      ) : null}
      {draft && reviewOpen ? (
        <Surface
          className="mc-policy-room-panel mc-policy-review"
          title="Review, then one confirmation"
          subtitle="This is the one check before the change applies. Confirm once and it proceeds."
        >
          <div data-testid="policy-review">
            <p className="mc-policy-review-consequence">
              From revision {policy.revision}, derived and unattended work will
              follow <strong>{profileName(draft.profile)}</strong>. Your exact
              instructions stay yours, and dangerous actions still get their
              one consequence confirmation.
            </p>
            {policy.profile !== draft.profile ? (
              <p className="mc-policy-review-diff">
                Profile: {profileName(policy.profile)} →{" "}
                {profileName(draft.profile)}
              </p>
            ) : null}
            <label className="mc-policy-review-summary">
              <span>Say what changed, in your words</span>
              <textarea
                data-testid="policy-change-summary"
                value={draft.changeSummary}
                placeholder="e.g. Let routine work run overnight without waiting on me"
                onChange={(event) =>
                  controller.setDraftChangeSummary(event.target.value)
                }
                rows={2}
              />
            </label>
            <div className="mc-policy-review-actions">
              <button
                type="button"
                className="mc-btn mc-btn-accent"
                data-testid="policy-confirm"
                disabled={
                  controller.updateBusy ||
                  controller.conflict ||
                  !changeSummaryReady
                }
                onClick={() => {
                  void controller.applyDraft().then((outcome) => {
                    if (outcome.ok) {
                      setReviewOpen(false);
                    }
                  });
                }}
              >
                {controller.updateBusy ? "Applying…" : "Confirm and apply"}
              </button>
              <button
                type="button"
                className="mc-btn"
                data-testid="policy-review-cancel"
                onClick={() => setReviewOpen(false)}
              >
                Back to editing
              </button>
            </div>
          </div>
        </Surface>
      ) : null}
    </section>
  );
}
