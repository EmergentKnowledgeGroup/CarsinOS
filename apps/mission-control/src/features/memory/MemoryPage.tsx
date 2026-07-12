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
  Users,
} from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Surface } from "../../ui/Surface";
import type {
  AgentMemoryLaneStatusResponse,
  AgentMemoryCardSummary,
  AgentMemoryGraphLink,
  AgentMemoryWhyCitation,
  RuntimeRoutingConfigResponse,
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
        {facts.map(([key, value]) => {
          const label = key
            .replaceAll("_", " ")
            .replace(/^principal\b/, "acting as");
          return (
            <div key={key} className="mc-memory-fact-row">
              <dt>{label}</dt>
              <dd>{expanded ? value : value.length > 120 ? `${value.slice(0, 120)}\u2026` : value}</dd>
            </div>
          );
        })}
      </dl>
      {(hasMore || facts.some(([, v]) => v.length > 120)) ? (
        <button type="button" className="ghost mc-memory-show-more" onClick={() => setExpanded(!expanded)}>
          {expanded ? "Show less" : `Show more${hasMore ? ` (${totalAvailable} total)` : ""}`}
        </button>
      ) : null}
    </>
  );
}

interface MemoryLaneSummary {
  humanIdentityId: string;
  displayName: string;
  memoryMode: string;
  memoryLabel: string;
  memoryTone: "up" | "warning" | "checking" | "";
  laneId: string | null;
  localSources: string[];
  localSourceCount: number;
  linkLabels: string[];
  usesRuntimeDefault: boolean;
}

const MEMORY_MODE_OPTIONS = [
  { value: "inherit_runtime", label: "Use runtime default" },
  { value: "disabled", label: "Memory off" },
  { value: "local_only", label: "Local only" },
  { value: "mno_only", label: "Memory only" },
  { value: "mno_with_local_sources", label: "Memory + local" },
] as const;

const RUNTIME_MEMORY_MODE_OPTIONS = [
  { value: "mno_primary", label: "Memory first" },
  { value: "local_augment", label: "Memory + runtime local support" },
  { value: "local_fallback_only", label: "Local fallback only" },
] as const;

function memoryModeLabel(mode: string): string {
  switch (mode) {
    case "disabled":
      return "Memory off";
    case "local_only":
      return "Local only";
    case "mno_only":
      return "Memory only";
    case "mno_with_local_sources":
      return "Memory + local";
    case "inherit_runtime":
    default:
      return "Uses runtime default";
  }
}

function toneForMemoryMode(mode: string): "up" | "warning" | "checking" | "" {
  switch (mode) {
    case "mno_only":
    case "mno_with_local_sources":
      return "up";
    case "local_only":
      return "warning";
    case "disabled":
      return "";
    case "inherit_runtime":
    default:
      return "checking";
  }
}

function runtimeMemoryModeLabel(mode: string): string {
  switch (mode) {
    case "mno_primary":
      return "Memory first";
    case "local_fallback_only":
      return "Local fallback only";
    case "local_augment":
    default:
      return "Memory + runtime local support";
  }
}

function toneForLaneRuntimeStatus(
  status: string | null | undefined
): "up" | "warning" | "checking" | "" {
  switch (status) {
    case "available":
      return "up";
    case "degraded":
    case "unconfigured":
      return "warning";
    case "not_started":
    case "local_only":
    case "memory_off":
      return "checking";
    default:
      return "";
  }
}

function laneRuntimeStatusLabel(status: string | null | undefined): string {
  switch (status) {
    case "available":
      return "Memory ready";
    case "degraded":
      return "Needs attention";
    case "not_started":
      return "Waiting for first use";
    case "local_only":
      return "Local memory only";
    case "memory_off":
      return "Memory off";
    case "unconfigured":
      return "Memory unconfigured";
    default:
      return "Status unknown";
  }
}

function laneRuntimeSourceLabel(source: string | null | undefined): string {
  switch (source) {
    case "managed_lane":
      return "lane runtime";
    case "assistant_binding":
      return "assistant binding";
    case "local_only":
      return "local only";
    case "memory_off":
      return "memory off";
    default:
      return "source unknown";
  }
}

