/**
 * The authoritative People & Routing surface, rehomed to Directory / Front
 * Desk. Presentation only: every read, draft, validation, and save flows
 * through the single shared PeopleRoutingController instance. Shows only
 * server-backed people, links, assignments, and policies — never contacts
 * inferred from Mail history.
 */

import { useState, type ReactNode } from "react";
import { Bot, GitBranch, Link2, Plus, RefreshCw, Save, Trash2, Users } from "lucide-react";
import clsx from "clsx";

import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Pagination } from "../../ui/Pagination";
import { Surface } from "../../ui/Surface";
import { usePagination } from "../../ui/usePagination";
import type { Agent } from "../../types";
import type { PeopleRoutingController } from "./usePeopleRoutingController";

const ROUTING_PROVIDER_OPTIONS = [
  { value: "discord", label: "Discord" },
  { value: "telegram", label: "Telegram" },
];

const DM_UNMAPPED_POLICY_OPTIONS = [
  { value: "approval_required", label: "Ask for approval" },
  { value: "block", label: "Block" },
];

const SHARED_UNMAPPED_POLICY_OPTIONS = [
  { value: "block", label: "Block" },
];

function providerIdentityLabel(provider: string): string {
  switch (provider.trim().toLowerCase()) {
    case "discord":
      return "Discord";
    case "telegram":
      return "Telegram";
    default:
      return provider || "Link";
  }
}

function laneMemoryModeLabel(mode: string): string {
  switch (mode) {
    case "disabled":
      return "Memory off";
    case "local_only":
      return "Local only";
    case "mno_only":
      return "MNO only";
    case "mno_with_local_sources":
      return "MNO + local";
    case "inherit_runtime":
    default:
      return "Runtime default";
  }
}

interface PeopleRoutingSectionProps {
  controller: PeopleRoutingController;
  agents: Agent[];
  /**
   * Optional room-identity affordance (Pin to Office). Rendered only on the
   * ready surface so loading/error/unconfigured panels expose no dead control.
   */
  pin?: ReactNode;
}

