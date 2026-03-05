import { useRef, useState } from "react";
import { formatDateTime, formatRelative } from "../../utils/datetime";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { InlineActions } from "../../ui/InlineActions";
import { useWidgetPagination } from "./useWidgetPagination";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type {
  Agent,
  AuthProfileResponse,
  ChannelRuntimeAdapterStatusResponse,
  CircuitBreakerStateResponse,
  JobStatusResponse,
  MissionControlCalendarJob,
  MissionControlFocusItem,
  MissionControlUsageByAgent,
  MissionControlUsageByModel,
  PluginManifestResponse,
  PluginRuntimeStatusResponse,
  SkillResponse,
  StatusResponse,
} from "../../types";
import type { EventStreamItem } from "../../app/useAppController";
import { CustomWidgetRenderer } from "./CustomWidgetRenderer";
import type { CockpitWidgetLayoutV2 } from "./cockpitLayout";
import type { RuntimeConnectionSettings } from "../../types";

interface CockpitWidgetRendererProps {
  widget: CockpitWidgetLayoutV2;
  settings: RuntimeConnectionSettings;
  incidentMode: boolean;
  setIncidentMode: (next: boolean) => void;
  gatewayStatus: StatusResponse | null;
  jobsStatus: JobStatusResponse | null;
  approvalsCount: number;
  openBreakers: CircuitBreakerStateResponse[];
  openPluginBreakers: PluginRuntimeStatusResponse[];
  channelStatuses: ChannelRuntimeAdapterStatusResponse[];
  incidentFocusItems: MissionControlFocusItem[];
  calendarJobs: MissionControlCalendarJob[];
  selectedProviderControlAgentId: string;
  setSelectedProviderControlAgentId: (agentId: string) => void;
  selectedProviderControlProvider: string;
  setSelectedProviderControlProvider: (provider: string) => void;
  providerOptions: string[];
  orderedProviderProfiles: AuthProfileResponse[];
  providerProfileOrderDirty: boolean;
  agents: Agent[];
  skills: SkillResponse[];
  plugins: PluginManifestResponse[];
  pluginRuntimeById: Map<string, PluginRuntimeStatusResponse>;
  visibleEvents: EventStreamItem[];
  usageChartsEnabled: boolean;
  usageToday: {
    currency: string;
    estimatedCostTotal: number;
    tokenInputTotal: number;
    tokenOutputTotal: number;
    byAgent: MissionControlUsageByAgent[];
    byModel: MissionControlUsageByModel[];
  } | null;
  usageWeek: {
    currency: string;
    estimatedCostTotal: number;
  } | null;
  usageUnavailableReason: string | null;
  usageCorrelationAvailable: boolean;
  usageFreshness: "fresh" | "stale" | "limited";
  usageTrend: {
    direction: "up" | "down" | "flat" | "limited" | "unknown";
    label: string;
  };
  usageBudgetWarnings: Array<{
    tone: "warning" | "critical";
    message: string;
  }>;
  usageUpdatedAtUtc: string | null;
  onRefreshAll: () => void;
  onRunCalendarJobNow: (jobId: string) => Promise<void>;
  onToggleCalendarJob: (jobId: string, enabled: boolean) => Promise<void>;
  onReconnectFocusChannel: (provider: string) => Promise<void>;
  onMoveProviderProfile: (profileId: string, delta: number) => void;
  onSaveProviderOrder: () => Promise<void>;
  onReloadProviderProfileOrder: () => Promise<void>;
  onToggleSkillState: (skillId: string, enabled: boolean) => Promise<void>;
  onTogglePluginState: (pluginId: string, enabled: boolean) => Promise<void>;
}

function PaginationControls({
  page,
  totalPages,
  onSetPage,
}: {
  page: number;
  totalPages: number;
  onSetPage: (p: number) => void;
}) {
  if (totalPages <= 1) return null;
  return (
    <div className="mc-widget-pagination">
      <button
        type="button"
        disabled={page <= 0}
        onClick={() => onSetPage(page - 1)}
        aria-label={`Previous page (${page + 1} of ${totalPages})`}
      >
        <ChevronLeft size={12} />
      </button>
      <span className="mc-widget-pagination-label">
        {page + 1}/{totalPages}
      </span>
      <button
        type="button"
        disabled={page >= totalPages - 1}
        onClick={() => onSetPage(page + 1)}
        aria-label={`Next page (${page + 1} of ${totalPages})`}
      >
        <ChevronRight size={12} />
      </button>
    </div>
  );
}