function buildMemoryLaneSummaries(
  routing: RuntimeRoutingConfigResponse | null,
  assistantAgentId: string
): MemoryLaneSummary[] {
  if (!routing?.enabled || !assistantAgentId.trim()) {
    return [];
  }

  const humans = new Map(
    routing.human_identities
      .filter((item) => item.enabled)
      .map((item) => [item.human_identity_id, item] as const)
  );
  const linksByHuman = new Map<string, string[]>();
  for (const link of routing.platform_identity_links) {
    if (!link.enabled) {
      continue;
    }
    const label = `${link.provider}: ${link.display_name?.trim() || link.platform_user_id}`;
    const next = linksByHuman.get(link.human_identity_id) ?? [];
    next.push(label);
    linksByHuman.set(link.human_identity_id, next);
  }
  return routing.assistant_assignments
    .filter(
      (item) =>
        item.enabled &&
        item.assistant_agent_id === assistantAgentId &&
        humans.has(item.human_identity_id)
    )
    .map((assignment) => {
      const human = humans.get(assignment.human_identity_id);
      const policy =
        routing.lane_memory_policies.find(
          (item) =>
            item.human_identity_id === assignment.human_identity_id &&
            item.assistant_agent_id === assignment.assistant_agent_id
        ) ?? null;
      const memoryMode = policy?.memory_mode ?? "inherit_runtime";
      return {
        humanIdentityId: assignment.human_identity_id,
        displayName: human?.display_name?.trim() || assignment.human_identity_id,
        memoryMode,
        memoryLabel: memoryModeLabel(memoryMode),
        memoryTone: toneForMemoryMode(memoryMode),
        laneId: policy?.lane_id ?? null,
        localSources: policy?.local_memory_sources ?? [],
        localSourceCount: policy?.local_memory_sources.length ?? 0,
        linkLabels: (linksByHuman.get(assignment.human_identity_id) ?? []).sort(),
        usesRuntimeDefault: !policy || memoryMode === "inherit_runtime",
      };
    })
    .sort((left, right) => left.displayName.localeCompare(right.displayName));
}

function joinLocalSourcesForDraft(sources: string[]): string {
  return sources.join("\n");
}

function normalizeLocalSourceDraft(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
}

