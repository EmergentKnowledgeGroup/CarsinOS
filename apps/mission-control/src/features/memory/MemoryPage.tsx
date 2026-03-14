import {
  Activity,
  Brain,
  FileArchive,
  GitBranch,
  Link2,
  Network,
  Radar,
  RefreshCw,
  ScrollText,
  ShieldAlert,
  ShieldCheck,
} from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Surface } from "../../ui/Surface";
import type {
  AgentMemoryCardSummary,
  AgentMemoryGraphLink,
  AgentMemoryWhyCitation,
} from "../../types";
import type { useMemoryController } from "./useMemoryController";

interface MemoryPageProps {
  controller: ReturnType<typeof useMemoryController>;
  onOpenAssistant: (agentId: string) => void;
}

function toneForBindingStatus(
  status: string | null | undefined
): "up" | "down" | "warning" | "checking" | "" {
  switch (status) {
    case "available":
      return "up";
    case "degraded":
      return "warning";
    case "unauthorized":
    case "unavailable":
      return "down";
    case "unconfigured":
      return "checking";
    default:
      return "";
  }
}

function stringifyValue(value: unknown): string | null {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed ? trimmed : null;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

function previewFacts(
  value: Record<string, unknown> | null | undefined,
  preferredKeys: string[],
  limit = 6
): Array<[string, string]> {
  if (!value) {
    return [];
  }
  const prioritized = preferredKeys
    .map((key) => [key, stringifyValue(value[key])] as const)
    .filter((entry): entry is [string, string] => Boolean(entry[1]));
  const remaining = Object.entries(value)
    .filter(([key]) => !preferredKeys.includes(key))
    .map(([key, raw]) => [key, stringifyValue(raw)] as const)
    .filter((entry): entry is [string, string] => Boolean(entry[1]));
  return [...prioritized, ...remaining].slice(0, limit);
}

function cardLabel(card: AgentMemoryCardSummary): string {
  return card.summary?.trim() || card.kind || card.atom_id;
}

function relationLabel(link: AgentMemoryGraphLink): string {
  return `${link.kind}: ${link.source} -> ${link.target}`;
}

function citationToken(citation: AgentMemoryWhyCitation): string | null {
  return (
    stringifyValue(citation.citation_token) ??
    stringifyValue(citation.token) ??
    stringifyValue(citation.id)
  );
}

function MemoryStatePanel({
  title,
  detail,
}: {
  title: string;
  detail: string;
}) {
  return (
    <section className="mc-memory-page" data-testid="memory-page">
      <Surface className="mc-memory-state" title={title} subtitle={detail}>
        <EmptyState message={detail} />
      </Surface>
    </section>
  );
}

function SummaryCard({
  icon,
  label,
  value,
  detail,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="mc-memory-summary-card">
      <div className="mc-memory-summary-kicker">
        {icon}
        <span>{label}</span>
      </div>
      <strong>{value}</strong>
      <p>{detail}</p>
    </div>
  );
}

function SurfaceLockState({ message }: { message: string }) {
  return <EmptyState className="mc-memory-empty" message={message} />;
}

function FactList({
  facts,
  totalAvailable,
}: {
  facts: Array<[string, string]>;
  totalAvailable?: number;
}) {
  const [expanded, setExpanded] = useState(false);
  if (facts.length === 0) {
    return <EmptyState className="mc-memory-empty" message="No structured facts available." />;
  }
  const hasMore = totalAvailable !== undefined && totalAvailable > facts.length;
  return (
    <>
      <dl className="mc-memory-fact-list">
        {facts.map(([key, value]) => (
          <div key={key} className="mc-memory-fact-row">
            <dt>{key.replaceAll("_", " ")}</dt>
            <dd>{expanded ? value : value.length > 120 ? `${value.slice(0, 120)}\u2026` : value}</dd>
          </div>
        ))}
      </dl>
      {(hasMore || facts.some(([, v]) => v.length > 120)) ? (
        <button type="button" className="ghost mc-memory-show-more" onClick={() => setExpanded(!expanded)}>
          {expanded ? "Show less" : `Show more${hasMore ? ` (${totalAvailable} total)` : ""}`}
        </button>
      ) : null}
    </>
  );
}

export function MemoryPage({ controller, onOpenAssistant }: MemoryPageProps) {
  if (!controller.enabled || controller.availability === "disabled") {
    return (
      <MemoryStatePanel
        title="Memory hub is disabled"
        detail="Enable Memory hub in Config > Reliability + Rollout to expose assistant-bound MNO lanes."
      />
    );
  }

  if (controller.availability === "unsupported") {
    return (
      <MemoryStatePanel
        title="Memory surface unavailable"
        detail={
          controller.availabilityMessage ??
          "The connected gateway does not expose assistant-bound Memory contracts yet."
        }
      />
    );
  }

  if (controller.availability === "error") {
    return (
      <MemoryStatePanel
        title="Memory failed to load"
        detail={controller.availabilityMessage ?? "Memory could not load."}
      />
    );
  }

  if (controller.availability === "loading" && !controller.status) {
    return (
      <MemoryStatePanel
        title="Loading Memory"
        detail="Resolving assistant-bound MNO lanes and probing native read surfaces."
      />
    );
  }

  const bindingStatus = controller.status?.binding_status ?? "unconfigured";
  const isUnavailable =
    bindingStatus === "unconfigured" ||
    bindingStatus === "unauthorized" ||
    bindingStatus === "unavailable";
  const binding = controller.status?.binding ?? controller.selectedAgent?.memory_binding ?? null;
  const laneFacts = previewFacts(
    {
      binding_id: binding?.binding_id,
      provider_kind: binding?.provider_kind,
      base_url: binding?.base_url,
      auth_mode: binding?.auth_mode,
      principal_id: binding?.principal_id,
      principal_display_name: binding?.principal_display_name,
      trusted_local_operator_actions: binding?.trusted_local_operator_actions,
    },
    [
      "binding_id",
      "provider_kind",
      "base_url",
      "auth_mode",
      "principal_display_name",
      "principal_id",
      "trusted_local_operator_actions",
    ]
  );
  const cardFacts = previewFacts(
    (controller.cardDetailResponse?.data.card ?? null) as Record<string, unknown> | null,
    ["card_id", "atom_id", "kind", "status", "summary", "contradiction"]
  );
  const atomFacts = previewFacts(
    (controller.atomDetailResponse?.data.atom ?? null) as Record<string, unknown> | null,
    ["atom_id", "kind", "status", "summary", "label"]
  );
  const whyFacts = previewFacts(
    (controller.turnWhyResponse?.data.why ?? null) as Record<string, unknown> | null,
    ["decision", "decision_reason", "evidence_time_window", "citations_hidden"]
  );
  const healthFacts = previewFacts(
    (controller.runtimeHealthResponse?.data ?? null) as Record<string, unknown> | null,
    ["status", "checked_at"]
  );
  const turnFacts = previewFacts(
    (controller.selectedTurn as Record<string, unknown> | null) ?? null,
    ["turn_id", "route", "decision_reason", "latency_ms", "created_at_utc"]
  );
  const citationFacts = previewFacts(
    (controller.citationResponse?.data ?? null) as Record<string, unknown> | null,
    ["citation", "source_id"]
  );
  const activeWhyCitations = controller.turnWhyResponse?.data.why.citations ?? [];

  return (
    <section className="mc-memory-page" data-testid="memory-page">
      <div className="mc-memory-summary-strip">
        <SummaryCard
          icon={<Brain size={16} />}
          label="Lane"
          value={controller.selectedAgent?.name ?? "No agent"}
          detail={`${bindingStatus} memory binding`}
        />
        <SummaryCard
          icon={<FileArchive size={16} />}
          label="Cards"
          value={String(controller.cards.length)}
          detail="Canonical memory cards from the active assistant lane."
        />
        <SummaryCard
          icon={<Network size={16} />}
          label="Graph"
          value={String(controller.graphNodes.length)}
          detail={
            controller.graphMapResponse?.data.truncated
              ? "Overview truncated by MNO limits."
              : "Overview map from MNO."
          }
        />
        <SummaryCard
          icon={<Activity size={16} />}
          label="Telemetry"
          value={String(controller.telemetryTurns.length)}
          detail="Recent turns available for explainability drilldown."
        />
      </div>

      <div className="mc-memory-grid">
        <Surface
          className="mc-memory-panel mc-memory-sidebar"
          title="Assistant Lane"
          subtitle="One MNO lane per assistant. No shared cross-assistant memory core."
          headerRight={
            <button type="button" className="ghost" onClick={() => void controller.refresh()}>
              <RefreshCw size={14} /> Refresh
            </button>
          }
        >
          <label className="mc-memory-field">
            <span>Assistant</span>
            <select
              data-testid="memory-agent-select"
              value={controller.selectedAgentId}
              onChange={(event) => controller.setSelectedAgentId(event.target.value)}
            >
              {controller.agents.map((agent) => (
                <option key={agent.agent_id} value={agent.agent_id}>
                  {agent.name}
                </option>
              ))}
            </select>
          </label>
          <div className="mc-memory-chip-row">
            <Chip label={`lane: ${bindingStatus}`} tone={toneForBindingStatus(bindingStatus)} />
            <Chip
              label={`orchestration: ${controller.status?.orchestration.health_status ?? "n/a"}`}
              tone={toneForBindingStatus(controller.status?.orchestration.health_status)}
            />
            <Chip
              label={
                controller.status?.native_runtime_health_mismatch
                  ? "health mismatch"
                  : "health aligned"
              }
              tone={controller.status?.native_runtime_health_mismatch ? "warning" : "up"}
            />
          </div>
          <FactList facts={laneFacts} />
          <div className="mc-memory-inline-actions">
            <button
              type="button"
              className="ghost"
              onClick={() => {
                if (controller.selectedAgentId) {
                  onOpenAssistant(controller.selectedAgentId);
                }
              }}
              disabled={!controller.selectedAgentId}
            >
              <ScrollText size={14} /> Open Assistant
            </button>
          </div>
          {controller.status?.native_runtime_health_mismatch ? (
            <div className="mc-memory-warning">
              <ShieldAlert size={14} />
              <span>
                Memory integration health and runtime health report different states.
                The orchestration-level health check is the source of truth.
              </span>
            </div>
          ) : null}
          {bindingStatus === "degraded" ? (
            <div className="mc-memory-warning">
              <ShieldAlert size={14} />
              <span>
                This lane is degraded. Mission Control will keep showing native surfaces that are
                still available.
              </span>
            </div>
          ) : null}
          {isUnavailable ? (
            <div className="mc-memory-warning">
              <ShieldAlert size={14} />
              <span>
                {bindingStatus === "unconfigured"
                  ? "This assistant does not have an MNO lane bound yet."
                  : bindingStatus === "unauthorized"
                    ? "This assistant lane exists, but auth is missing or insufficient."
                    : "This assistant lane exists, but carsinOS cannot reach it safely."}
              </span>
            </div>
          ) : null}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-sidebar"
          title="Card Index"
          subtitle="Browse the active lane before drilling into atom or graph detail."
        >
          <div className="mc-memory-filter-grid">
            <label className="mc-memory-field">
              <span>Search</span>
              <input
                value={controller.cardQuery}
                onChange={(event) => controller.setCardQuery(event.target.value)}
                placeholder="summary, atom, kind"
              />
            </label>
            <label className="mc-memory-field">
              <span>Status</span>
              <select
                value={controller.cardStatusFilter}
                onChange={(event) => controller.setCardStatusFilter(event.target.value)}
              >
                <option value="all">all</option>
                <option value="active">active</option>
                <option value="archived">archived</option>
              </select>
            </label>
          </div>
          {!controller.nativeSurfaceAvailability.cards || !controller.canRead ? (
            <SurfaceLockState message="Cards are unavailable for this assistant lane." />
          ) : controller.cards.length === 0 ? (
            <SurfaceLockState message="No cards matched this lane and filter set." />
          ) : (
            <div className="mc-memory-list">
              {controller.cards.map((card) => (
                <button
                  key={`${card.card_id ?? card.atom_id}`}
                  type="button"
                  className={`mc-memory-list-item${
                    controller.selectedCardId === card.card_id ? " is-active" : ""
                  }`}
                  onClick={() => {
                    if (card.card_id) {
                      controller.setSelectedCardId(card.card_id);
                    }
                    controller.setSelectedAtomId(card.atom_id);
                    controller.setSelectedGraphAtomId(card.atom_id);
                  }}
                >
                  <div className="mc-memory-list-head">
                    <strong>{cardLabel(card)}</strong>
                    <Chip label={card.kind} />
                  </div>
                  <p>{card.atom_id}</p>
                  <div className="mc-memory-list-foot">
                    {card.status ? <span>{card.status}</span> : null}
                    {card.contradiction ? <span>{String(card.contradiction)}</span> : null}
                  </div>
                </button>
              ))}
            </div>
          )}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-sidebar"
          title="Episode Ledger"
          subtitle="Episode summaries are lane-local. Editing is not yet supported — episodes are recorded by the runtime."
        >
          <label className="mc-memory-field">
            <span>Search</span>
            <input
              value={controller.episodeQuery}
              onChange={(event) => controller.setEpisodeQuery(event.target.value)}
              placeholder="episode, run, card"
            />
          </label>
          {!controller.nativeSurfaceAvailability.episodes || !controller.canRead ? (
            <SurfaceLockState message="Episodes are unavailable for this assistant lane." />
          ) : controller.episodes.length === 0 ? (
            <SurfaceLockState message="No episodes matched the current search." />
          ) : (
            <div className="mc-memory-list">
              {controller.episodes.map((episode) => (
                <article key={episode.episode_id} className="mc-memory-list-item is-static">
                  <div className="mc-memory-list-head">
                    <strong>{episode.label || episode.episode_id}</strong>
                    {episode.status ? <Chip label={episode.status} /> : null}
                  </div>
                  <p>{episode.run_id || episode.card_id || "No linked run/card"}</p>
                  {episode.updated_at_utc ? (
                    <div className="mc-memory-list-foot">
                      <span>{episode.updated_at_utc}</span>
                    </div>
                  ) : null}
                </article>
              ))}
            </div>
          )}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-atlas"
          title="Atlas Overview"
          subtitle="`/api/memory/graph-map` is the authoritative overview source."
        >
          {!controller.nativeSurfaceAvailability.graph_overview || !controller.canRead ? (
            <SurfaceLockState message="Graph overview is unavailable for this lane." />
          ) : (
            <>
              <div className="mc-memory-chip-row">
                <Chip label={`nodes: ${controller.graphNodes.length}`} />
                <Chip label={`links: ${controller.graphLinks.length}`} />
                <Chip
                  label={
                    controller.graphMapResponse?.data.truncated
                      ? "truncated (60-node limit)"
                      : "complete"
                  }
                  tone={controller.graphMapResponse?.data.truncated ? "warning" : "up"}
                />
              </div>
              <div className="mc-memory-atlas-map">
                {controller.graphNodes.map((node) => (
                  <button
                    key={`graph-${node.atom_id}`}
                    type="button"
                    className={`mc-memory-node${
                      controller.selectedGraphAtomId === node.atom_id ? " is-active" : ""
                    }`}
                    onClick={() => {
                      controller.setSelectedGraphAtomId(node.atom_id);
                      controller.setSelectedAtomId(node.atom_id);
                    }}
                  >
                    <span>{cardLabel(node)}</span>
                    <small>{node.atom_id}</small>
                  </button>
                ))}
              </div>
            </>
          )}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-atlas"
          title="Local Neighborhood"
          subtitle="Server-side BFS expansion from `/api/memory/graph/neighbors`."
        >
          {!controller.nativeSurfaceAvailability.graph_neighbors || !controller.canRead ? (
            <SurfaceLockState message="Graph neighborhood drilldown is unavailable for this lane." />
          ) : controller.graphError ? (
            <SurfaceLockState message={controller.graphError} />
          ) : !controller.graphNeighborsResponse ? (
            <SurfaceLockState message="Select an atom from the overview to inspect its neighborhood." />
          ) : (
            <>
              <div className="mc-memory-neighborhood-head">
                <div>
                  <strong>{cardLabel(controller.graphNeighborsResponse.data.node)}</strong>
                  <p>{controller.graphNeighborsResponse.data.node.atom_id}</p>
                </div>
                <div className="mc-memory-chip-row">
                  <Chip label={`depth ${controller.graphNeighborsResponse.data.depth}`} />
                  <Chip label={`requests ${controller.graphNeighborsResponse.data.requests_used}`} />
                  <Chip
                    label={
                      controller.graphNeighborsResponse.data.truncated
                        ? "truncated"
                        : "complete"
                    }
                    tone={
                      controller.graphNeighborsResponse.data.truncated ? "warning" : "up"
                    }
                  />
                </div>
              </div>
              <div className="mc-memory-neighbor-grid">
                {controller.graphNeighborsResponse.data.neighbors.map((neighbor) => (
                  <button
                    key={`neighbor-${neighbor.atom_id}`}
                    type="button"
                    className="mc-memory-neighbor-card"
                    onClick={() => {
                      controller.setSelectedGraphAtomId(neighbor.atom_id);
                      controller.setSelectedAtomId(neighbor.atom_id);
                    }}
                  >
                    <div className="mc-memory-list-head">
                      <strong>{cardLabel(neighbor)}</strong>
                      {neighbor.distance ? <Chip label={`d${neighbor.distance}`} /> : null}
                    </div>
                    <p>{neighbor.atom_id}</p>
                    {neighbor.via_edge_kind ? (
                      <div className="mc-memory-list-foot">
                        <span>{neighbor.via_edge_kind}</span>
                      </div>
                    ) : null}
                  </button>
                ))}
              </div>
              <div className="mc-memory-link-list">
                {controller.graphNeighborsResponse.data.links
                  .slice(0, 8)
                  .map((link, index) => (
                    <div key={`link-${index}`} className="mc-memory-link-row">
                      <GitBranch size={13} />
                      <span>{relationLabel(link)}</span>
                    </div>
                  ))}
              </div>
            </>
          )}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-detail"
          title="Selected Record"
          subtitle="Card and atom detail stay read-only and lane-scoped in this phase."
        >
          {controller.detailError ? (
            <SurfaceLockState message={controller.detailError} />
          ) : !controller.cardDetailResponse && !controller.atomDetailResponse ? (
            <SurfaceLockState message="Select a card or graph node to inspect detail." />
          ) : (
            <div className="mc-memory-detail-stack">
              <div className="mc-memory-subpanel">
                <div className="mc-memory-subpanel-head">
                  <h4>Card dossier</h4>
                  {controller.selectedCard?.card_id ? (
                    <Chip label={controller.selectedCard.card_id} />
                  ) : null}
                </div>
                <FactList facts={cardFacts} />
              </div>
              <div className="mc-memory-subpanel">
                <div className="mc-memory-subpanel-head">
                  <h4>Atom detail</h4>
                  {controller.selectedAtomId ? <Chip label={controller.selectedAtomId} /> : null}
                </div>
                <FactList facts={atomFacts} />
              </div>
              {controller.cardDetailResponse?.data.provenance_events?.length ? (
                <div className="mc-memory-subpanel">
                  <div className="mc-memory-subpanel-head">
                    <h4>Provenance</h4>
                  </div>
                  <div className="mc-memory-link-list">
                    {controller.cardDetailResponse.data.provenance_events
                      .slice(0, 6)
                      .map((event, index) => (
                        <div key={`prov-${index}`} className="mc-memory-link-row">
                          <Link2 size={13} />
                          <span>
                            {stringifyValue(event.kind) ??
                              stringifyValue(event.source_kind) ??
                              "provenance event"}
                          </span>
                        </div>
                      ))}
                  </div>
                </div>
              ) : null}
            </div>
          )}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-detail"
          title="Why + Citations"
          subtitle="Telemetry turns drive explainability and citation drilldown."
        >
          {!controller.nativeSurfaceAvailability.turn_why || !controller.canRead ? (
            <SurfaceLockState message="Why/citation surfaces are unavailable for this lane." />
          ) : (
            <div className="mc-memory-detail-stack">
              <div className="mc-memory-subpanel">
                <div className="mc-memory-subpanel-head">
                  <h4>Recent turns</h4>
                </div>
                {controller.telemetryTurns.length === 0 ? (
                  <SurfaceLockState message="No recent telemetry turns available." />
                ) : (
                  <div className="mc-memory-turn-row">
                    {controller.telemetryTurns.map((turn, index) => {
                      const turnRecord = turn as Record<string, unknown>;
                      const turnId = stringifyValue(
                        turnRecord.turn_id ?? turnRecord.id
                      );
                      if (!turnId) {
                        return null;
                      }
                      const turnTimestamp = stringifyValue(turnRecord.created_at_utc ?? turnRecord.created_at ?? turnRecord.timestamp);
                      return (
                        <button
                          key={`${turnId}-${index}`}
                          type="button"
                          className={`mc-memory-turn-pill${
                            controller.selectedTurnId === turnId ? " is-active" : ""
                          }`}
                          onClick={() => controller.setSelectedTurnId(turnId)}
                          title={turnTimestamp ? `Turn ${turnId} \u00B7 ${turnTimestamp}` : turnId}
                        >
                          <span className="mc-turn-pill-id">{turnId.slice(0, 8)}</span>
                          {turnTimestamp ? <span className="mc-turn-pill-ts">{turnTimestamp}</span> : null}
                        </button>
                      );
                    })}
                  </div>
                )}
                <FactList facts={turnFacts} />
              </div>

              <div className="mc-memory-subpanel">
                <div className="mc-memory-subpanel-head">
                  <h4>Decision why</h4>
                </div>
                {controller.whyError ? (
                  <SurfaceLockState message={controller.whyError} />
                ) : (
                  <FactList facts={whyFacts} />
                )}
              </div>

              <div className="mc-memory-subpanel">
                <div className="mc-memory-subpanel-head">
                  <h4>Citation drilldown</h4>
                </div>
                {activeWhyCitations.length === 0 ? (
                  <SurfaceLockState message="No citations returned for the selected turn." />
                ) : (
                  <div className="mc-memory-turn-row">
                    {activeWhyCitations.map((citation, index) => {
                      const token = citationToken(citation);
                      if (!token) {
                        return null;
                      }
                      return (
                        <button
                          key={`${token}-${index}`}
                          type="button"
                          className={`mc-memory-turn-pill${
                            controller.selectedCitationToken === token ? " is-active" : ""
                          }`}
                          onClick={() => controller.setSelectedCitationToken(token)}
                        >
                          {citation.label || token}
                        </button>
                      );
                    })}
                  </div>
                )}
                {controller.citationError ? (
                  <SurfaceLockState message={controller.citationError} />
                ) : (
                  <FactList facts={citationFacts} />
                )}
              </div>
            </div>
          )}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-detail"
          title="Runtime Telemetry"
          subtitle="Native runtime health is secondary to orchestration health but still operator-visible."
        >
          <div className="mc-memory-detail-stack">
            <div className="mc-memory-subpanel">
              <div className="mc-memory-subpanel-head">
                <h4>Health</h4>
                <span className="mc-memory-health-icon">
                  {controller.runtimeHealthResponse?.data.status === "ok" ? (
                    <ShieldCheck size={14} />
                  ) : (
                    <ShieldAlert size={14} />
                  )}
                </span>
              </div>
              <FactList facts={healthFacts} />
            </div>

            <div className="mc-memory-subpanel">
              <div className="mc-memory-subpanel-head">
                <h4>Telemetry summary</h4>
              </div>
              {controller.telemetrySummary.length === 0 ? (
                <SurfaceLockState message="Telemetry summary is unavailable for this lane." />
              ) : (
                <div className="mc-memory-link-list">
                  {controller.telemetrySummary.slice(0, 6).map((row, index) => (
                    <div key={`summary-${index}`} className="mc-memory-link-row">
                      <Radar size={13} />
                      <span>
                        {stringifyValue((row as Record<string, unknown>).label) ??
                          stringifyValue((row as Record<string, unknown>).route) ??
                          stringifyValue((row as Record<string, unknown>).kind) ??
                          "summary row"}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>

            <div className="mc-memory-subpanel">
              <div className="mc-memory-subpanel-head">
                <h4>Decision reasons</h4>
              </div>
              {controller.decisionReasons.length === 0 ? (
                <SurfaceLockState message="Decision reasons are unavailable for this lane." />
              ) : (
                <div className="mc-memory-link-list">
                  {controller.decisionReasons.slice(0, 8).map((reason, index) => (
                    <div key={`reason-${index}`} className="mc-memory-link-row">
                      <Brain size={13} />
                      <span>
                        {stringifyValue((reason as Record<string, unknown>).label) ??
                          stringifyValue((reason as Record<string, unknown>).reason) ??
                          stringifyValue((reason as Record<string, unknown>).title) ??
                          "decision reason"}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </Surface>
      </div>
    </section>
  );
}
