import { useDeferredValue, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Compass,
  GitBranch,
  HeartPulse,
  PackageOpen,
  RefreshCw,
  ShieldAlert,
  Upload,
} from "lucide-react";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { Modal } from "../../ui/Modal";
import { Surface } from "../../ui/Surface";
import { formatRelative } from "../../utils/datetime";
import type {
  ChannelConfigResponse,
  ChannelRuntimeAdapterStatusResponse,
  ConnectorPublishedToolResponse,
  GetDiscordPairingStatusResponse,
  GetTelegramPairingStatusResponse,
  RuntimeChannelsConfigResponse,
  RuntimeRoutingConfigResponse,
} from "../../types";
import {
  approveDiscordPairing,
  approveTelegramPairing,
  denyDiscordPairing,
  denyTelegramPairing,
  getChannelConfig,
  getDiscordPairingStatus,
  getChannelRuntimeStatus,
  getRuntimeConfig,
  getTelegramPairingStatus,
} from "../../lib/api";
import {
  CONNECTOR_ASSIGNMENT_AUTH_MODE_OPTIONS,
  CONNECTOR_AUTH_KIND_OPTIONS,
  CONNECTOR_AUTH_STATUS_OPTIONS,
  CONNECTOR_EXTERNAL_REFERENCE_POLICY_OPTIONS,
  CONNECTOR_JSON_TEXTAREA_ROWS,
  CONNECTOR_INTERACTION_PAYLOAD_PLACEHOLDER,
  CONNECTOR_ORIGIN_KIND_OPTIONS,
  CONNECTOR_SOURCE_KIND_OPTIONS,
  CONNECTOR_TEXTAREA_ROWS,
} from "./connectorsConfig";
import "./connectors.css";
import { stringifyJson } from "./connectorsModel";
import {
  SIMPLE_INTEGRATIONS,
  SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT,
  type SimpleIntegrationId,
} from "./simpleIntegrations";
import { deriveSimpleIntegrationStatuses } from "./simpleIntegrationStatus";
import type { useConnectorsController } from "./useConnectorsController";

interface ConnectorsPageProps {
  controller: ReturnType<typeof useConnectorsController>;
  onOpenSimpleIntegrationWizard: (integrationId: SimpleIntegrationId) => void;
}

function humanize(value: string | null | undefined): string {
  if (!value) {
    return "n/a";
  }
  return value.replaceAll("_", " ").replaceAll("-", " ");
}

function formatTimestamp(value: number | null | undefined): string {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "n/a";
  }
  return formatRelative(value);
}

function toneForStatus(status: string | null | undefined): "up" | "down" | "warning" | "checking" | "" {
  switch (status) {
    case "enabled":
    case "ready":
    case "succeeded":
    case "trusted_curated":
    case "reviewed_local":
    case "active":
      return "up";
    case "disabled":
    case "error":
    case "failed":
    case "blocked":
    case "expired":
    case "unsafe_blocked":
    case "cancelled":
    case "unpublished":
      return "down";
    case "under_review":
    case "converted":
    case "pending":
    case "waiting_on_operator":
    case "operator_write_gated":
    case "destructive_write_gated":
    case "superseded":
      return "warning";
    default:
      return status ? "checking" : "";
  }
}

function toneForHealth(
  status: string | null | undefined,
  degradedReason: string | null | undefined,
  authRequired: boolean
): "up" | "down" | "warning" | "checking" | "" {
  if (status === "enabled" && !degradedReason && !authRequired) {
    return "up";
  }
  if (status === "disabled" || status === "error") {
    return "down";
  }
  if (degradedReason || authRequired) {
    return "warning";
  }
  return status ? "checking" : "";
}

function connectorToolAuthRequired(tool: ConnectorPublishedToolResponse): boolean {
  const metadata = tool.origin_metadata;
  if (!metadata || typeof metadata !== "object" || Array.isArray(metadata)) {
    return false;
  }
  return (metadata as { auth_required?: unknown }).auth_required === true;
}

function ConnectorsStatePanel({
  title,
  detail,
}: {
  title: string;
  detail: string;
}) {
  return (
    <section className="mc-connectors-page" data-testid="connectors-page">
      <Surface className="mc-connectors-state" title={title} subtitle={detail}>
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
    <div className="mc-connectors-summary-card">
      <div className="mc-connectors-summary-kicker">
        {icon}
        <span>{label}</span>
      </div>
      <strong>{value}</strong>
      <p>{detail}</p>
    </div>
  );
}

function ToolSelectionRow({
  tool,
  selected,
  checked,
  onSelect,
  onToggle,
}: {
  tool: ConnectorPublishedToolResponse;
  selected: boolean;
  checked: boolean;
  onSelect: () => void;
  onToggle: () => void;
}) {
  return (
    <div className={`mc-connectors-tool-row${selected ? " is-selected" : ""}`}>
      <label className="mc-connectors-tool-check">
        <input
          type="checkbox"
          checked={checked}
          onChange={onToggle}
          aria-label={`Select published tool ${tool.display_name}`}
        />
      </label>
      <button type="button" className="mc-connectors-tool-button" onClick={onSelect}>
        <div className="mc-connectors-tool-head">
          <strong>{tool.display_name}</strong>
          <div className="mc-connectors-inline-chips">
            <Chip label={humanize(tool.deprecation_state)} tone={toneForStatus(tool.deprecation_state)} />
            <Chip
              label={humanize(tool.write_classification)}
              tone={toneForStatus(tool.write_classification)}
            />
            {connectorToolAuthRequired(tool) ? <Chip label="Auth required" tone="warning" /> : null}
          </div>
        </div>
        <div className="mc-connectors-tool-meta">
          <span>{tool.tool_name}</span>
          <span>{formatTimestamp(tool.published_at)}</span>
        </div>
      </button>
    </div>
  );
}

/* ── Pagination constants ── */

type ConnectorsTab = "setup" | "catalog" | "import" | "registry" | "manage";
type ManageSubTab = "overview" | "review" | "auth" | "health";

const CATALOG_PER_PAGE = 6;
const REGISTRY_PER_PAGE = 6;
const CANDIDATES_PER_PAGE = 4;
const TOOLS_PER_PAGE = 5;