export function MemoryPage({ controller, onOpenAssistant }: MemoryPageProps) {
  const [activeSection, setActiveSection] = useState<
    "routing" | "library" | "episodes" | "graph" | "detail" | "explain" | "health"
  >("routing");
  const [laneModeDrafts, setLaneModeDrafts] = useState<Record<string, string>>({});
  const [laneSourceDrafts, setLaneSourceDrafts] = useState<Record<string, string>>({});
  const [runtimeModeDraft, setRuntimeModeDraft] = useState<string | null>(null);
  const [runtimeSourceDraft, setRuntimeSourceDraft] = useState<string | null>(null);
  if (!controller.enabled || controller.availability === "disabled") {
    return (
      <MemoryStatePanel
        title="Memory is turned off"
        detail="Enable Memory in Config > Reliability + Rollout to inspect what your agents remember."
      />
    );
  }

  if (controller.availability === "unsupported") {
    return (
      <MemoryStatePanel
        title="Memory not available"
        detail={
          controller.availabilityMessage ??
          "This gateway version does not support agent memory inspection yet."
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
        detail="Checking agent memory status..."
      />
    );
  }

  const bindingStatus = controller.status?.binding_status ?? "unconfigured";
  const cardFiltersActive =
    controller.cardQuery.trim().length > 0 || controller.cardStatusFilter !== "all";
  const episodeFiltersActive = controller.episodeQuery.trim().length > 0;
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
  const laneSummaries = buildMemoryLaneSummaries(
    controller.routingConfig,
    controller.selectedAgentId
  );
  const laneStatusByHuman = new Map<string, AgentMemoryLaneStatusResponse>(
    controller.laneStatuses.map((item) => [item.human_identity_id, item])
  );
  const managedLaneCount = controller.laneStatuses.filter(
    (item) => item.source === "managed_lane"
  ).length;
  const availableLaneCount = controller.laneStatuses.filter(
    (item) => item.status === "available"
  ).length;
  const summaryDetail =
    managedLaneCount > 0
      ? `${availableLaneCount}/${managedLaneCount} memory-backed lane${
          managedLaneCount === 1 ? "" : "s"
        } ready.`
      : `${bindingStatus} memory binding`;
  const runtimeDefaultSources = controller.runtimeMemoryConfig?.memory_md_sources ?? [];
  const runtimeModeValue = runtimeModeDraft ?? (controller.runtimeMemoryConfig?.blend_mode ?? "local_augment");
  const runtimeSourceDraftValue =
    runtimeSourceDraft ?? joinLocalSourcesForDraft(runtimeDefaultSources);
  const runtimeSourceDraftItems = normalizeLocalSourceDraft(runtimeSourceDraftValue);
  const runtimeDefaultsDirty =
    runtimeModeValue !== (controller.runtimeMemoryConfig?.blend_mode ?? "local_augment") ||
    runtimeSourceDraftItems.join("\n") !== runtimeDefaultSources.join("\n");
  const runtimeDefaultsModeLabel = runtimeMemoryModeLabel(runtimeModeValue);
  const runtimeDefaultsHelperText =
    runtimeModeValue === "local_fallback_only"
      ? "Use this only when the shared memory service is unavailable for this deployment. These files become the fallback source for local-memory sync."
      : "Shared memory stays the long-term memory truth. These files are support material you can sync into local memory for every lane.";
  const canSyncRuntimeFiles = !runtimeDefaultsDirty && runtimeDefaultSources.length > 0;
  const sidebarLaneLabel =
    controller.laneStatuses.length > 0
      ? `routed lanes: ${controller.laneStatuses.length}`
      : `lane: ${bindingStatus}`;
  const sidebarLaneTone =
    controller.laneStatuses.length > 0
      ? availableLaneCount > 0
        ? "up"
        : "checking"
      : toneForBindingStatus(bindingStatus);

  return (
    <section className="mc-memory-page" data-testid="memory-page">
      <div className="mc-memory-summary-strip">
        <SummaryCard
          icon={<Brain size={16} />}
          label="Lane"
          value={controller.selectedAgent?.name ?? "No agent"}
          detail={summaryDetail}
        />
        <SummaryCard
          icon={<FileArchive size={16} />}
          label="Cards"
          value={String(controller.cards.length)}
          detail="Stored memory entries for this agent."
        />
        <SummaryCard
          icon={<Network size={16} />}
          label="Graph"
          value={String(controller.graphNodes.length)}
          detail={
            controller.graphMapResponse?.data.truncated
              ? "Graph preview (truncated due to size)."
              : "Knowledge graph overview."
          }
        />
        <SummaryCard
          icon={<Activity size={16} />}
          label="Telemetry"
          value={String(controller.telemetryTurns.length)}
          detail="Recent interactions you can inspect."
        />
        <SummaryCard
          icon={<Users size={16} />}
          label="Linked people"
          value={String(laneSummaries.length)}
          detail="People currently routed to this assistant."
        />
      </div>

      <div className="mc-page-section-tabs" aria-label="Memory sections">
        {[
          ["routing", "Routing"],
          ["library", "Library"],
          ["episodes", "Episodes"],
          ["graph", "Graph"],
          ["detail", "Details"],
          ["explain", "Reasoning"],
          ["health", "Health"],
        ].map(([value, label]) => (
          <button
            key={value}
            type="button"
            className={`mc-page-section-btn${
              activeSection === value ? " mc-page-section-btn-active" : ""
            }`}
            onClick={() =>
              setActiveSection(
                value as
                  | "routing"
                  | "library"
                  | "episodes"
                  | "graph"
                  | "detail"
                  | "explain"
                  | "health"
              )
            }
          >
            {label}
          </button>
        ))}
      </div>

      {activeSection === "routing" ? (
        <>
          <Surface
            className="mc-memory-panel mc-memory-lane-map"
            title="Runtime Memory Defaults"
            subtitle="These defaults apply to every lane unless you override them for a specific person."
          >
            <div className="mc-memory-chip-row">
              <Chip label={runtimeDefaultsModeLabel} tone="up" />
              <Chip
                label={`${runtimeDefaultSources.length} runtime local source${
                  runtimeDefaultSources.length === 1 ? "" : "s"
                }`}
                tone={runtimeDefaultSources.length > 0 ? "up" : "checking"}
              />
              <Chip
                label={
                  controller.runtimeMemoryConfig
                    ? "runtime memory loaded"
                    : "runtime memory not loaded yet"
                }
                tone={controller.runtimeMemoryConfig ? "up" : "checking"}
              />
            </div>
            <div className="mc-field-grid">
              <label className="mc-memory-field">
                <span>Default memory mode</span>
                <select
                  value={runtimeModeValue}
                  onChange={(event) => setRuntimeModeDraft(event.target.value)}
                  disabled={controller.runtimeMemorySavePending}
                >
                  {RUNTIME_MEMORY_MODE_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="mc-memory-field">
                <span>Runtime local files</span>
                <textarea
                  rows={4}
                  value={runtimeSourceDraftValue}
                  onChange={(event) => setRuntimeSourceDraft(event.target.value)}
                  disabled={controller.runtimeMemorySavePending}
                  placeholder="Optional support files, one path per line. Use this for shared memory.md files, operator notes, or docs that every lane should inherit."
                />
              </label>
            </div>
            <p className="mc-memory-lane-note">{runtimeDefaultsHelperText}</p>
            <div className="mc-memory-inline-actions">
              <button
                type="button"
                className="ghost"
                onClick={() => {
                  setRuntimeModeDraft(null);
                  setRuntimeSourceDraft(null);
                }}
                disabled={!runtimeDefaultsDirty || controller.runtimeMemorySavePending}
              >
                Reset
              </button>
              <button
                type="button"
                className="ghost"
                onClick={() => {
                  void controller.syncRuntimeMemoryDefaults();
                }}
                disabled={!canSyncRuntimeFiles || controller.memorySyncPendingKey === "runtime"}
              >
                {controller.memorySyncPendingKey === "runtime"
                  ? "Syncing..."
                  : "Sync runtime files now"}
              </button>
              <button
                type="button"
                className="ghost"
                onClick={() => {
                  void controller
                    .saveRuntimeMemoryDefaults(runtimeModeValue, runtimeSourceDraftItems)
                    .then((ok) => {
                      if (!ok) {
                        return;
                      }
                      setRuntimeModeDraft(null);
                      setRuntimeSourceDraft(null);
                    });
                }}
                disabled={!runtimeDefaultsDirty || controller.runtimeMemorySavePending}
              >
                {controller.runtimeMemorySavePending ? "Saving..." : "Save runtime defaults"}
              </button>
            </div>
          </Surface>

          <Surface
            className="mc-memory-panel mc-memory-lane-map"
            title="Lane Map"
            subtitle="Who routes to this assistant and what kind of memory each lane is allowed to use."
          >
            <div className="mc-memory-chip-row">
              <Chip
                label={controller.routingConfig?.enabled ? "routing on" : "routing off"}
                tone={controller.routingConfig?.enabled ? "up" : "checking"}
              />
              <Chip
                label={
                  controller.routingConfig?.use_channel_defaults_as_fallback
                    ? "channel fallback on"
                    : "channel fallback off"
                }
                tone={
                  controller.routingConfig?.use_channel_defaults_as_fallback ? "warning" : "up"
                }
              />
              <Chip
                label={`${laneSummaries.length} lane${laneSummaries.length === 1 ? "" : "s"}`}
              />
            </div>
            <p className="mc-memory-lane-note">
              Lane local files are synced into the local-memory store. Save first, then sync, and
              carsinOS will use that refreshed local context alongside shared memory when the lane mode
              allows it.
            </p>
            {controller.routingLoading ? (
              <EmptyState
                className="mc-memory-empty"
                message="Loading the lane routing snapshot for this assistant."
              />
            ) : controller.routingError ? (
              <EmptyState className="mc-memory-empty" message={controller.routingError} />
            ) : !controller.routingConfig?.enabled ? (
              <div className="mc-memory-warning">
                <ShieldAlert size={14} />
                <span>
                  Lane routing is off. This assistant is still using the older assistant-bound
                  behavior.
                </span>
              </div>
            ) : laneSummaries.length === 0 ? (
              <div className="mc-empty-drawer mc-empty-drawer-stack">
                <span>No linked people currently route to this assistant.</span>
                <span className="mc-memory-lane-note">
                  When routing links exist, this panel will show who talks to this assistant and
                  what memory mode each person gets.
                </span>
              </div>
            ) : (
              <div className="mc-memory-lane-card-grid">
                {controller.laneStatusError ? (
                  <div className="mc-memory-warning">
                    <ShieldAlert size={14} />
                    <span>{controller.laneStatusError}</span>
                  </div>
                ) : null}
                {laneSummaries.map((lane) => {
                  const laneKey = `${lane.humanIdentityId}:${controller.selectedAgentId}`;
                  const draftMode = laneModeDrafts[laneKey] ?? lane.memoryMode;
                  const draftSourcesText =
                    laneSourceDrafts[laneKey] ?? joinLocalSourcesForDraft(lane.localSources);
                  const draftSources = normalizeLocalSourceDraft(draftSourcesText);
                  const isDirty =
                    draftMode !== lane.memoryMode ||
                    draftSources.join("\n") !== lane.localSources.join("\n");
                  const isSaving = controller.lanePolicySaveKey === laneKey;
                  const laneSyncKey = `lane:${lane.humanIdentityId}:${controller.selectedAgentId}`;
                  const isSyncing = controller.memorySyncPendingKey === laneSyncKey;
                  const effectiveSavedSourceCount =
                    lane.localSourceCount > 0
                      ? lane.localSourceCount
                      : lane.usesRuntimeDefault
                        ? controller.runtimeMemorySourceCount
                        : 0;
                  const canSyncLaneFiles = !isDirty && effectiveSavedSourceCount > 0;
                  const laneStatus = laneStatusByHuman.get(lane.humanIdentityId) ?? null;
                  const runtimeLabel = laneStatus
                    ? laneRuntimeStatusLabel(laneStatus.status)
                    : controller.laneStatusLoading
                      ? "Checking live status"
                      : "Status unknown";
                  const runtimeTone = toneForLaneRuntimeStatus(
                    laneStatus?.status ?? (controller.laneStatusLoading ? "not_started" : "")
                  );
                  const runtimeSource = laneStatus
                    ? laneRuntimeSourceLabel(laneStatus.source)
                    : "source unknown";
                  const runtimeDetail =
                    laneStatus?.detail ??
                    (controller.laneStatusLoading
                      ? "Checking whether this lane is using shared memory, local memory, or is still waiting for first use."
                      : "Lane runtime status has not loaded yet.");
                  return (
                    <article
                      key={`${lane.humanIdentityId}:${controller.selectedAgentId}`}
                      className="mc-memory-lane-card"
                    >
                      <div className="mc-memory-lane-card-head">
                        <div>
                          <strong>{lane.displayName}</strong>
                          <p>{lane.humanIdentityId}</p>
                        </div>
                        <Chip label={lane.memoryLabel} tone={lane.memoryTone} />
                      </div>
                      <div className="mc-memory-chip-row">
                        <Chip
                          label={lane.usesRuntimeDefault ? "runtime default" : "lane policy set"}
                          tone={lane.usesRuntimeDefault ? "checking" : "up"}
                        />
                        <Chip
                          label={
                            lane.localSourceCount > 0
                              ? `${lane.localSourceCount} lane source${
                                  lane.localSourceCount === 1 ? "" : "s"
                                }`
                              : lane.usesRuntimeDefault && controller.runtimeMemorySourceCount > 0
                                ? `${controller.runtimeMemorySourceCount} runtime source${
                                    controller.runtimeMemorySourceCount === 1 ? "" : "s"
                                  }`
                                : "no local source override"
                          }
                          tone={
                            lane.localSourceCount > 0 || controller.runtimeMemorySourceCount > 0
                              ? "up"
                              : "checking"
                          }
                        />
                      </div>
                      <div className="mc-memory-chip-row">
                        <Chip label={runtimeLabel} tone={runtimeTone} />
                        <Chip label={runtimeSource} tone={laneStatus ? "up" : "checking"} />
                      </div>
                      <p className="mc-memory-lane-note">{runtimeDetail}</p>
                      <dl className="mc-memory-fact-list">
                        <div className="mc-memory-fact-row">
                          <dt>Lane ID</dt>
                          <dd>
                            {laneStatus?.lane_id ??
                              lane.laneId ??
                              "auto-generated from human + assistant"}
                          </dd>
                        </div>
                        <div className="mc-memory-fact-row">
                          <dt>Effective mode</dt>
                          <dd>
                            {laneStatus
                              ? memoryModeLabel(laneStatus.effective_memory_mode)
                              : lane.memoryLabel}
                          </dd>
                        </div>
                        <div className="mc-memory-fact-row">
                          <dt>Links</dt>
                          <dd>
                            {lane.linkLabels.length > 0
                              ? lane.linkLabels.join(" • ")
                              : "No Discord or Telegram identities linked yet."}
                          </dd>
                        </div>
                      </dl>
                      <label className="mc-memory-field">
                        <span>Memory mode</span>
                        <select
                          value={draftMode}
                          onChange={(event) =>
                            setLaneModeDrafts((current) => ({
                              ...current,
                              [laneKey]: event.target.value,
                            }))
                          }
                          disabled={isSaving}
                        >
                          {MEMORY_MODE_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="mc-memory-field">
                        <span>Lane local files</span>
                        <textarea
                          rows={4}
                          value={draftSourcesText}
                          onChange={(event) =>
                            setLaneSourceDrafts((current) => ({
                              ...current,
                              [laneKey]: event.target.value,
                            }))
                          }
                          disabled={isSaving}
                          placeholder={
                            lane.usesRuntimeDefault && controller.runtimeMemorySourceCount > 0
                              ? "Optional lane-specific file paths. Leave blank to keep using the runtime default local sources."
                              : "Optional local file paths, one per line. Use this when this person needs supporting docs or memory.md files beyond the shared runtime default."
                          }
                        />
                      </label>
                      <p className="mc-memory-lane-note">
                        Shared memory stays the long-term memory truth here. Local files act as support
                        material when the mode includes local memory. Leave this blank to inherit
                        the runtime defaults above.
                      </p>
                      <div className="mc-memory-inline-actions">
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => {
                            setLaneModeDrafts((current) => ({
                              ...current,
                              [laneKey]: lane.memoryMode,
                            }));
                            setLaneSourceDrafts((current) => ({
                              ...current,
                              [laneKey]: joinLocalSourcesForDraft(lane.localSources),
                            }));
                          }}
                          disabled={!isDirty || isSaving}
                        >
                          Reset
                        </button>
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => {
                            void controller.syncLaneMemorySources(
                              lane.humanIdentityId,
                              controller.selectedAgentId
                            );
                          }}
                          disabled={!canSyncLaneFiles || isSyncing || isSaving}
                        >
                          {isSyncing ? "Syncing..." : "Sync lane files"}
                        </button>
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => {
                            void controller
                              .saveLaneMemoryPolicy(
                                lane.humanIdentityId,
                                controller.selectedAgentId,
                                draftMode,
                                { localMemorySources: draftSources }
                              )
                              .then((ok) => {
                                if (!ok) {
                                  return;
                                }
                                setLaneModeDrafts((current) => {
                                  const next = { ...current };
                                  delete next[laneKey];
                                  return next;
                                });
                                setLaneSourceDrafts((current) => {
                                  const next = { ...current };
                                  delete next[laneKey];
                                  return next;
                                });
                              });
                          }}
                          disabled={!isDirty || isSaving}
                        >
                          {isSaving ? "Saving..." : "Save lane settings"}
                        </button>
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
          </Surface>
        </>
      ) : null}

      {activeSection === "library" ? (
        <div className="mc-memory-grid">
        <Surface
          className="mc-memory-panel mc-memory-sidebar"
          title="Agent Memory"
          subtitle="Each agent has its own separate memory. Select an agent to inspect."
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
            <Chip label={sidebarLaneLabel} tone={sidebarLaneTone} />
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
                Memory health status doesn't match the overall system health.
                The system-level health check takes priority.
              </span>
            </div>
          ) : null}
          {bindingStatus === "degraded" ? (
            <div className="mc-memory-warning">
              <ShieldAlert size={14} />
              <span>
                This agent's memory connection is degraded. Some features may be limited,
                but available data is still shown.
              </span>
            </div>
          ) : null}
          {isUnavailable ? (
            <div className="mc-memory-warning">
              <ShieldAlert size={14} />
              <span>
                {bindingStatus === "unconfigured"
                  ? "This agent doesn't have memory set up yet."
                  : bindingStatus === "unauthorized"
                    ? "Memory exists for this agent, but authentication is missing or expired."
                    : "Memory exists for this agent, but the connection is currently unavailable."}
              </span>
            </div>
          ) : null}
        </Surface>

        <Surface
          className="mc-memory-panel mc-memory-sidebar"
          title="Memory Cards"
          subtitle="Browse stored facts and knowledge entries for this agent."
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
            <SurfaceLockState message="Memory cards are not available for this agent." />
          ) : controller.cards.length === 0 ? (
            <div className="mc-empty-drawer mc-empty-drawer-stack">
              <span>
                {cardFiltersActive
                  ? "No memory cards match your current filters."
                  : "No memory cards saved for this agent yet."}
              </span>
              {cardFiltersActive ? (
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    controller.setCardQuery("");
                    controller.setCardStatusFilter("all");
                  }}
                >
                  Clear filters
                </button>
              ) : null}
            </div>
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
        </div>
      ) : null}

      {activeSection === "episodes" ? (
        <div className="mc-memory-grid">
        <Surface
          className="mc-memory-panel mc-memory-sidebar"
          title="Episodes"
          subtitle="Interaction history recorded by the system. Read-only for now."
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
            <SurfaceLockState message="Episodes are not available for this agent." />
          ) : controller.episodes.length === 0 ? (
            <div className="mc-empty-drawer mc-empty-drawer-stack">
              <span>
                {episodeFiltersActive
                  ? "No episodes matched the current search."
                  : "No episodes recorded for this agent yet."}
              </span>
              {episodeFiltersActive ? (
                <button
                  type="button"
                  className="ghost"
                  onClick={() => controller.setEpisodeQuery("")}
                >
                  Clear search
                </button>
              ) : null}
            </div>
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
        </div>
      ) : null}

      {activeSection === "graph" ? (
        <div className="mc-memory-grid">
        <Surface
          className="mc-memory-panel mc-memory-atlas"
          title="Knowledge Graph"
          subtitle="How this agent's facts and concepts are connected."
        >
          {!controller.nativeSurfaceAvailability.graph_overview || !controller.canRead ? (
            <SurfaceLockState message="Knowledge graph is not available for this agent." />
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
          title="Related Concepts"
          subtitle="Facts and concepts connected to the selected item."
        >
          {!controller.nativeSurfaceAvailability.graph_neighbors || !controller.canRead ? (
            <SurfaceLockState message="Related concepts view is not available for this agent." />
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
        </div>
      ) : null}

      {activeSection === "detail" ? (
        <div className="mc-memory-grid">
        <Surface
          className="mc-memory-panel mc-memory-detail"
          title="Details"
          subtitle="Detailed view of the selected memory entry. Read-only."
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
        </div>
      ) : null}

      {activeSection === "explain" ? (
        <div className="mc-memory-grid">
        <Surface
          className="mc-memory-panel mc-memory-detail"
          title="Reasoning + Sources"
          subtitle="Understand why the agent made a decision and what evidence it used."
        >
          {!controller.nativeSurfaceAvailability.turn_why || !controller.canRead ? (
            <SurfaceLockState message="Reasoning and source inspection is not available for this agent." />
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
        </div>
      ) : null}

      {activeSection === "health" ? (
        <div className="mc-memory-grid">
        <Surface
          className="mc-memory-panel mc-memory-detail"
          title="Health + Diagnostics"
          subtitle="System health and diagnostic information for this agent's memory."
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
                <SurfaceLockState message="Diagnostic summary is not available for this agent." />
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
                <SurfaceLockState message="Decision history is not available for this agent." />
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
      ) : null}
    </section>
  );
}
