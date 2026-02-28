import { formatDateTime } from "../../utils/datetime";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { InlineActions } from "../../ui/InlineActions";
import type {
  Agent,
  AuthProfileResponse,
  ChannelRuntimeAdapterStatusResponse,
  CircuitBreakerStateResponse,
  JobStatusResponse,
  MissionControlCalendarJob,
  MissionControlFocusItem,
  PluginManifestResponse,
  PluginRuntimeStatusResponse,
  SkillResponse,
  StatusResponse,
} from "../../types";
import type { EventStreamItem } from "../../app/useAppController";
import type { CockpitWidgetLayout } from "./cockpitLayout";

interface CockpitWidgetRendererProps {
  widget: CockpitWidgetLayout;
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

export function CockpitWidgetRenderer(props: CockpitWidgetRendererProps) {
  const { widget } = props;

  if (widget.widget === "health") {
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
          <button type="button" onClick={props.onRefreshAll}>
            Refresh all
          </button>
        </InlineActions>
      </article>
    );
  }

  if (widget.widget === "focus") {
    return (
      <article className="mc-cockpit-widget-body">
        <ul className="mc-cockpit-list">
          {props.incidentFocusItems.slice(0, 8).map((item) => (
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
      </article>
    );
  }

  if (widget.widget === "breakers") {
    return (
      <article className="mc-cockpit-widget-body">
        <h4>Core Breakers</h4>
        <ul className="mc-cockpit-list compact">
          {props.openBreakers.slice(0, 6).map((breaker) => (
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
        <h4>Plugin Breakers</h4>
        <ul className="mc-cockpit-list compact">
          {props.openPluginBreakers.slice(0, 6).map((breaker) => (
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
      </article>
    );
  }

  if (widget.widget === "jobs") {
    return (
      <article className="mc-cockpit-widget-body">
        <ul className="mc-cockpit-list">
          {props.calendarJobs.slice(0, 10).map((job) => (
            <li key={job.job_id}>
              <div>
                <strong>{job.name}</strong>
                <p>{formatDateTime(job.next_run_at)}</p>
              </div>
              <InlineActions>
                <button type="button" onClick={() => void props.onRunCalendarJobNow(job.job_id)}>
                  Run
                </button>
                <button
                  type="button"
                  className={job.enabled ? "danger" : ""}
                  onClick={() => void props.onToggleCalendarJob(job.job_id, !job.enabled)}
                >
                  {job.enabled ? "Pause" : "Resume"}
                </button>
              </InlineActions>
            </li>
          ))}
          {props.calendarJobs.length === 0 ? <li>No scheduled jobs.</li> : null}
        </ul>
      </article>
    );
  }

  if (widget.widget === "channels") {
    return (
      <article className="mc-cockpit-widget-body">
        <ul className="mc-cockpit-list">
          {props.channelStatuses.map((item) => (
            <li key={item.provider}>
              <div>
                <strong>{item.provider}</strong>
                <p>{item.last_error ?? item.detail ?? item.lifecycle_state}</p>
              </div>
              <button type="button" onClick={() => void props.onReconnectFocusChannel(item.provider)}>
                Reconnect
              </button>
            </li>
          ))}
        </ul>
      </article>
    );
  }

  if (widget.widget === "profiles") {
    return (
      <article className="mc-cockpit-widget-body">
        <div className="mc-field-grid">
          <label>
            Agent
            <select
              value={props.selectedProviderControlAgentId}
              onChange={(event) => props.setSelectedProviderControlAgentId(event.target.value)}
            >
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
              onChange={(event) => props.setSelectedProviderControlProvider(event.target.value)}
            >
              {props.providerOptions.map((provider) => (
                <option key={provider} value={provider}>
                  {provider}
                </option>
              ))}
            </select>
          </label>
        </div>
        <ul className="mc-cockpit-list">
          {props.orderedProviderProfiles.map((profile) => (
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
        <InlineActions>
          <button type="button" onClick={() => void props.onSaveProviderOrder()}>
            Save Order
          </button>
          <button type="button" onClick={() => void props.onReloadProviderProfileOrder()}>
            Reload
          </button>
          {props.providerProfileOrderDirty ? <Chip label="unsaved" tone="error" /> : null}
        </InlineActions>
      </article>
    );
  }

  if (widget.widget === "skills") {
    return (
      <article className="mc-cockpit-widget-body">
        <ul className="mc-cockpit-list">
          {props.skills.map((skill) => (
            <li key={skill.skill_id}>
              <div>
                <strong>{skill.title}</strong>
                <p>{skill.skill_id}</p>
              </div>
              <button
                type="button"
                className={skill.enabled ? "danger" : ""}
                onClick={() => void props.onToggleSkillState(skill.skill_id, !skill.enabled)}
              >
                {skill.enabled ? "Disable" : "Enable"}
              </button>
            </li>
          ))}
          {props.skills.length === 0 ? <li>No skills loaded.</li> : null}
        </ul>
      </article>
    );
  }

  if (widget.widget === "plugins") {
    return (
      <article className="mc-cockpit-widget-body">
        <ul className="mc-cockpit-list">
          {props.plugins.map((plugin) => {
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
                  onClick={() => void props.onTogglePluginState(plugin.plugin_id, !plugin.enabled)}
                >
                  {plugin.enabled ? "Disable" : "Enable"}
                </button>
              </li>
            );
          })}
          {props.plugins.length === 0 ? <li>No plugins installed.</li> : null}
        </ul>
      </article>
    );
  }

  return (
    <article className="mc-cockpit-widget-body">
      <div className="mc-events compact">
        {props.visibleEvents.slice(0, 24).map((event) => (
          <article key={event.event_id} className="mc-event-item">
            <div className="mc-event-head">
              <span>{event.event_type}</span>
              <span>{formatDateTime(event.ts_unix_ms)}</span>
            </div>
          </article>
        ))}
        {props.visibleEvents.length === 0 ? (
          <EmptyState className="mc-empty-events" message="No events captured." />
        ) : null}
      </div>
    </article>
  );
}