export function ConnectorsPage({
  controller,
  onOpenSimpleIntegrationWizard,
}: ConnectorsPageProps) {
  const [activeTab, setActiveTab] = useState<ConnectorsTab>("setup");
  const [manageSubTab, setManageSubTab] = useState<ManageSubTab>("overview");
  const [catalogPage, setCatalogPage] = useState(0);
  const [registryPage, setRegistryPage] = useState(0);
  const [candidatePage, setCandidatePage] = useState(0);
  const [toolsPage, setToolsPage] = useState(0);
  const [catalogQuery, setCatalogQuery] = useState("");
  const [registryQuery, setRegistryQuery] = useState("");
  const [confirmAction, setConfirmAction] = useState<"publish" | "unpublish" | null>(null);
  const [setupRuntimeChannels, setSetupRuntimeChannels] =
    useState<RuntimeChannelsConfigResponse | null>(null);
  const [setupRouting, setSetupRouting] = useState<RuntimeRoutingConfigResponse | null>(null);
  const [setupChannelConfig, setSetupChannelConfig] =
    useState<ChannelConfigResponse | null>(null);
  const [setupChannelStatuses, setSetupChannelStatuses] = useState<
    ChannelRuntimeAdapterStatusResponse[]
  >([]);
  const [discordPairingStatus, setDiscordPairingStatus] =
    useState<GetDiscordPairingStatusResponse | null>(null);
  const [discordPairingBusyCode, setDiscordPairingBusyCode] = useState<string | null>(null);
  const [telegramPairingStatus, setTelegramPairingStatus] =
    useState<GetTelegramPairingStatusResponse | null>(null);
  const [telegramPairingBusyCode, setTelegramPairingBusyCode] = useState<string | null>(null);
  const [pairingHumanDrafts, setPairingHumanDrafts] = useState<Record<string, string>>({});
  const [setupStatusError, setSetupStatusError] = useState<string | null>(null);
  const deferredCatalogQuery = useDeferredValue(catalogQuery.trim().toLowerCase());
  const deferredRegistryQuery = useDeferredValue(registryQuery.trim().toLowerCase());
  const isBusy = Boolean(controller.mutatingAction);

  const agentsById = useMemo(
    () => new Map(controller.agents.map((agent) => [agent.agent_id, agent] as const)),
    [controller.agents]
  );
  const routingHumans = useMemo(
    () => (setupRouting?.human_identities ?? []).filter((item) => item.enabled),
    [setupRouting]
  );
  const localOperatorHumanId = useMemo(() => {
    const candidate = setupRouting?.local_operator_human_identity_id?.trim();
    return candidate ? candidate : "";
  }, [setupRouting]);

  const filteredCatalog = useMemo(() => {
    return controller.catalog.filter((item) => {
      if (!deferredCatalogQuery) {
        return true;
      }
      const haystack = [
        item.display_name,
        item.summary,
        item.publisher,
        item.slug,
        item.source_kind,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(deferredCatalogQuery);
    });
  }, [controller.catalog, deferredCatalogQuery]);

  const filteredConnectors = useMemo(() => {
    return controller.installedConnectors.filter((item) => {
      if (!deferredRegistryQuery) {
        return true;
      }
      const haystack = [
        item.display_name,
        item.slug,
        item.source_kind,
        item.status,
        item.trust_state,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(deferredRegistryQuery);
    });
  }, [controller.installedConnectors, deferredRegistryQuery]);

  const totalPublishedTools = controller.installedConnectors.reduce(
    (sum, item) => sum + item.published_tool_count,
    0
  );
  const sharedBinding =
    controller.selectedConnectorDetail?.auth_bindings.find((item) => !item.agent_id) ?? null;
  const toolDetail =
    controller.selectedToolDetail?.published_tool ?? controller.selectedPublishedTool ?? null;
  const simpleIntegrationStatuses = useMemo(
    () =>
      deriveSimpleIntegrationStatuses(
        SIMPLE_INTEGRATIONS,
        controller.agents,
        setupRuntimeChannels,
        setupChannelConfig,
        setupChannelStatuses
      ),
    [controller.agents, setupChannelConfig, setupChannelStatuses, setupRuntimeChannels]
  );
  const setupStatusLoading = !setupRuntimeChannels || !setupChannelConfig;
  const showDiscordAccessRequests = Boolean(
    discordPairingStatus &&
      (discordPairingStatus.pending_requests.length > 0 ||
        discordPairingStatus.blocked_senders.length > 0 ||
        discordPairingStatus.dm_policy === "approval_required")
  );
  const showTelegramAccessRequests = Boolean(
    telegramPairingStatus &&
      (telegramPairingStatus.pending_requests.length > 0 ||
        telegramPairingStatus.blocked_senders.length > 0 ||
        telegramPairingStatus.dm_policy === "pairing")
  );

  useEffect(() => {
    let cancelled = false;

    const loadSetupStatus = async () => {
      const [
        runtimeResult,
        channelConfigResult,
        runtimeStatusResult,
        discordPairingResult,
        telegramPairingResult,
      ] =
        await Promise.allSettled([
          getRuntimeConfig(controller.settings),
          getChannelConfig(controller.settings),
          getChannelRuntimeStatus(controller.settings),
          getDiscordPairingStatus(controller.settings),
          getTelegramPairingStatus(controller.settings),
        ]);

      if (cancelled) {
        return;
      }

      let nextError: string | null = null;

      if (runtimeResult.status === "fulfilled") {
        setSetupRuntimeChannels(runtimeResult.value.config.channels);
        setSetupRouting(runtimeResult.value.config.routing);
      } else {
        setSetupRouting(null);
        nextError =
          "Some live connection details could not be loaded. Saved setup may still be shown below.";
      }

      if (channelConfigResult.status === "fulfilled") {
        setSetupChannelConfig(channelConfigResult.value.config);
      } else {
        nextError =
          "Some live connection details could not be loaded. Saved setup may still be shown below.";
      }

      if (runtimeStatusResult.status === "fulfilled") {
        setSetupChannelStatuses(runtimeStatusResult.value.items);
      } else {
        nextError =
          "Some live connection details could not be loaded. Saved setup may still be shown below.";
      }

      if (discordPairingResult.status === "fulfilled") {
        setDiscordPairingStatus(discordPairingResult.value);
      } else {
        setDiscordPairingStatus(null);
      }

      if (telegramPairingResult.status === "fulfilled") {
        setTelegramPairingStatus(telegramPairingResult.value);
      } else {
        setTelegramPairingStatus(null);
      }

      setSetupStatusError(nextError);
    };

    void loadSetupStatus().catch(() => {
      if (cancelled) {
        return;
      }
      setSetupStatusError(
        "Some live connection details could not be loaded. Saved setup may still be shown below."
      );
    });

    const handleStatusUpdated = () => {
      void loadSetupStatus().catch(() => {
        if (cancelled) {
          return;
        }
        setSetupStatusError(
          "Some live connection details could not be loaded. Saved setup may still be shown below."
        );
      });
    };

    window.addEventListener(SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT, handleStatusUpdated);

    return () => {
      cancelled = true;
      window.removeEventListener(
        SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT,
        handleStatusUpdated
      );
    };
  }, [controller.settings, controller.mutatingAction]);

  useEffect(() => {
    const pendingKeys = [
      ...(discordPairingStatus?.pending_requests.map((item) => `discord:${item.code}`) ?? []),
      ...(telegramPairingStatus?.pending_requests.map((item) => `telegram:${item.code}`) ?? []),
    ];
    setPairingHumanDrafts((current) => {
      const next: Record<string, string> = {};
      for (const key of pendingKeys) {
        next[key] = current[key] ?? "";
      }
      return next;
    });
  }, [discordPairingStatus, telegramPairingStatus]);

  if (!controller.enabled || controller.availability === "disabled") {
    return (
      <ConnectorsStatePanel
        title="Connectors are disabled"
        detail="Enable Connectors in Config > Reliability + Rollout to expose connector intake, review, and auth controls."
      />
    );
  }

  if (controller.availability === "unsupported") {
    return (
      <ConnectorsStatePanel
        title="Connectors surface unavailable"
        detail={
          controller.availabilityMessage ??
          "The connected gateway does not expose the Connectors surface yet."
        }
      />
    );
  }

  if (controller.availability === "error") {
    return (
      <ConnectorsStatePanel
        title="Connectors failed to load"
        detail={controller.availabilityMessage ?? "Connectors could not load."}
      />
    );
  }

  if (controller.availability === "loading" && controller.installedConnectors.length === 0) {
    return (
      <ConnectorsStatePanel
        title="Loading Connectors"
        detail="Resolving catalog intake, installed registry state, and paused connector interactions."
      />
    );
  }

  /* ── Catalog pagination ── */
  const totalCatalogPages = Math.max(1, Math.ceil(filteredCatalog.length / CATALOG_PER_PAGE));
  const safeCatalogPage = Math.min(catalogPage, totalCatalogPages - 1);
  if (safeCatalogPage !== catalogPage) setCatalogPage(safeCatalogPage);
  const pagedCatalog = filteredCatalog.slice(
    safeCatalogPage * CATALOG_PER_PAGE,
    (safeCatalogPage + 1) * CATALOG_PER_PAGE
  );

  /* ── Registry pagination ── */
  const totalRegistryPages = Math.max(1, Math.ceil(filteredConnectors.length / REGISTRY_PER_PAGE));
  const safeRegistryPage = Math.min(registryPage, totalRegistryPages - 1);
  if (safeRegistryPage !== registryPage) setRegistryPage(safeRegistryPage);
  const pagedConnectors = filteredConnectors.slice(
    safeRegistryPage * REGISTRY_PER_PAGE,
    (safeRegistryPage + 1) * REGISTRY_PER_PAGE
  );

  /* ── Candidates pagination ── */
  const candidates = controller.selectedConversion?.proposed_tools ?? [];
  const totalCandidatePages = Math.max(1, Math.ceil(candidates.length / CANDIDATES_PER_PAGE));
  const safeCandidatePage = Math.min(candidatePage, totalCandidatePages - 1);
  if (safeCandidatePage !== candidatePage) setCandidatePage(safeCandidatePage);
  const pagedCandidates = candidates.slice(
    safeCandidatePage * CANDIDATES_PER_PAGE,
    (safeCandidatePage + 1) * CANDIDATES_PER_PAGE
  );

  /* ── Published tools pagination ── */
  const publishedTools = controller.selectedConnectorDetail?.published_tools ?? [];
  const totalToolsPages = Math.max(1, Math.ceil(publishedTools.length / TOOLS_PER_PAGE));
  const safeToolsPage = Math.min(toolsPage, totalToolsPages - 1);
  if (safeToolsPage !== toolsPage) setToolsPage(safeToolsPage);
  const pagedTools = publishedTools.slice(
    safeToolsPage * TOOLS_PER_PAGE,
    (safeToolsPage + 1) * TOOLS_PER_PAGE
  );

  const openManageTab = (connectorId: string) => {
    controller.selectConnector(connectorId);
    setActiveTab("manage");
    setManageSubTab("overview");
    setCandidatePage(0);
    setToolsPage(0);
  };

  const pairingHumanKey = (provider: "discord" | "telegram", code: string) =>
    `${provider}:${code}`;

  const resolveTelegramPairing = async (code: string, action: "approve" | "deny") => {
    setTelegramPairingBusyCode(code);
    try {
      const humanIdentityId = pairingHumanDrafts[pairingHumanKey("telegram", code)] ?? "";
      if (action === "approve" && !humanIdentityId.trim()) {
        return;
      }
      const response =
        action === "approve"
          ? await approveTelegramPairing(controller.settings, code, humanIdentityId)
          : await denyTelegramPairing(controller.settings, code, humanIdentityId);
      setTelegramPairingStatus(response.status);
      window.dispatchEvent(new CustomEvent(SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT));
    } finally {
      setTelegramPairingBusyCode(null);
    }
  };

  const resolveDiscordPairing = async (code: string, action: "approve" | "deny") => {
    setDiscordPairingBusyCode(code);
    try {
      const humanIdentityId = pairingHumanDrafts[pairingHumanKey("discord", code)] ?? "";
      if (action === "approve" && !humanIdentityId.trim()) {
        return;
      }
      const response =
        action === "approve"
          ? await approveDiscordPairing(controller.settings, code, humanIdentityId)
          : await denyDiscordPairing(controller.settings, code, humanIdentityId);
      setDiscordPairingStatus(response.status);
      window.dispatchEvent(new CustomEvent(SIMPLE_INTEGRATION_STATUS_UPDATED_EVENT));
    } finally {
      setDiscordPairingBusyCode(null);
    }
  };

  return (
    <section className="mc-connectors-page mc-connectors-paged" data-testid="connectors-page">
      <nav className="mc-connectors-tab-bar">
        <button
          type="button"
          className={activeTab === "setup" ? "active" : ""}
          onClick={() => setActiveTab("setup")}
        >
          Setup
        </button>
        <button
          type="button"
          className={activeTab === "catalog" ? "active" : ""}
          onClick={() => { setActiveTab("catalog"); setCatalogPage(0); }}
        >
          Catalog ({controller.catalog.length})
        </button>
        <button
          type="button"
          className={activeTab === "import" ? "active" : ""}
          onClick={() => setActiveTab("import")}
        >
          Import
        </button>
        <button
          type="button"
          className={activeTab === "registry" ? "active" : ""}
          onClick={() => { setActiveTab("registry"); setRegistryPage(0); }}
        >
          Registry ({controller.installedConnectors.length})
        </button>
        <button
          type="button"
          className={`${activeTab === "manage" ? "active" : ""}${!controller.selectedConnector ? " is-disabled" : ""}`}
          disabled={!controller.selectedConnector}
          onClick={() => setActiveTab("manage")}
        >
          Manage{controller.selectedConnector ? `: ${controller.selectedConnector.display_name}` : ""}
        </button>
      </nav>

      {/* ── Setup tab ── */}
      {activeTab === "setup" ? (
        <div className="mc-connectors-tab-content">
          <Surface
            className="mc-connectors-panel mc-connectors-quick-setup"
            title="Quick Setup"
            subtitle="Use these for common chat integrations when you want one agent wired up fast, then check the status section below to see what is merely saved versus truly ready for live traffic."
          >
            {setupStatusError ? (
              <p className="mc-connectors-soft-warning">{setupStatusError}</p>
            ) : null}
            <div className="mc-connectors-quick-setup-grid">
              {SIMPLE_INTEGRATIONS.map((integration) => {
                const statusCard =
                  simpleIntegrationStatuses.find((item) => item.id === integration.id) ?? null;
                return (
                  <button
                    type="button"
                    key={integration.id}
                    className="mc-connectors-quick-card"
                    onClick={() => onOpenSimpleIntegrationWizard(integration.id)}
                  >
                    <div className="mc-connectors-card-head">
                      <strong>{integration.displayName}</strong>
                      <Chip
                        label={statusCard?.statusLabel ?? integration.statusLabel}
                        tone={
                          statusCard?.tone ??
                          (integration.setupMode === "channel_runtime" ? "up" : "warning")
                        }
                      />
                    </div>
                    <p>{statusCard?.summary ?? integration.shortDescription}</p>
                    <div className="mc-connectors-quick-status">
                      <div className="mc-connectors-quick-status-row">
                        <span>Current status</span>
                        <strong>
                          {setupStatusLoading && !statusCard
                            ? "Loading…"
                            : statusCard?.runtimeLabel ?? "loading"}
                        </strong>
                      </div>
                      <div className="mc-connectors-quick-status-row">
                        <span>Assigned agent</span>
                        <strong>{statusCard?.assignedAgentLabel ?? "Loading…"}</strong>
                      </div>
                    </div>
                    <p className="mc-connectors-quick-detail">
                      {statusCard?.detail ?? integration.plainLanguage}
                    </p>
                    <div className="mc-connectors-quick-action">
                      <span>What to do now</span>
                      <strong>{statusCard?.launchLabel ?? integration.launchNextStepLabel}</strong>
                    </div>
                  </button>
                );
              })}
            </div>
          </Surface>

          {showDiscordAccessRequests || showTelegramAccessRequests ? (
            <Surface
              className="mc-connectors-panel"
              title="Direct Message Access Requests"
              subtitle="Unknown people stay locked until you link them to the right person record. Approval creates the lane link; there is no silent fallback."
            >
              {showDiscordAccessRequests && discordPairingStatus ? (
                <div className="mc-connectors-access-provider">
                  <div className="mc-connectors-telegram-access-note">
                    <strong>Discord DM policy: {humanize(discordPairingStatus.dm_policy)}</strong>
                    <p>
                      New Discord direct messages stay locked until you choose who that account
                      belongs to and approve the link.
                    </p>
                  </div>
                  {discordPairingStatus.pending_requests.length > 0 ? (
                    <div className="mc-connectors-telegram-pending-list">
                      {discordPairingStatus.pending_requests.map((item) => (
                        <div key={item.code} className="mc-connectors-telegram-pending-card">
                          <div className="mc-connectors-card-head">
                            <strong>Discord approval code {item.code}</strong>
                            <Chip label="Waiting" tone="warning" />
                          </div>
                          <div className="mc-connectors-quick-status">
                            <div className="mc-connectors-quick-status-row">
                              <span>Discord user</span>
                              <strong>{item.user_id}</strong>
                            </div>
                            <div className="mc-connectors-quick-status-row">
                              <span>Chat</span>
                              <strong>{item.channel_id}</strong>
                            </div>
                            <div className="mc-connectors-quick-status-row">
                              <span>Attempts</span>
                              <strong>{item.attempt_count}</strong>
                            </div>
                            <div className="mc-connectors-quick-status-row">
                              <span>Expires</span>
                              <strong>{formatTimestamp(item.expires_at)}</strong>
                            </div>
                          </div>
                          <p className="mc-connectors-quick-detail">
                            First message preview: {item.preview_text || "No preview captured"}
                          </p>
                          {routingHumans.length > 0 ? (
                            <label className="mc-connectors-field mc-connectors-field-span">
                              <span>Route this person to</span>
                              <select
                                value={pairingHumanDrafts[pairingHumanKey("discord", item.code)] ?? ""}
                                onChange={(event) => {
                                  const nextValue = event.target.value;
                                  setPairingHumanDrafts((current) => ({
                                    ...current,
                                    [pairingHumanKey("discord", item.code)]: nextValue,
                                  }));
                                }}
                              >
                                <option value="">Choose person...</option>
                                {routingHumans.map((human) => (
                                  <option
                                    key={human.human_identity_id}
                                    value={human.human_identity_id}
                                  >
                                    {human.display_name}
                                    {human.human_identity_id === localOperatorHumanId
                                      ? " (You)"
                                      : ""}
                                  </option>
                                  ))}
                              </select>
                              <small>
                                Pick the real person first. Approval will create the lane link and
                                future DMs will resume the same assistant lane automatically.
                              </small>
                            </label>
                          ) : null}
                          <div className="mc-connectors-telegram-action-row">
                            <button
                              type="button"
                              disabled={
                                discordPairingBusyCode === item.code ||
                                !pairingHumanDrafts[pairingHumanKey("discord", item.code)]?.trim()
                              }
                              onClick={() => {
                                void resolveDiscordPairing(item.code, "approve");
                              }}
                            >
                              {discordPairingBusyCode === item.code ? "Working..." : "Approve"}
                            </button>
                            <button
                              type="button"
                              className="secondary"
                              disabled={discordPairingBusyCode === item.code}
                              onClick={() => {
                                void resolveDiscordPairing(item.code, "deny");
                              }}
                            >
                              Deny + mute
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="mc-connectors-quick-detail">
                      No pending Discord approvals right now. When someone new DMs the bot, their
                      approval code will appear here.
                    </p>
                  )}
                  {discordPairingStatus.blocked_senders.length > 0 ? (
                    <div className="mc-connectors-telegram-blocked-list">
                      {discordPairingStatus.blocked_senders.map((item) => (
                        <div
                          key={`${item.user_id}-${item.blocked_until}`}
                          className="mc-connectors-list-row"
                        >
                          <div>
                            <div className="mc-connectors-list-title">
                              <strong>Discord user {item.user_id}</strong>
                              <Chip label="Muted" tone="down" />
                            </div>
                            <p>{humanize(item.reason)}</p>
                            <div className="mc-connectors-list-meta">
                              <span>{item.attempt_count} blocked attempts</span>
                              <span>Until {formatTimestamp(item.blocked_until)}</span>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : null}
                </div>
              ) : null}

              {showTelegramAccessRequests && telegramPairingStatus ? (
                <div className="mc-connectors-access-provider">
                  <div className="mc-connectors-telegram-access-note">
                    <strong>Telegram DM policy: {humanize(telegramPairingStatus.dm_policy)}</strong>
                    <p>
                      A first Telegram DM does not reach the agent yet. Choose who owns that
                      account, approve it once, and future messages will resume the same lane.
                    </p>
                  </div>
                  {telegramPairingStatus.pending_requests.length > 0 ? (
                    <div className="mc-connectors-telegram-pending-list">
                      {telegramPairingStatus.pending_requests.map((item) => (
                        <div key={item.code} className="mc-connectors-telegram-pending-card">
                          <div className="mc-connectors-card-head">
                            <strong>Telegram approval code {item.code}</strong>
                            <Chip label="Waiting" tone="warning" />
                          </div>
                          <div className="mc-connectors-quick-status">
                            <div className="mc-connectors-quick-status-row">
                              <span>Telegram user</span>
                              <strong>{item.user_id}</strong>
                            </div>
                            <div className="mc-connectors-quick-status-row">
                              <span>Chat</span>
                              <strong>{item.chat_id}</strong>
                            </div>
                            <div className="mc-connectors-quick-status-row">
                              <span>Attempts</span>
                              <strong>{item.attempt_count}</strong>
                            </div>
                            <div className="mc-connectors-quick-status-row">
                              <span>Expires</span>
                              <strong>{formatTimestamp(item.expires_at)}</strong>
                            </div>
                          </div>
                          <p className="mc-connectors-quick-detail">
                            First message preview: {item.preview_text || "No preview captured"}
                          </p>
                          {routingHumans.length > 0 ? (
                            <label className="mc-connectors-field mc-connectors-field-span">
                              <span>Route this person to</span>
                              <select
                                value={pairingHumanDrafts[pairingHumanKey("telegram", item.code)] ?? ""}
                                onChange={(event) => {
                                  const nextValue = event.target.value;
                                  setPairingHumanDrafts((current) => ({
                                    ...current,
                                    [pairingHumanKey("telegram", item.code)]: nextValue,
                                  }));
                                }}
                              >
                                <option value="">Choose person...</option>
                                {routingHumans.map((human) => (
                                  <option
                                    key={human.human_identity_id}
                                    value={human.human_identity_id}
                                  >
                                    {human.display_name}
                                    {human.human_identity_id === localOperatorHumanId
                                      ? " (You)"
                                      : ""}
                                  </option>
                                  ))}
                              </select>
                              <small>
                                Pick the real person first. Approval will create the lane link and
                                future Telegram messages will resume the same assistant lane.
                              </small>
                            </label>
                          ) : null}
                          <div className="mc-connectors-telegram-action-row">
                            <button
                              type="button"
                              disabled={
                                telegramPairingBusyCode === item.code ||
                                !pairingHumanDrafts[pairingHumanKey("telegram", item.code)]?.trim()
                              }
                              onClick={() => {
                                void resolveTelegramPairing(item.code, "approve");
                              }}
                            >
                              {telegramPairingBusyCode === item.code ? "Working..." : "Approve"}
                            </button>
                            <button
                              type="button"
                              className="secondary"
                              disabled={telegramPairingBusyCode === item.code}
                              onClick={() => {
                                void resolveTelegramPairing(item.code, "deny");
                              }}
                            >
                              Deny + mute
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="mc-connectors-quick-detail">
                      No pending Telegram approvals right now. When someone new messages the bot,
                      their approval code will appear here.
                    </p>
                  )}
                  {telegramPairingStatus.blocked_senders.length > 0 ? (
                    <div className="mc-connectors-telegram-blocked-list">
                      {telegramPairingStatus.blocked_senders.map((item) => (
                        <div
                          key={`${item.user_id}-${item.blocked_until}`}
                          className="mc-connectors-list-row"
                        >
                          <div>
                            <div className="mc-connectors-list-title">
                              <strong>Telegram user {item.user_id}</strong>
                              <Chip label="Muted" tone="down" />
                            </div>
                            <p>{humanize(item.reason)}</p>
                            <div className="mc-connectors-list-meta">
                              <span>{item.attempt_count} blocked attempts</span>
                              <span>Until {formatTimestamp(item.blocked_until)}</span>
                            </div>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : null}
                </div>
              ) : null}
            </Surface>
          ) : null}

          <div className="mc-connectors-summary-strip">
            <SummaryCard
              icon={<Compass size={16} />}
              label="Catalog"
              value={String(controller.catalog.length)}
              detail="Curated intake templates"
            />
            <SummaryCard
              icon={<PackageOpen size={16} />}
              label="Installed"
              value={String(controller.installedConnectors.length)}
              detail="Connector records in registry"
            />
            <SummaryCard
              icon={<GitBranch size={16} />}
              label="Live tools"
              value={String(totalPublishedTools)}
              detail="Published capabilities on deck"
            />
            <SummaryCard
              icon={<HeartPulse size={16} />}
              label="Paused"
              value={String(controller.pausedInteractions.length)}
              detail="Interactions waiting on operator"
            />
          </div>

          {controller.pausedInteractions.length > 0 ? (
            <Surface
              className="mc-connectors-panel"
              title="Global interactions"
              subtitle="Resume paused connector work without losing the operator handoff trail."
            >
              <label className="mc-connectors-field mc-connectors-field-span">
                <span>Resume payload JSON</span>
                <textarea
                  rows={3}
                  value={controller.interactionPayloadText}
                  onChange={(event) => controller.setInteractionPayloadText(event.target.value)}
                  placeholder={CONNECTOR_INTERACTION_PAYLOAD_PLACEHOLDER}
                />
              </label>
              <div className="mc-connectors-list">
                {controller.pausedInteractions.slice(0, 4).map((item) => (
                  <div key={item.interaction_id} className="mc-connectors-list-row">
                    <div>
                      <div className="mc-connectors-list-title">
                        <strong>{humanize(item.interaction_kind)}</strong>
                        <Chip label={humanize(item.status)} tone={toneForStatus(item.status)} />
                      </div>
                      <p>{item.prompt_summary}</p>
                      <div className="mc-connectors-list-meta">
                        <span>{item.agent_id ? agentsById.get(item.agent_id)?.name ?? item.agent_id : "shared"}</span>
                        <span>{formatTimestamp(item.updated_at)}</span>
                      </div>
                    </div>
                    <button
                      type="button"
                      className="secondary"
                      disabled={isBusy}
                      onClick={() => {
                        void controller.resumeInteraction(item.interaction_id);
                      }}
                    >
                      Resume
                    </button>
                  </div>
                ))}
              </div>
            </Surface>
          ) : null}
        </div>
      ) : null}

      {/* ── Catalog tab ── */}
      {activeTab === "catalog" ? (
        <Surface
          className="mc-connectors-panel mc-connectors-tab-surface"
          title="Catalog intake"
          subtitle="Start from curated scaffolds, then land a connector draft with a clear source of truth."
        >
          <div className="mc-connectors-toolbar">
            <input
              value={catalogQuery}
              onChange={(event) => { setCatalogQuery(event.target.value); setCatalogPage(0); }}
              placeholder="Search catalog"
            />
            <span className="mc-connectors-sort-label">Sorted by name</span>
          </div>
          <div className="mc-connectors-card-list">
            {pagedCatalog.length === 0 ? (
              <EmptyState message="No catalog templates match the current search." />
            ) : (
              pagedCatalog.map((item) => (
                <button
                  type="button"
                  key={item.catalog_item_id}
                  className="mc-connectors-card"
                  onClick={() => controller.applyCatalogTemplate(item.catalog_item_id)}
                >
                  <div className="mc-connectors-card-head">
                    <strong>{item.display_name}</strong>
                    <div className="mc-connectors-inline-chips">
                      <Chip label={item.source_kind} tone="checking" />
                      <Chip label={humanize(item.trust_class)} tone={toneForStatus(item.trust_class)} />
                    </div>
                  </div>
                  <p>{item.summary}</p>
                  <div className="mc-connectors-card-meta">
                    <span>{item.publisher}</span>
                    <span>{item.available_versions.join(", ") || "v1"}</span>
                  </div>
                </button>
              ))
            )}
          </div>
          {totalCatalogPages > 1 ? (
            <div className="mc-connectors-pager">
              <button
                type="button"
                className="mc-connectors-pager-btn"
                disabled={safeCatalogPage === 0}
                onClick={() => setCatalogPage((p) => p - 1)}
              >
                <ChevronLeft size={16} />
                <span>Previous</span>
              </button>
              <span className="mc-connectors-pager-counter">
                {safeCatalogPage + 1} / {totalCatalogPages}
              </span>
              <button
                type="button"
                className="mc-connectors-pager-btn"
                disabled={safeCatalogPage >= totalCatalogPages - 1}
                onClick={() => setCatalogPage((p) => p + 1)}
              >
                <span>Next</span>
                <ChevronRight size={16} />
              </button>
            </div>
          ) : null}
        </Surface>
      ) : null}

      {/* ── Import tab ── */}
      {activeTab === "import" ? (
        <Surface
          className="mc-connectors-panel mc-connectors-tab-surface"
          title="Import connector"
          subtitle="Pragmatic intake flow for raw JSON, inline source text, or URL-backed imports."
          headerRight={
            <div className="mc-connectors-surface-actions">
              <button
                type="button"
                className="ghost"
                onClick={() => controller.resetImportDraft()}
                disabled={isBusy}
              >
                Clear
              </button>
              <button
                type="button"
                data-testid="connectors-import-submit"
                onClick={() => {
                  void controller.importFromDraft().then((imported) => {
                    if (!imported) {
                      return;
                    }
                    setActiveTab("manage");
                    setManageSubTab("overview");
                  });
                }}
                disabled={isBusy}
              >
                <Upload size={14} />
                <span>Import</span>
              </button>
            </div>
          }
        >
          <div className="mc-connectors-form-grid mc-connectors-form-compact">
            <label className="mc-connectors-field">
              <span>Display name</span>
              <input
                value={controller.importDraft.display_name}
                onChange={(event) =>
                  controller.updateImportDraft({ display_name: event.target.value })
                }
                placeholder="GitHub automation"
              />
            </label>
            <label className="mc-connectors-field">
              <span>Source kind</span>
              <select
                value={controller.importDraft.source_kind}
                onChange={(event) =>
                  controller.updateImportDraft({ source_kind: event.target.value })
                }
              >
                {CONNECTOR_SOURCE_KIND_OPTIONS.map((option) => (
                  <option key={option} value={option}>
                    {option}
                  </option>
                ))}
              </select>
            </label>
            <label className="mc-connectors-field">
              <span>Slug</span>
              <input
                value={controller.importDraft.slug}
                onChange={(event) => controller.updateImportDraft({ slug: event.target.value })}
                placeholder="github-automation"
              />
            </label>
            <label className="mc-connectors-field">
              <span>Version label</span>
              <input
                value={controller.importDraft.version_label}
                onChange={(event) =>
                  controller.updateImportDraft({ version_label: event.target.value })
                }
                placeholder="v1"
              />
            </label>
            <label className="mc-connectors-field">
              <span>Origin kind</span>
              <select
                value={controller.importDraft.origin_kind}
                onChange={(event) =>
                  controller.updateImportDraft({ origin_kind: event.target.value })
                }
              >
                {CONNECTOR_ORIGIN_KIND_OPTIONS.map((option) => (
                  <option key={option} value={option}>
                    {option}
                  </option>
                ))}
              </select>
            </label>
            <label className="mc-connectors-field">
              <span>External ref policy</span>
              <select
                value={controller.importDraft.external_reference_policy}
                onChange={(event) =>
                  controller.updateImportDraft({
                    external_reference_policy: event.target.value,
                  })
                }
              >
                {CONNECTOR_EXTERNAL_REFERENCE_POLICY_OPTIONS.map((option) => (
                  <option key={option} value={option}>
                    {option}
                  </option>
                ))}
              </select>
            </label>
            <label className="mc-connectors-field">
              <span>Catalog item id</span>
              <input
                value={controller.importDraft.catalog_item_id}
                onChange={(event) =>
                  controller.updateImportDraft({ catalog_item_id: event.target.value })
                }
                placeholder="github-openapi"
              />
            </label>
            <label className="mc-connectors-field">
              <span>Import URL</span>
              <input
                value={controller.importDraft.import_url}
                onChange={(event) =>
                  controller.updateImportDraft({ import_url: event.target.value })
                }
                placeholder="https://example.com/openapi.json"
              />
            </label>
            <label className="mc-connectors-field mc-connectors-field-span">
              <span>Endpoint URL</span>
              <input
                value={controller.importDraft.endpoint_url}
                onChange={(event) =>
                  controller.updateImportDraft({ endpoint_url: event.target.value })
                }
                placeholder="https://api.example.com"
              />
            </label>
            <label className="mc-connectors-field mc-connectors-field-span">
              <span>Source text</span>
              <textarea
                rows={CONNECTOR_TEXTAREA_ROWS}
                value={controller.importDraft.source_text}
                onChange={(event) =>
                  controller.updateImportDraft({ source_text: event.target.value })
                }
                placeholder="Paste raw source text when the upstream is easier to stage inline."
              />
            </label>
            <label className="mc-connectors-field mc-connectors-field-span">
              <span>Source JSON</span>
              <textarea
                rows={CONNECTOR_JSON_TEXTAREA_ROWS}
                value={controller.importDraft.source_json_text}
                onChange={(event) =>
                  controller.updateImportDraft({ source_json_text: event.target.value })
                }
                placeholder='{"openapi":"3.1.0","paths":{}}'
              />
            </label>
          </div>
        </Surface>
      ) : null}

      {/* ── Registry tab ── */}
      {activeTab === "registry" ? (
        <Surface
          className="mc-connectors-panel mc-connectors-tab-surface"
          title="Installed registry"
          subtitle="Select a connector to inspect its current version, auth posture, and published tools."
        >
          <div className="mc-connectors-toolbar">
            <input
              value={registryQuery}
              onChange={(event) => { setRegistryQuery(event.target.value); setRegistryPage(0); }}
              placeholder="Search installed connectors"
            />
            <span className="mc-connectors-sort-label">Sorted by name</span>
            <button
              type="button"
              className="ghost"
              disabled={isBusy}
              onClick={() => {
                void controller.refresh();
              }}
            >
              <RefreshCw size={14} />
              <span>Refresh</span>
            </button>
          </div>
          <div className="mc-connectors-card-list">
            {pagedConnectors.length === 0 ? (
              <EmptyState message="No installed connectors match the current search." />
            ) : (
              pagedConnectors.map((item) => (
                <button
                  type="button"
                  key={item.connector_id}
                  className={`mc-connectors-card${
                    item.connector_id === controller.selectedConnectorId ? " is-selected" : ""
                  }`}
                  onClick={() => openManageTab(item.connector_id)}
                >
                  <div className="mc-connectors-card-head">
                    <strong>{item.display_name}</strong>
                    <div className="mc-connectors-inline-chips">
                      <Chip label={humanize(item.status)} tone={toneForStatus(item.status)} />
                      <Chip
                        label={humanize(item.trust_state)}
                        tone={toneForStatus(item.trust_state)}
                      />
                    </div>
                  </div>
                  <p>{item.slug}</p>
                  <div className="mc-connectors-card-meta">
                    <span>{item.source_kind}</span>
                    <span>{item.published_tool_count} live tool(s)</span>
                    <span>{item.assigned_agent_count} agent(s)</span>
                  </div>
                </button>
              ))
            )}
          </div>
          {totalRegistryPages > 1 ? (
            <div className="mc-connectors-pager">
              <button
                type="button"
                className="mc-connectors-pager-btn"
                disabled={safeRegistryPage === 0}
                onClick={() => setRegistryPage((p) => p - 1)}
              >
                <ChevronLeft size={16} />
                <span>Previous</span>
              </button>
              <span className="mc-connectors-pager-counter">
                {safeRegistryPage + 1} / {totalRegistryPages}
              </span>
              <button
                type="button"
                className="mc-connectors-pager-btn"
                disabled={safeRegistryPage >= totalRegistryPages - 1}
                onClick={() => setRegistryPage((p) => p + 1)}
              >
                <span>Next</span>
                <ChevronRight size={16} />
              </button>
            </div>
          ) : null}
        </Surface>
      ) : null}

      {/* ── Manage tab ── */}
      {activeTab === "manage" ? (
        <div className="mc-connectors-tab-content">
          <Surface
            className="mc-connectors-panel mc-connectors-hero"
            title={controller.selectedConnector?.display_name ?? "Connector detail"}
            subtitle={
              controller.selectedConnector
                ? `${controller.selectedConnector.slug} · ${controller.selectedConnector.source_kind}`
                : "Select an installed connector to open review, auth, and health surfaces."
            }
            headerRight={
              controller.selectedConnector ? (
                <div className="mc-connectors-surface-actions">
                  <button
                    type="button"
                    className="ghost"
                    disabled={isBusy}
                    onClick={() => {
                      void controller.refresh();
                    }}
                  >
                    <RefreshCw size={14} />
                    <span>Sync</span>
                  </button>
                  <button
                    type="button"
                    className={
                      controller.selectedConnector.status === "enabled" ? "danger" : "secondary"
                    }
                    disabled={isBusy}
                    onClick={() => {
                      void controller.updateConnectorEnabled(
                        controller.selectedConnector?.status !== "enabled"
                      );
                    }}
                  >
                    {controller.selectedConnector.status === "enabled" ? "Disable" : "Enable"}
                  </button>
                </div>
              ) : null
            }
          >
            {!controller.selectedConnector ? (
              <EmptyState message="Select a connector from the registry to open its lifecycle controls." />
            ) : controller.detailLoading && !controller.selectedConnectorDetail ? (
              <EmptyState message="Loading selected connector detail…" />
            ) : controller.detailError ? (
              <EmptyState message={controller.detailError} />
            ) : (
              <div className="mc-connectors-overview">
                <div className="mc-connectors-inline-chips">
                  <Chip
                    label={humanize(controller.selectedConnector.status)}
                    tone={toneForStatus(controller.selectedConnector.status)}
                  />
                  <Chip
                    label={humanize(controller.selectedConnector.trust_state)}
                    tone={toneForStatus(controller.selectedConnector.trust_state)}
                  />
                  {sharedBinding ? (
                    <Chip
                      label={`shared auth · ${humanize(sharedBinding.status)}`}
                      tone={toneForStatus(sharedBinding.status)}
                    />
                  ) : (
                    <Chip label="shared auth missing" tone="warning" />
                  )}
                </div>
                <dl className="mc-connectors-fact-grid">
                  <div>
                    <dt>Origin</dt>
                    <dd>{humanize(controller.selectedConnector.origin_kind)}</dd>
                  </div>
                  <div>
                    <dt>Current version</dt>
                    <dd>{controller.selectedVersion?.version_label ?? "not live"}</dd>
                  </div>
                  <div>
                    <dt>Last conversion</dt>
                    <dd>{formatTimestamp(controller.selectedConnector.last_conversion_at)}</dd>
                  </div>
                  <div>
                    <dt>Last review</dt>
                    <dd>{formatTimestamp(controller.selectedConnector.last_review_at)}</dd>
                  </div>
                  <div>
                    <dt>Published tools</dt>
                    <dd>{controller.selectedConnector.published_tool_count}</dd>
                  </div>
                  <div>
                    <dt>Assigned agents</dt>
                    <dd>{controller.selectedConnector.assigned_agent_count}</dd>
                  </div>
                </dl>
              </div>
            )}
          </Surface>

          {controller.selectedConnector ? (
            <>
              <nav className="mc-connectors-sub-tab-bar">
                <button
                  type="button"
                  className={manageSubTab === "overview" ? "active" : ""}
                  onClick={() => setManageSubTab("overview")}
                >
                  Review
                </button>
                <button
                  type="button"
                  className={manageSubTab === "auth" ? "active" : ""}
                  onClick={() => setManageSubTab("auth")}
                >
                  Auth
                </button>
                <button
                  type="button"
                  className={manageSubTab === "health" ? "active" : ""}
                  onClick={() => { setManageSubTab("health"); setToolsPage(0); }}
                >
                  Health
                </button>
              </nav>

              <div className="mc-connectors-manage-content">
                {/* ── Review sub-tab ── */}
                {manageSubTab === "overview" ? (
                  <Surface
                    className="mc-connectors-panel"
                    title="Review + publish"
                    subtitle="Convert the selected version, inspect generated operations, then publish only the tools you want live."
                    headerRight={
                      <div className="mc-connectors-surface-actions">
                        <button
                          type="button"
                          className="secondary"
                          data-testid="connectors-convert-submit"
                          disabled={isBusy || !controller.selectedConnector}
                          onClick={() => {
                            void controller.convertSelectedConnector();
                          }}
                        >
                          Convert
                        </button>
                        <button
                          type="button"
                          className="ghost"
                          disabled={isBusy || !controller.selectedVersionId}
                          onClick={() => {
                            void controller.rollbackSelectedVersion();
                          }}
                        >
                          Roll back
                        </button>
                      </div>
                    }
                  >
                    <div className="mc-connectors-toolbar mc-connectors-toolbar-stacked">
                      <label className="mc-connectors-field">
                        <span>Version target</span>
                        <select
                          value={controller.selectedVersionId}
                          onChange={(event) => controller.setSelectedVersionId(event.target.value)}
                        >
                          {(controller.selectedConnectorDetail?.versions ?? []).map((item) => (
                            <option key={item.version_id} value={item.version_id}>
                              {item.version_label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <div className="mc-connectors-inline-chips">
                        <Chip
                          label={
                            controller.selectedConversion
                              ? `conversion · ${humanize(controller.selectedConversion.status)}`
                              : "no review cache"
                          }
                          tone={toneForStatus(controller.selectedConversion?.status)}
                        />
                        {controller.selectedConversion ? (
                          <Chip
                            label={`${controller.selectedConversion.proposed_tools.length} proposed`}
                            tone="checking"
                          />
                        ) : null}
                      </div>
                    </div>

                    {!controller.selectedConversion ? (
                      <EmptyState message="Run conversion on the selected version to inspect reviewable operations." />
                    ) : (
                      <>
                        {controller.selectedConversion.warnings.length > 0 ? (
                          <div className="mc-connectors-warning-list">
                            {controller.selectedConversion.warnings.map((warning) => (
                              <div key={`${warning.code}-${warning.message}`} className="mc-connectors-warning">
                                <ShieldAlert size={14} />
                                <span>
                                  <strong>{warning.code}</strong> {warning.message}
                                </span>
                              </div>
                            ))}
                          </div>
                        ) : null}

                        <div className="mc-connectors-toggle-row">
                          <label className="mc-connectors-checkbox">
                            <input
                              type="checkbox"
                              checked={controller.publishDraft.enable_after_publish}
                              disabled={isBusy}
                              onChange={(event) =>
                                controller.setEnableAfterPublish(event.target.checked)
                              }
                            />
                            <span>Enable immediately after publish</span>
                          </label>
                          <button
                            type="button"
                            data-testid="connectors-publish-submit"
                            disabled={
                              isBusy || controller.publishDraft.selected_candidate_ids.length === 0
                            }
                            onClick={() => {
                              setConfirmAction("publish");
                            }}
                          >
                            Publish selected
                          </button>
                        </div>

                        <div className="mc-connectors-candidate-list">
                          {pagedCandidates.map((candidate) => {
                            const selected = controller.publishDraft.selected_candidate_ids.includes(
                              candidate.candidate_id
                            );
                            return (
                              <div
                                key={candidate.candidate_id}
                                className={`mc-connectors-candidate${
                                  selected ? " is-selected" : ""
                                }${candidate.review_blocked ? " is-blocked" : ""}`}
                              >
                                <label className="mc-connectors-checkbox">
                                  <input
                                    type="checkbox"
                                    checked={selected}
                                    disabled={candidate.review_blocked}
                                    aria-label={`Select publish candidate ${candidate.display_name}`}
                                    onChange={() =>
                                      controller.togglePublishCandidate(candidate.candidate_id)
                                    }
                                  />
                                </label>
                                <div className="mc-connectors-candidate-main">
                                  <div className="mc-connectors-card-head">
                                    <strong>{candidate.display_name}</strong>
                                    <div className="mc-connectors-inline-chips">
                                      <Chip
                                        label={humanize(candidate.write_classification)}
                                        tone={toneForStatus(candidate.write_classification)}
                                      />
                                      {candidate.review_blocked ? (
                                        <Chip label="blocked" tone="down" />
                                      ) : null}
                                    </div>
                                  </div>
                                  <p>{candidate.description ?? candidate.operation_key}</p>
                                  {candidate.review_block_reason ? (
                                    <p className="mc-connectors-inline-error">
                                      {candidate.review_block_reason}
                                    </p>
                                  ) : null}
                                  <div className="mc-connectors-field">
                                    <span>Alias override</span>
                                    <input
                                      value={
                                        controller.publishDraft.alias_overrides[
                                          candidate.candidate_id
                                        ] ?? ""
                                      }
                                      disabled={candidate.review_blocked}
                                      onChange={(event) =>
                                        controller.setPublishAlias(
                                          candidate.candidate_id,
                                          event.target.value
                                        )
                                      }
                                      placeholder={candidate.proposed_tool_name}
                                    />
                                  </div>
                                </div>
                              </div>
                            );
                          })}
                        </div>

                        {totalCandidatePages > 1 ? (
                          <div className="mc-connectors-pager">
                            <button
                              type="button"
                              className="mc-connectors-pager-btn"
                              disabled={safeCandidatePage === 0}
                              onClick={() => setCandidatePage((p) => p - 1)}
                            >
                              <ChevronLeft size={16} />
                              <span>Previous</span>
                            </button>
                            <span className="mc-connectors-pager-counter">
                              {safeCandidatePage + 1} / {totalCandidatePages}
                            </span>
                            <button
                              type="button"
                              className="mc-connectors-pager-btn"
                              disabled={safeCandidatePage >= totalCandidatePages - 1}
                              onClick={() => setCandidatePage((p) => p + 1)}
                            >
                              <span>Next</span>
                              <ChevronRight size={16} />
                            </button>
                          </div>
                        ) : null}

                        {controller.selectedConversion.normalization_notes.length > 0 ? (
                          <div className="mc-connectors-note-stack">
                            {controller.selectedConversion.normalization_notes.map((note) => (
                              <p key={note}>{note}</p>
                            ))}
                          </div>
                        ) : null}

                        <details className="mc-connectors-json-panel">
                          <summary>Diff from previous</summary>
                          <pre>{stringifyJson(controller.selectedConversion.diff_from_previous)}</pre>
                        </details>
                      </>
                    )}
                  </Surface>
                ) : null}

                {/* ── Auth sub-tab ── */}
                {manageSubTab === "auth" ? (
                  <Surface
                    className="mc-connectors-panel"
                    title="Assignments + auth"
                    subtitle="Own the connector once, then expose it to the right agents with the right auth shape."
                  >
                    <div className="mc-connectors-stack">
                      <section className="mc-connectors-section">
                        <div className="mc-connectors-section-head">
                          <h3>Assignments</h3>
                          <button
                            type="button"
                            className="secondary"
                            data-testid="connectors-assignment-submit"
                            disabled={isBusy}
                            onClick={() => {
                              void controller.saveAssignmentDraft();
                            }}
                          >
                            Save assignment
                          </button>
                        </div>
                        <div className="mc-connectors-list">
                          {(controller.selectedConnectorDetail?.assignments ?? []).length === 0 ? (
                            <EmptyState message="No agent assignments have been saved yet." />
                          ) : (
                            controller.selectedConnectorDetail?.assignments.map((item) => (
                              <div key={item.assignment_id} className="mc-connectors-list-row">
                                <div>
                                  <div className="mc-connectors-list-title">
                                    <strong>
                                      {agentsById.get(item.agent_id)?.name ?? item.agent_id}
                                    </strong>
                                    <Chip
                                      label={item.enabled ? "enabled" : "disabled"}
                                      tone={item.enabled ? "up" : "down"}
                                    />
                                  </div>
                                  <div className="mc-connectors-list-meta">
                                    <span>{humanize(item.auth_mode)}</span>
                                    <span>{formatTimestamp(item.updated_at)}</span>
                                  </div>
                                </div>
                              </div>
                            ))
                          )}
                        </div>
                        <div className="mc-connectors-form-grid">
                          <label className="mc-connectors-field">
                            <span>Agent</span>
                            <select
                              data-testid="connectors-assignment-agent"
                              value={controller.assignmentDraft.agent_id}
                              onChange={(event) =>
                                controller.updateAssignmentDraft({ agent_id: event.target.value })
                              }
                            >
                              {controller.agents.map((agent) => (
                                <option key={agent.agent_id} value={agent.agent_id}>
                                  {agent.name}
                                </option>
                              ))}
                            </select>
                          </label>
                          <label className="mc-connectors-field">
                            <span>Auth mode</span>
                            <select
                              value={controller.assignmentDraft.auth_mode}
                              onChange={(event) =>
                                controller.updateAssignmentDraft({ auth_mode: event.target.value })
                              }
                            >
                              {CONNECTOR_ASSIGNMENT_AUTH_MODE_OPTIONS.map((option) => (
                                <option key={option} value={option}>
                                  {option}
                                </option>
                              ))}
                            </select>
                          </label>
                          <label className="mc-connectors-checkbox mc-connectors-checkbox-card">
                            <input
                              type="checkbox"
                              checked={controller.assignmentDraft.enabled}
                              onChange={(event) =>
                                controller.updateAssignmentDraft({ enabled: event.target.checked })
                              }
                            />
                            <span>Assignment enabled</span>
                          </label>
                        </div>
                      </section>

                      <section className="mc-connectors-section">
                        <div className="mc-connectors-section-head">
                          <h3>Auth bindings</h3>
                          <button
                            type="button"
                            className="secondary"
                            data-testid="connectors-auth-binding-submit"
                            disabled={isBusy}
                            onClick={() => {
                              void controller.saveAuthBinding();
                            }}
                          >
                            Save binding
                          </button>
                        </div>
                        <div className="mc-connectors-list">
                          {(controller.selectedConnectorDetail?.auth_bindings ?? []).length === 0 ? (
                            <EmptyState message="No shared or agent-scoped auth bindings are on record." />
                          ) : (
                            controller.selectedConnectorDetail?.auth_bindings.map((item) => (
                              <div key={item.auth_binding_id} className={`mc-connectors-list-row${item.agent_id ? "" : " mc-connectors-shared-binding"}`}>
                                <div>
                                  <div className="mc-connectors-list-title">
                                    <strong>
                                      {item.agent_id
                                        ? agentsById.get(item.agent_id)?.name ?? item.agent_id
                                        : "\u2731 Shared default"}
                                    </strong>
                                    <Chip
                                      label={humanize(item.status)}
                                      tone={toneForStatus(item.status)}
                                    />
                                  </div>
                                  <div className="mc-connectors-list-meta">
                                    <span>{item.auth_kind}</span>
                                    <span>{item.secret_ref ?? "no secret ref"}</span>
                                    <span>{formatTimestamp(item.updated_at)}</span>
                                  </div>
                                </div>
                              </div>
                            ))
                          )}
                        </div>
                        <div className="mc-connectors-form-grid">
                          <label className="mc-connectors-field">
                            <span>Scope</span>
                            <select
                              value={controller.authBindingDraft.agent_id}
                              onChange={(event) =>
                                controller.updateAuthBindingDraft({ agent_id: event.target.value })
                              }
                            >
                              <option value="">Shared default</option>
                              {controller.agents.map((agent) => (
                                <option key={agent.agent_id} value={agent.agent_id}>
                                  {agent.name}
                                </option>
                              ))}
                            </select>
                          </label>
                          <label className="mc-connectors-field">
                            <span>Auth kind</span>
                            <select
                              value={controller.authBindingDraft.auth_kind}
                              onChange={(event) =>
                                controller.updateAuthBindingDraft({ auth_kind: event.target.value })
                              }
                            >
                              {CONNECTOR_AUTH_KIND_OPTIONS.map((option) => (
                                <option key={option} value={option}>
                                  {option}
                                </option>
                              ))}
                            </select>
                          </label>
                          <label className="mc-connectors-field">
                            <span>Status</span>
                            <select
                              value={controller.authBindingDraft.status}
                              onChange={(event) =>
                                controller.updateAuthBindingDraft({ status: event.target.value })
                              }
                            >
                              {CONNECTOR_AUTH_STATUS_OPTIONS.map((option) => (
                                <option key={option} value={option}>
                                  {option}
                                </option>
                              ))}
                            </select>
                          </label>
                          <label className="mc-connectors-field">
                            <span>Secret ref</span>
                            <input
                              data-testid="connectors-auth-secret-ref"
                              value={controller.authBindingDraft.secret_ref}
                              onChange={(event) =>
                                controller.updateAuthBindingDraft({ secret_ref: event.target.value })
                              }
                              placeholder="secrets/connectors/github"
                            />
                          </label>
                          <label className="mc-connectors-field mc-connectors-field-span">
                            <span>OAuth session id</span>
                            <input
                              value={controller.authBindingDraft.oauth_session_id}
                              onChange={(event) =>
                                controller.updateAuthBindingDraft({
                                  oauth_session_id: event.target.value,
                                })
                              }
                              placeholder="optional oauth session id"
                            />
                          </label>
                          <label className="mc-connectors-field mc-connectors-field-span">
                            <span>Auth metadata JSON</span>
                            <textarea
                              rows={3}
                              value={controller.authBindingDraft.auth_metadata_text}
                              onChange={(event) =>
                                controller.updateAuthBindingDraft({
                                  auth_metadata_text: event.target.value,
                                })
                              }
                              placeholder='{"header_name":"Authorization"}'
                            />
                          </label>
                        </div>
                      </section>
                    </div>
                  </Surface>
                ) : null}

                {/* ── Health sub-tab ── */}
                {manageSubTab === "health" ? (
                  <>
                    <Surface
                      className="mc-connectors-panel"
                      title="Health + live tools"
                      subtitle="Read current runtime posture, then inspect the tool payload that is actually live."
                      headerRight={
                        <button
                          type="button"
                          className="ghost"
                          data-testid="connectors-health-refresh"
                          disabled={isBusy || !controller.selectedConnector}
                          onClick={() => {
                            void controller.refreshHealth();
                          }}
                        >
                          <RefreshCw size={14} />
                          <span>Refresh health</span>
                        </button>
                      }
                    >
                      <div className="mc-connectors-stack">
                        <div className="mc-connectors-health-strip">
                          <div className="mc-connectors-health-stat">
                            <strong>Status</strong>
                            <span>{humanize(controller.health?.status ?? controller.selectedConnector.status)}</span>
                          </div>
                          <div className="mc-connectors-health-stat">
                            <strong>Health</strong>
                            <span>
                              <Chip
                                label={
                                  controller.health?.degraded_reason
                                    ? "degraded"
                                    : controller.health?.auth_required
                                      ? "auth required"
                                      : "ready"
                                }
                                tone={toneForHealth(
                                  controller.health?.status,
                                  controller.health?.degraded_reason,
                                  controller.health?.auth_required ?? false
                                )}
                              />
                            </span>
                          </div>
                          <div className="mc-connectors-health-stat">
                            <strong>Checked</strong>
                            <span>{formatTimestamp(controller.health?.last_checked_at)}</span>
                          </div>
                          <div className="mc-connectors-health-stat">
                            <strong>Auth coverage</strong>
                            <span>
                              {controller.health
                                ? `${controller.health.auth_missing_tool_count} missing / ${controller.health.auth_required_tool_count} required`
                                : "not checked"}
                            </span>
                          </div>
                        </div>

                        {controller.healthError ? (
                          <p className="mc-connectors-inline-error">{controller.healthError}</p>
                        ) : controller.health?.degraded_reason ? (
                          <p className="mc-connectors-inline-error">{controller.health.degraded_reason}</p>
                        ) : controller.health?.auth_required ? (
                          <p className="mc-connectors-inline-error">
                            Add connector auth before running the tools marked Auth required.
                          </p>
                        ) : null}

                        <div className="mc-connectors-list">
                          {publishedTools.length === 0 ? (
                            <EmptyState message="No published tools are live for this connector yet." />
                          ) : (
                            pagedTools.map((tool) => (
                              <ToolSelectionRow
                                key={tool.published_tool_id}
                                tool={tool}
                                selected={tool.published_tool_id === controller.selectedPublishedToolId}
                                checked={controller.selectedPublishedToolIds.includes(
                                  tool.published_tool_id
                                )}
                                onSelect={() => controller.selectPublishedTool(tool.published_tool_id)}
                                onToggle={() =>
                                  controller.togglePublishedToolSelection(tool.published_tool_id)
                                }
                              />
                            ))
                          )}
                        </div>

                        {totalToolsPages > 1 ? (
                          <div className="mc-connectors-pager">
                            <button
                              type="button"
                              className="mc-connectors-pager-btn"
                              disabled={safeToolsPage === 0}
                              onClick={() => setToolsPage((p) => p - 1)}
                            >
                              <ChevronLeft size={16} />
                              <span>Previous</span>
                            </button>
                            <span className="mc-connectors-pager-counter">
                              {safeToolsPage + 1} / {totalToolsPages}
                            </span>
                            <button
                              type="button"
                              className="mc-connectors-pager-btn"
                              disabled={safeToolsPage >= totalToolsPages - 1}
                              onClick={() => setToolsPage((p) => p + 1)}
                            >
                              <span>Next</span>
                              <ChevronRight size={16} />
                            </button>
                          </div>
                        ) : null}

                        <div className="mc-connectors-surface-actions">
                          <button
                            type="button"
                            className="danger"
                            disabled={isBusy || controller.selectedPublishedToolIds.length === 0}
                            onClick={() => {
                              setConfirmAction("unpublish");
                            }}
                          >
                            Unpublish selected
                          </button>
                        </div>

                        {controller.toolDetailError ? (
                          <p className="mc-connectors-inline-error">{controller.toolDetailError}</p>
                        ) : toolDetail ? (
                          <div className="mc-connectors-json-stack">
                            <div className="mc-connectors-json-panel">
                              <div className="mc-connectors-json-head">
                                <h3>{toolDetail.display_name}</h3>
                                <div className="mc-connectors-inline-chips">
                                  <Chip label={toolDetail.tool_name} tone="checking" />
                                  <Chip
                                    label={humanize(toolDetail.write_classification)}
                                    tone={toneForStatus(toolDetail.write_classification)}
                                  />
                                </div>
                              </div>
                              <pre>{stringifyJson(toolDetail.tool_schema)}</pre>
                            </div>
                            <div className="mc-connectors-json-panel">
                              <div className="mc-connectors-json-head">
                                <h3>Origin metadata</h3>
                              </div>
                              <pre>{stringifyJson(toolDetail.origin_metadata)}</pre>
                            </div>
                          </div>
                        ) : (
                          <EmptyState message="Select a published tool to inspect its live schema and origin metadata." />
                        )}
                      </div>
                    </Surface>

                    {controller.selectedConnectorInteractions.length > 0 ? (
                      <Surface
                        className="mc-connectors-panel"
                        title="Connector interactions"
                        subtitle="Track connector-specific pauses, resumptions, and operator-sensitive handoffs."
                      >
                        <div className="mc-connectors-list">
                          {controller.selectedConnectorInteractions.slice(0, 4).map((item) => (
                            <div key={item.interaction_id} className="mc-connectors-list-row">
                              <div>
                                <div className="mc-connectors-list-title">
                                  <strong>{humanize(item.interaction_kind)}</strong>
                                  <Chip label={humanize(item.status)} tone={toneForStatus(item.status)} />
                                </div>
                                <p>{item.prompt_summary}</p>
                                <div className="mc-connectors-list-meta">
                                  <span>
                                    {item.agent_id
                                      ? agentsById.get(item.agent_id)?.name ?? item.agent_id
                                      : "shared"}
                                  </span>
                                  <span>{formatTimestamp(item.updated_at)}</span>
                                </div>
                              </div>
                              <button
                                type="button"
                                className="secondary"
                                disabled={isBusy}
                                onClick={() => {
                                  void controller.resumeInteraction(item.interaction_id);
                                }}
                              >
                                Resume
                              </button>
                            </div>
                          ))}
                        </div>
                      </Surface>
                    ) : null}
                  </>
                ) : null}
              </div>
            </>
          ) : null}
        </div>
      ) : null}

      <Modal
        open={confirmAction !== null}
        onClose={() => setConfirmAction(null)}
        title={
          confirmAction === "publish"
            ? "Publish connector tools"
            : "Unpublish connector tools"
        }
        subtitle={
          confirmAction === "publish"
            ? "This will update the live tool catalog available to assigned agents."
            : "This will remove selected live tools from assigned agents."
        }
        width="560px"
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setConfirmAction(null)}>
              Cancel
            </button>
            <button
              type="button"
              className={confirmAction === "unpublish" ? "danger" : undefined}
              disabled={isBusy}
              onClick={() => {
                const action = confirmAction;
                setConfirmAction(null);
                if (action === "publish") {
                  void controller.publishSelectedTools();
                } else if (action === "unpublish") {
                  void controller.unpublishSelectedTools();
                }
              }}
            >
              {confirmAction === "publish" ? "Publish" : "Unpublish"}
            </button>
          </>
        }
      >
        <div className="mc-connectors-note-stack">
          <p>
            Connector:{" "}
            <strong>{controller.selectedConnector?.display_name ?? "Unknown connector"}</strong>
          </p>
          <p>
            {confirmAction === "publish"
              ? `${controller.publishDraft.selected_candidate_ids.length} proposed tool(s) are selected for publish.`
              : `${controller.selectedPublishedToolIds.length} live tool(s) are selected for unpublish.`}
          </p>
          {confirmAction === "publish" ? (
            <p>
              {controller.publishDraft.enable_after_publish
                ? "The connector will be enabled immediately after publish."
                : "The connector will keep its current enabled state after publish."}
            </p>
          ) : null}
        </div>
      </Modal>
    </section>
  );
}