const LIST_ITEM_HEIGHT = 44;
const COMPACT_ITEM_HEIGHT = 38;
const EVENT_ITEM_HEIGHT = 32;
const MONEY_FORMATTERS = new Map<string, Intl.NumberFormat>();
const TOKEN_FORMATTER = new Intl.NumberFormat("en-US", {
  notation: "compact",
  maximumFractionDigits: 1,
});

function normalizeCurrencyCode(currency: string | null | undefined): string {
  if (typeof currency !== "string") {
    return "USD";
  }
  const normalized = currency.trim().toUpperCase();
  return /^[A-Z]{3}$/.test(normalized) ? normalized : "USD";
}

function formatMoney(value: number, currency: string): string {
  const currencyCode = normalizeCurrencyCode(currency);
  let formatter = MONEY_FORMATTERS.get(currencyCode);
  if (!formatter) {
    formatter = new Intl.NumberFormat("en-US", {
      style: "currency",
      currency: currencyCode,
      maximumFractionDigits: 3,
    });
    MONEY_FORMATTERS.set(currencyCode, formatter);
  }
  return formatter.format(value);
}

function formatTokens(value: number): string {
  return TOKEN_FORMATTER.format(value);
}

export function CockpitWidgetRenderer(props: CockpitWidgetRendererProps) {
  const { widget } = props;
  const [busyActions, setBusyActions] = useState<Set<string>>(new Set());
  const busyActionsRef = useRef<Set<string>>(new Set());

  const focusListRef = useRef<HTMLDivElement>(null);
  const breakerCoreRef = useRef<HTMLDivElement>(null);
  const breakerPluginRef = useRef<HTMLDivElement>(null);
  const jobsListRef = useRef<HTMLDivElement>(null);
  const channelsListRef = useRef<HTMLDivElement>(null);
  const profilesListRef = useRef<HTMLDivElement>(null);
  const skillsListRef = useRef<HTMLDivElement>(null);
  const pluginsListRef = useRef<HTMLDivElement>(null);
  const eventsListRef = useRef<HTMLDivElement>(null);

  const focusPagination = useWidgetPagination(props.incidentFocusItems.length, focusListRef, LIST_ITEM_HEIGHT);
  const breakerCorePagination = useWidgetPagination(props.openBreakers.length, breakerCoreRef, COMPACT_ITEM_HEIGHT);
  const breakerPluginPagination = useWidgetPagination(props.openPluginBreakers.length, breakerPluginRef, COMPACT_ITEM_HEIGHT);
  const jobsPagination = useWidgetPagination(props.calendarJobs.length, jobsListRef, LIST_ITEM_HEIGHT);
  const channelsPagination = useWidgetPagination(props.channelStatuses.length, channelsListRef, LIST_ITEM_HEIGHT);
  const profilesPagination = useWidgetPagination(props.orderedProviderProfiles.length, profilesListRef, LIST_ITEM_HEIGHT);
  const skillsPagination = useWidgetPagination(props.skills.length, skillsListRef, LIST_ITEM_HEIGHT);
  const pluginsPagination = useWidgetPagination(props.plugins.length, pluginsListRef, LIST_ITEM_HEIGHT);
  const eventsPagination = useWidgetPagination(props.visibleEvents.length, eventsListRef, EVENT_ITEM_HEIGHT);

  const runBusyAction = (key: string, fn: () => Promise<unknown>) => {
    if (busyActionsRef.current.has(key)) {
      return;
    }
    busyActionsRef.current.add(key);
    setBusyActions(new Set(busyActionsRef.current));
    void fn()
      .catch((error: unknown) => {
        console.error("cockpit widget action failed", { key, error });
      })
      .finally(() => {
        busyActionsRef.current.delete(key);
        setBusyActions(new Set(busyActionsRef.current));
      });
  };

  const isBusyAction = (key: string) => busyActions.has(key);

  if (widget.widget === "custom") {
    if (widget.custom_config) {
      return <CustomWidgetRenderer config={widget.custom_config} settings={props.settings} />;
    }
    return (
      <article className="mc-cockpit-widget-body">
        <span className="mc-custom-widget-empty">Custom widget configuration missing.</span>
      </article>
    );
  }

  if (widget.widget === "health") {
    const topAgents = (props.usageToday?.byAgent ?? []).slice(0, 3);
    const topModels = (props.usageToday?.byModel ?? []).slice(0, 3);
    const usageCurrency = normalizeCurrencyCode(
      props.usageToday?.currency ?? props.usageWeek?.currency
    );
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-health-grid">
          <div>
            <strong>Gateway</strong>
            <p>{props.gatewayStatus?.service ?? "offline"}</p>
          </div>
          <div>
            <strong>Scheduler</strong>
            <p>{props.jobsStatus?.scheduler_running ? "running" : "paused"}</p>
          </div>
          <div>
            <strong>Approvals</strong>
            <p>{props.approvalsCount}</p>
          </div>
          <div>
            <strong>Open Breakers</strong>
            <p>{props.openBreakers.length + props.openPluginBreakers.length}</p>
          </div>
          <div>
            <strong>Degraded Channels</strong>
            <p>
              {
                props.channelStatuses.filter(
                  (item) => !item.healthy || item.lifecycle_state !== "running"
                ).length
              }
            </p>
          </div>
        </div>
        <InlineActions>
          <label className="mc-checkbox">
            <input
              type="checkbox"
              checked={props.incidentMode}
              onChange={(event) => props.setIncidentMode(event.target.checked)}
            />
            Incident mode
          </label>
          <button
            type="button"
            disabled={isBusyAction("health:refresh-all")}
            onClick={() =>
              runBusyAction("health:refresh-all", async () => {
                await Promise.resolve(props.onRefreshAll());
              })
            }
          >
            {isBusyAction("health:refresh-all") ? "Refreshing..." : "Refresh all"}
          </button>
        </InlineActions>
        <section className="mc-usage-panel" data-testid="mc-usage-panel">
          <header className="mc-usage-head">
            <h4>Cost + Token Usage</h4>
            <span className={`mc-usage-freshness mc-usage-freshness-${props.usageFreshness}`}>
              {props.usageFreshness === "limited"
                ? "stale >60m"
                : props.usageFreshness === "stale"
                  ? "stale >15m"
                  : "fresh"}
            </span>
          </header>

          {!props.usageChartsEnabled ? (
            <p className="mc-usage-unavailable" data-testid="usage-not-available">
              Usage charts are disabled in Runtime Settings.
            </p>
          ) : props.usageUnavailableReason ? (
            <p className="mc-usage-unavailable" data-testid="usage-not-available">
              Not available: {props.usageUnavailableReason}
            </p>
          ) : (
            <>
              <div className="mc-usage-summary-grid">
                <div>
                  <strong>Today Cost</strong>
                  <p data-testid="usage-summary-today-cost">
                    {formatMoney(props.usageToday?.estimatedCostTotal ?? 0, usageCurrency)}
                  </p>
                </div>
                <div>
                  <strong>Week Cost</strong>
                  <p>{formatMoney(props.usageWeek?.estimatedCostTotal ?? 0, usageCurrency)}</p>
                </div>
                <div>
                  <strong>Today Tokens</strong>
                  <p>
                    {formatTokens(
                      (props.usageToday?.tokenInputTotal ?? 0) +
                        (props.usageToday?.tokenOutputTotal ?? 0)
                    )}
                  </p>
                </div>
                <div>
                  <strong>Trend</strong>
                  <p data-testid="usage-trend-label">{props.usageTrend.label}</p>
                </div>
              </div>

              {props.usageFreshness !== "fresh" ? (
                <p className="mc-usage-stale-note">
                  {props.usageFreshness === "limited"
                    ? "Data is older than 60 minutes. Trend claims are limited."
                    : "Data is older than 15 minutes. Validate before acting."}
                </p>
              ) : null}

              {props.usageBudgetWarnings.length > 0 ? (
                <ul className="mc-usage-warning-list">
                  {props.usageBudgetWarnings.map((warning) => (
                    <li
                      key={warning.message}
                      className={warning.tone === "critical" ? "critical" : "warning"}
                    >
                      {warning.message}
                    </li>
                  ))}
                </ul>
              ) : null}

              <div className="mc-usage-breakdown-grid">
                <div>
                  <strong>By Agent</strong>
                  <ul className="mc-usage-breakdown-list">
                    {topAgents.map((item) => (
                      <li key={item.agent_id}>
                        <span>{item.agent_name}</span>
                        <span>{formatMoney(item.estimated_cost_total, usageCurrency)}</span>
                      </li>
                    ))}
                    {topAgents.length === 0 ? <li>No usage yet.</li> : null}
                  </ul>
                </div>
                <div>
                  <strong>By Model</strong>
                  <ul className="mc-usage-breakdown-list">
                    {topModels.map((item) => (
                      <li key={`${item.model_provider}:${item.model_id}`}>
                        <span>{item.model_id}</span>
                        <span>{formatMoney(item.estimated_cost_total, usageCurrency)}</span>
                      </li>
                    ))}
                    {topModels.length === 0 ? <li>No usage yet.</li> : null}
                  </ul>
                </div>
              </div>

              <p className="mc-usage-footnote" data-testid="usage-correlation-status">
                {props.usageCorrelationAvailable
                  ? "Job/card correlation data available."
                  : "Job/card correlation unavailable from gateway contract."}
                {" "}
                {props.usageUpdatedAtUtc
                  ? `Updated ${formatDateTime(Date.parse(props.usageUpdatedAtUtc))}.`
                  : ""}
              </p>
            </>
          )}
        </section>
      </article>
    );
  }

  if (widget.widget === "focus") {
    const items = props.incidentFocusItems.slice(focusPagination.startIndex, focusPagination.endIndex);
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-widget-list-container" ref={focusListRef}>
          <ul className="mc-cockpit-list">
            {items.map((item) => (
              <li key={item.item_id}>
                <div>
                  <strong>{item.title}</strong>
                  <p>{item.detail}</p>
                </div>
                <Chip label={item.severity} tone={item.severity} />
              </li>
            ))}
            {props.incidentFocusItems.length === 0 ? <li>No active items.</li> : null}
          </ul>
        </div>
        <PaginationControls page={focusPagination.page} totalPages={focusPagination.totalPages} onSetPage={focusPagination.setPage} />
      </article>
    );
  }

  if (widget.widget === "breakers") {
    const coreItems = props.openBreakers.slice(breakerCorePagination.startIndex, breakerCorePagination.endIndex);
    const pluginItems = props.openPluginBreakers.slice(breakerPluginPagination.startIndex, breakerPluginPagination.endIndex);
    return (
      <article className="mc-cockpit-widget-body">
        <h4>Core Breakers</h4>
        <div className="mc-widget-list-container" ref={breakerCoreRef}>
          <ul className="mc-cockpit-list compact">
            {coreItems.map((breaker) => (
              <li key={`${breaker.scope}:${breaker.target_id}`}>
                <div>
                  <strong>{breaker.scope}</strong>
                  <p>{breaker.target_id}</p>
                </div>
                <span>{breaker.last_error_code ?? breaker.state}</span>
              </li>
            ))}
            {props.openBreakers.length === 0 ? <li>No open core breakers.</li> : null}
          </ul>
        </div>
        <PaginationControls page={breakerCorePagination.page} totalPages={breakerCorePagination.totalPages} onSetPage={breakerCorePagination.setPage} />
        <h4>Plugin Breakers</h4>
        <div className="mc-widget-list-container" ref={breakerPluginRef}>
          <ul className="mc-cockpit-list compact">
            {pluginItems.map((breaker) => (
              <li key={breaker.plugin_id}>
                <div>
                  <strong>{breaker.plugin_id}</strong>
                  <p>{breaker.last_error ?? "faulted"}</p>
                </div>
                <span>{breaker.last_error_code ?? "faulted"}</span>
              </li>
            ))}
            {props.openPluginBreakers.length === 0 ? <li>No plugin breakers.</li> : null}
          </ul>
        </div>
        <PaginationControls page={breakerPluginPagination.page} totalPages={breakerPluginPagination.totalPages} onSetPage={breakerPluginPagination.setPage} />
      </article>
    );
  }

  if (widget.widget === "jobs") {
    const items = props.calendarJobs.slice(jobsPagination.startIndex, jobsPagination.endIndex);
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-widget-list-container" ref={jobsListRef}>
          <ul className="mc-cockpit-list">
            {items.map((job) => (
              <li key={job.job_id}>
                <div>
                  <strong>{job.name}</strong>
                  <p title={formatDateTime(job.next_run_at)}>{formatRelative(job.next_run_at)}</p>
                </div>
                <InlineActions>
                  <button
                    type="button"
                    disabled={isBusyAction(`job:run:${job.job_id}`)}
                    onClick={() =>
                      runBusyAction(`job:run:${job.job_id}`, () =>
                        props.onRunCalendarJobNow(job.job_id)
                      )
                    }
                  >
                    {isBusyAction(`job:run:${job.job_id}`) ? "Working..." : "Run"}
                  </button>
                  <button
                    type="button"
                    className={job.enabled ? "danger" : ""}
                    disabled={isBusyAction(`job:toggle:${job.job_id}`)}
                    onClick={() =>
                      runBusyAction(`job:toggle:${job.job_id}`, () =>
                        props.onToggleCalendarJob(job.job_id, !job.enabled)
                      )
                    }
                  >
                    {isBusyAction(`job:toggle:${job.job_id}`)
                      ? "Working..."
                      : job.enabled
                        ? "Pause"
                        : "Resume"}
                  </button>
                </InlineActions>
              </li>
            ))}
            {props.calendarJobs.length === 0 ? <li>No scheduled jobs.</li> : null}
          </ul>
        </div>
        <PaginationControls page={jobsPagination.page} totalPages={jobsPagination.totalPages} onSetPage={jobsPagination.setPage} />
      </article>
    );
  }

  if (widget.widget === "channels") {
    const items = props.channelStatuses.slice(channelsPagination.startIndex, channelsPagination.endIndex);
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-widget-list-container" ref={channelsListRef}>
          <ul className="mc-cockpit-list">
            {items.map((item) => (
              <li key={item.provider}>
                <div>
                  <strong>{item.provider}</strong>
                  <p>{item.last_error ?? item.detail ?? item.lifecycle_state}</p>
                </div>
                <button
                  type="button"
                  disabled={isBusyAction(`channel:reconnect:${item.provider}`)}
                  onClick={() =>
                    runBusyAction(`channel:reconnect:${item.provider}`, () =>
                      props.onReconnectFocusChannel(item.provider)
                    )
                  }
                >
                  {isBusyAction(`channel:reconnect:${item.provider}`) ? "Working..." : "Reconnect"}
                </button>
              </li>
            ))}
            {props.channelStatuses.length === 0 ? <li>No channels configured.</li> : null}
          </ul>
        </div>
        <PaginationControls page={channelsPagination.page} totalPages={channelsPagination.totalPages} onSetPage={channelsPagination.setPage} />
      </article>
    );
  }

  if (widget.widget === "profiles") {
    const items = props.orderedProviderProfiles.slice(profilesPagination.startIndex, profilesPagination.endIndex);
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-field-grid">
          <label>
            Agent
            <select
              value={props.selectedProviderControlAgentId}
              disabled={props.agents.length === 0}
              onChange={(event) => props.setSelectedProviderControlAgentId(event.target.value)}
            >
              {props.agents.length === 0 ? (
                <option value="">Select an agent</option>
              ) : null}
              {props.agents.map((agent) => (
                <option key={agent.agent_id} value={agent.agent_id}>
                  {agent.name} ({agent.agent_id})
                </option>
              ))}
            </select>
          </label>
          <label>
            Provider
            <select
              value={props.selectedProviderControlProvider}
              disabled={props.providerOptions.length === 0}
              onChange={(event) => props.setSelectedProviderControlProvider(event.target.value)}
            >
              {props.providerOptions.length === 0 ? (
                <option value="">Select a provider</option>
              ) : null}
              {props.providerOptions.map((provider) => (
                <option key={provider} value={provider}>
                  {provider}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="mc-widget-list-container" ref={profilesListRef}>
          <ul className="mc-cockpit-list">
            {items.map((profile) => (
              <li key={profile.auth_profile_id}>
                <div>
                  <strong>{profile.display_name}</strong>
                  <p>
                    {profile.auth_mode} / {profile.risk_level} / {" "}
                    {profile.enabled ? "enabled" : "disabled"}
                  </p>
                </div>
                <InlineActions>
                  <button
                    type="button"
                    onClick={() => props.onMoveProviderProfile(profile.auth_profile_id, -1)}
                  >
                    Up
                  </button>
                  <button
                    type="button"
                    onClick={() => props.onMoveProviderProfile(profile.auth_profile_id, 1)}
                  >
                    Down
                  </button>
                </InlineActions>
              </li>
            ))}
            {props.orderedProviderProfiles.length === 0 ? <li>No profiles for provider.</li> : null}
          </ul>
        </div>
        <PaginationControls page={profilesPagination.page} totalPages={profilesPagination.totalPages} onSetPage={profilesPagination.setPage} />
        <InlineActions>
          <button
            type="button"
            disabled={isBusyAction("profile:save-order")}
            onClick={() =>
              runBusyAction("profile:save-order", () => props.onSaveProviderOrder())
            }
          >
            {isBusyAction("profile:save-order") ? "Saving..." : "Save Order"}
          </button>
          <button
            type="button"
            disabled={isBusyAction("profile:reload-order")}
            onClick={() =>
              runBusyAction("profile:reload-order", () => props.onReloadProviderProfileOrder())
            }
          >
            {isBusyAction("profile:reload-order") ? "Reloading..." : "Reload"}
          </button>
          {props.providerProfileOrderDirty ? <Chip label="unsaved" tone="error" /> : null}
        </InlineActions>
      </article>
    );
  }

  if (widget.widget === "skills") {
    const items = props.skills.slice(skillsPagination.startIndex, skillsPagination.endIndex);
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-widget-list-container" ref={skillsListRef}>
          <ul className="mc-cockpit-list">
            {items.map((skill) => (
              <li key={skill.skill_id}>
                <div>
                  <strong>{skill.title}</strong>
                  <p>{skill.skill_id}</p>
                </div>
                <button
                  type="button"
                  className={skill.enabled ? "danger" : ""}
                  disabled={isBusyAction(`skill:toggle:${skill.skill_id}`)}
                  onClick={() =>
                    runBusyAction(`skill:toggle:${skill.skill_id}`, () =>
                      props.onToggleSkillState(skill.skill_id, !skill.enabled)
                    )
                  }
                >
                  {isBusyAction(`skill:toggle:${skill.skill_id}`)
                    ? "Working..."
                    : skill.enabled
                      ? "Disable"
                      : "Enable"}
                </button>
              </li>
            ))}
            {props.skills.length === 0 ? <li>No skills loaded.</li> : null}
          </ul>
        </div>
        <PaginationControls page={skillsPagination.page} totalPages={skillsPagination.totalPages} onSetPage={skillsPagination.setPage} />
      </article>
    );
  }

  if (widget.widget === "plugins") {
    const items = props.plugins.slice(pluginsPagination.startIndex, pluginsPagination.endIndex);
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-widget-list-container" ref={pluginsListRef}>
          <ul className="mc-cockpit-list">
            {items.map((plugin) => {
              const runtime = props.pluginRuntimeById.get(plugin.plugin_id);
              return (
                <li key={plugin.plugin_id}>
                  <div>
                    <strong>{plugin.display_name}</strong>
                    <p>
                      {plugin.plugin_id} /{" "}
                      {runtime ? (runtime.faulted ? "faulted" : "ok") : "unknown"}
                    </p>
                  </div>
                  <button
                    type="button"
                    className={plugin.enabled ? "danger" : ""}
                    disabled={isBusyAction(`plugin:toggle:${plugin.plugin_id}`)}
                    onClick={() =>
                      runBusyAction(`plugin:toggle:${plugin.plugin_id}`, () =>
                        props.onTogglePluginState(plugin.plugin_id, !plugin.enabled)
                      )
                    }
                  >
                    {isBusyAction(`plugin:toggle:${plugin.plugin_id}`)
                      ? "Working..."
                      : plugin.enabled
                        ? "Disable"
                        : "Enable"}
                  </button>
                </li>
              );
            })}
            {props.plugins.length === 0 ? <li>No plugins installed.</li> : null}
          </ul>
        </div>
        <PaginationControls page={pluginsPagination.page} totalPages={pluginsPagination.totalPages} onSetPage={pluginsPagination.setPage} />
      </article>
    );
  }

  // Events (default/fallback)
  const eventItems = props.visibleEvents.slice(eventsPagination.startIndex, eventsPagination.endIndex);
  return (
    <article className="mc-cockpit-widget-body">
      <div className="mc-widget-list-container" ref={eventsListRef}>
        <div className="mc-events compact">
          {eventItems.map((event) => (
            <article key={event.event_id} className="mc-event-item">
              <div className="mc-event-head">
                <span>{event.event_type}</span>
                <span title={formatDateTime(event.ts_unix_ms)}>{formatRelative(event.ts_unix_ms)}</span>
              </div>
            </article>
          ))}
          {props.visibleEvents.length === 0 ? (
            <EmptyState className="mc-empty-events" message="No events captured." />
          ) : null}
        </div>
      </div>
      <PaginationControls page={eventsPagination.page} totalPages={eventsPagination.totalPages} onSetPage={eventsPagination.setPage} />
    </article>
  );
}