export function PeopleRoutingSection({
  controller,
  agents,
  pin,
}: PeopleRoutingSectionProps) {
  const [routingView, setRoutingView] = useState<"people" | "overview">("people");
  const [routingPage, setRoutingPage] = useState(1);

  const {
    gatewayConfigured,
    routingDraft,
    routingLoading,
    routingSaving,
    routingError,
    routingNotice,
    routingDirty,
    humanRoutingCards,
    routingSummary,
    loadRoutingConfig,
    patchRoutingDraft,
    addHumanIdentity,
    updateHumanDisplayName,
    updateHumanEnabled,
    removeHumanIdentity,
    setHumanAssignment,
    addPlatformIdentityLink,
    updatePlatformIdentityLink,
    removePlatformIdentityLink,
    resetRoutingDraft,
    saveRoutingDraft,
  } = controller;

  const agentsById = new Map(agents.map((agent) => [agent.agent_id, agent] as const));

  const routingPagination = usePagination(humanRoutingCards, 4);
  // Keep the stored page aligned with the rendered clamp so a shrink cannot
  // leave a stale later page behind that resurfaces on regrowth.
  const clampedRoutingPage = routingPagination.clampPage(routingPage);
  if (clampedRoutingPage !== routingPage) {
    setRoutingPage(clampedRoutingPage);
  }
  const visibleHumanRoutingCards = routingPagination.getPage(clampedRoutingPage);

  const ready = gatewayConfigured && routingDraft !== null;

  return (
    <Surface
      className="mc-team-routing-surface"
      title="People And Routing"
      subtitle="Decide who is talking, where they come from, and which assistant owns their main lane across Discord and Telegram."
      headerRight={
        <div className="mc-strategy-inline-actions">
          {ready ? pin : null}
          <button
            type="button"
            className="ghost"
            onClick={() => {
              if (
                routingDirty &&
                !window.confirm(
                  "Refresh people and routing? Your unsaved routing changes will be discarded."
                )
              ) {
                return;
              }
              void loadRoutingConfig();
            }}
            disabled={routingLoading || routingSaving}
          >
            <RefreshCw size={14} />
            Refresh
          </button>
          <button
            type="button"
            className="ghost"
            onClick={addHumanIdentity}
            disabled={!routingDraft || routingLoading || routingSaving}
          >
            <Plus size={14} />
            Add Person
          </button>
          <button
            type="button"
            className="ghost"
            onClick={resetRoutingDraft}
            disabled={!routingDirty || routingSaving}
          >
            Reset
          </button>
          <button
            type="button"
            className={clsx("mc-btn mc-btn-accent", routingSaving && "mc-btn-loading")}
            onClick={() => void saveRoutingDraft()}
            disabled={!routingDraft || !routingDirty || routingSaving}
          >
            <Save size={14} />
            Save Routing
          </button>
        </div>
      }
    >
      {!gatewayConfigured ? (
        <EmptyState message="Connect Mission Control to the gateway before you manage people and routing." />
      ) : routingLoading && !routingDraft ? (
        <EmptyState message="Loading people and routing..." />
      ) : (
        <>
          {routingError ? <div className="mc-notice mc-notice-error">{routingError}</div> : null}
          {routingNotice ? (
            <div
              className={clsx(
                "mc-notice",
                routingNotice.tone === "error" ? "mc-notice-error" : "mc-notice-info"
              )}
            >
              {routingNotice.message}
            </div>
          ) : null}

          {routingDraft ? (
            <>
              <div className="mc-page-section-tabs" aria-label="Routing views">
                <button
                  type="button"
                  className={`mc-page-section-btn${routingView === "people" ? " mc-page-section-btn-active" : ""}`}
                  onClick={() => setRoutingView("people")}
                >
                  People
                </button>
                <button
                  type="button"
                  className={`mc-page-section-btn${routingView === "overview" ? " mc-page-section-btn-active" : ""}`}
                  onClick={() => setRoutingView("overview")}
                >
                  Routing Setup
                </button>
              </div>

              {routingView === "overview" ? (
                <div className="mc-page-section-stack">
                  <div className="mc-team-routing-summary">
                    <div className="mc-team-routing-summary-card">
                      <span className="mc-team-routing-kicker">
                        <Users size={14} />
                        People
                      </span>
                      <strong>{routingSummary.humans}</strong>
                      <p>Humans currently active in routing.</p>
                    </div>
                    <div className="mc-team-routing-summary-card">
                      <span className="mc-team-routing-kicker">
                        <Link2 size={14} />
                        Linked Accounts
                      </span>
                      <strong>{routingSummary.linkedAccounts}</strong>
                      <p>Discord or Telegram identities tied to those people.</p>
                    </div>
                    <div className="mc-team-routing-summary-card">
                      <span className="mc-team-routing-kicker">
                        <Bot size={14} />
                        Assigned Assistants
                      </span>
                      <strong>{routingSummary.assignedHumans}</strong>
                      <p>People already routed to one real assistant.</p>
                    </div>
                    <div className="mc-team-routing-summary-card">
                      <span className="mc-team-routing-kicker">
                        <GitBranch size={14} />
                        Local Operator
                      </span>
                      <strong>{routingSummary.localOperator}</strong>
                      <p>
                        {routingSummary.waitingForAssignment} active people still need an
                        assistant route.
                      </p>
                    </div>
                  </div>

                  <div className="mc-team-routing-controls">
                    <label className="mc-modal-field">
                      <span>Local app operator</span>
                      <select
                        value={routingDraft.local_operator_human_identity_id ?? ""}
                        onChange={(event) =>
                          patchRoutingDraft((next) => {
                            next.local_operator_human_identity_id =
                              event.target.value || null;
                          })
                        }
                      >
                        <option value="">Choose person...</option>
                        {routingDraft.human_identities
                          .filter((human) => human.enabled)
                          .map((human) => (
                            <option
                              key={human.human_identity_id}
                              value={human.human_identity_id}
                            >
                              {human.display_name || human.human_identity_id}
                            </option>
                          ))}
                      </select>
                      <small>
                        Assistant chat on this machine uses this person record so your desktop,
                        Discord, and Telegram conversations can stay in one shared lane once
                        those accounts are linked.
                      </small>
                    </label>
                    <label className="mc-modal-field">
                      <span>Unknown DMs</span>
                      <select
                        value={routingDraft.dm_unmapped_policy}
                        onChange={(event) =>
                          patchRoutingDraft((next) => {
                            next.dm_unmapped_policy = event.target.value;
                          })
                        }
                      >
                        {DM_UNMAPPED_POLICY_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                      <small>
                        Best default here is ask first. Unknown DMs should not silently land
                        in an assistant lane.
                      </small>
                    </label>
                    <label className="mc-modal-field">
                      <span>Unknown shared-space messages</span>
                      <select
                        value={routingDraft.shared_unmapped_policy}
                        onChange={(event) =>
                          patchRoutingDraft((next) => {
                            next.shared_unmapped_policy = event.target.value;
                          })
                        }
                      >
                        {SHARED_UNMAPPED_POLICY_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                      <small>
                        Best default here is block. Shared spaces should not guess which
                        assistant a stranger belongs to.
                      </small>
                    </label>
                  </div>
                </div>
              ) : null}

              {routingView === "people" ? (
                <div className="mc-page-section-stack">
                  <div className="mc-team-routing-grid">
                    {humanRoutingCards.length === 0 ? (
                      <EmptyState message="No routed people yet. Add one person, link their account, then choose their assistant." />
                    ) : (
                      visibleHumanRoutingCards.map((card) => {
                        const assignedAgentName = card.assignment?.assistant_agent_id
                          ? agentsById.get(card.assignment.assistant_agent_id)?.name ??
                            card.assignment.assistant_agent_id
                          : null;
                        return (
                          <article
                            key={`human-route-${card.index}`}
                            className={clsx(
                              "mc-team-routing-card",
                              !card.human.enabled && "is-paused"
                            )}
                          >
                            <div className="mc-team-routing-card-head">
                              <div>
                                <strong>
                                  {card.human.display_name || card.human.human_identity_id}
                                </strong>
                                <span>{card.human.human_identity_id}</span>
                              </div>
                              <div className="mc-team-card-tags">
                                <Chip
                                  label={card.human.enabled ? "active" : "paused"}
                                  tone={card.human.enabled ? "up" : "checking"}
                                />
                                {routingDraft.local_operator_human_identity_id ===
                                card.human.human_identity_id ? (
                                  <Chip label="local operator" tone="up" />
                                ) : null}
                                <Chip
                                  label={
                                    assignedAgentName
                                      ? `assistant: ${assignedAgentName}`
                                      : "needs assistant"
                                  }
                                  tone={assignedAgentName ? "up" : "warning"}
                                />
                                <Chip
                                  label={
                                    card.memoryPolicy
                                      ? laneMemoryModeLabel(card.memoryPolicy.memory_mode)
                                      : "runtime default memory"
                                  }
                                  tone={card.memoryPolicy ? "warning" : "checking"}
                                />
                              </div>
                            </div>

                            <div className="mc-field-grid">
                              <label className="mc-modal-field">
                                <span>Display name</span>
                                <input
                                  value={card.human.display_name}
                                  onChange={(event) =>
                                    updateHumanDisplayName(
                                      card.human.human_identity_id,
                                      event.target.value
                                    )
                                  }
                                  placeholder="Alex"
                                />
                              </label>
                              <label className="mc-modal-field">
                                <span>Human ID</span>
                                <input
                                  value={card.human.human_identity_id}
                                  readOnly
                                  aria-readonly="true"
                                />
                                <small className="mc-field-help">
                                  Lane IDs are locked after creation so history, memory, and channel
                                  routing do not silently fork.
                                </small>
                              </label>
                            </div>

                            <div className="mc-field-grid">
                              <label className="mc-modal-field">
                                <span>Assistant</span>
                                <select
                                  value={card.assignment?.assistant_agent_id ?? ""}
                                  onChange={(event) =>
                                    setHumanAssignment(
                                      card.human.human_identity_id,
                                      event.target.value
                                    )
                                  }
                                >
                                  <option value="">Choose assistant...</option>
                                  {agents.map((agent) => (
                                    <option key={agent.agent_id} value={agent.agent_id}>
                                      {agent.name}
                                    </option>
                                  ))}
                                </select>
                                <small className="mc-field-help">
                                  This is the assistant this person talks to no matter which
                                  linked channel they use.
                                </small>
                              </label>
                              <label className="mc-team-routing-toggle mc-team-routing-toggle-inline">
                                <input
                                  type="checkbox"
                                  checked={card.human.enabled}
                                  onChange={(event) =>
                                    updateHumanEnabled(
                                      card.human.human_identity_id,
                                      event.target.checked
                                    )
                                  }
                                />
                                <span>Person is active</span>
                                <small>Turn this off to pause routing without deleting the record.</small>
                              </label>
                            </div>

                            <div className="mc-team-routing-links">
                              <div className="mc-team-routing-links-head">
                                <div>
                                  <strong>Linked accounts</strong>
                                  <p>
                                    Link this person’s Discord or Telegram identity so carsinOS
                                    can resume the same assistant lane everywhere.
                                  </p>
                                </div>
                                <button
                                  type="button"
                                  className="ghost"
                                  onClick={() =>
                                    addPlatformIdentityLink(card.human.human_identity_id)
                                  }
                                >
                                  <Plus size={14} />
                                  Add Link
                                </button>
                              </div>
                              {card.links.length === 0 ? (
                                <EmptyState message="No linked accounts yet." />
                              ) : (
                                <div className="mc-team-routing-link-list">
                                  {card.links.map(({ index, link }) => (
                                    <div key={`${card.human.human_identity_id}:${index}`} className="mc-team-routing-link-row">
                                      <label className="mc-modal-field">
                                        <span>Provider</span>
                                        <select
                                          value={link.provider}
                                          onChange={(event) =>
                                            updatePlatformIdentityLink(index, {
                                              provider: event.target.value,
                                            })
                                          }
                                        >
                                          {ROUTING_PROVIDER_OPTIONS.map((option) => (
                                            <option key={option.value} value={option.value}>
                                              {option.label}
                                            </option>
                                          ))}
                                        </select>
                                      </label>
                                      <label className="mc-modal-field">
                                        <span>{providerIdentityLabel(link.provider)} user ID</span>
                                        <input
                                          value={link.platform_user_id}
                                          onChange={(event) =>
                                            updatePlatformIdentityLink(index, {
                                              platform_user_id: event.target.value,
                                            })
                                          }
                                          placeholder="platform user id"
                                        />
                                      </label>
                                      <label className="mc-modal-field">
                                        <span>Label</span>
                                        <input
                                          value={link.display_name ?? ""}
                                          onChange={(event) =>
                                            updatePlatformIdentityLink(index, {
                                              display_name: event.target.value || null,
                                            })
                                          }
                                          placeholder="optional nickname"
                                        />
                                      </label>
                                      <label className="mc-team-routing-toggle mc-team-routing-toggle-inline">
                                        <input
                                          type="checkbox"
                                          checked={link.enabled}
                                          onChange={(event) =>
                                            updatePlatformIdentityLink(index, {
                                              enabled: event.target.checked,
                                            })
                                          }
                                        />
                                        <span>Link is active</span>
                                        <small>
                                          Turn this off to keep the mapping saved without using it.
                                        </small>
                                      </label>
                                      <button
                                        type="button"
                                        className="ghost danger"
                                        onClick={() => removePlatformIdentityLink(index)}
                                        title="Remove linked account"
                                      >
                                        <Trash2 size={14} />
                                        Remove
                                      </button>
                                    </div>
                                  ))}
                                </div>
                              )}
                            </div>

                            <div className="mc-team-routing-card-foot">
                              <p>
                                {card.assignment?.assistant_agent_id ? (
                                  <>
                                    Main lane:{" "}
                                    <code>
                                      {card.memoryPolicy?.lane_id ??
                                        `human:${card.human.human_identity_id}:assistant:${card.assignment.assistant_agent_id}`}
                                    </code>
                                  </>
                                ) : (
                                  "Pick an assistant so this person has somewhere real to land."
                                )}
                              </p>
                              <button
                                type="button"
                                className="ghost danger"
                                onClick={() => removeHumanIdentity(card.human.human_identity_id)}
                              >
                                <Trash2 size={14} />
                                Remove Person
                              </button>
                            </div>
                          </article>
                        );
                      })
                    )}
                  </div>
                  <Pagination
                    currentPage={clampedRoutingPage}
                    totalPages={routingPagination.totalPages}
                    onPageChange={setRoutingPage}
                  />
                </div>
              ) : null}
            </>
          ) : null}
        </>
      )}
    </Surface>
  );
}
