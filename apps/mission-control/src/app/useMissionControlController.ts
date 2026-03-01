import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  getAgentProviderProfileOrder,
  getChannelRuntimeStatus,
  getJobsStatus,
  getGatewayStatus,
  getMissionControlCalendarWeek,
  getMissionControlFocus,
  listApprovals,
  listAuthProfiles,
  listJobs,
  listPlugins,
  listPluginRuntimeStatus,
  listSkills,
  reconnectChannelRuntime,
  resolveApproval,
  runJobNow,
  setAgentProviderProfileOrder,
  setJobEnabledState,
  setPluginEnabled,
  setSkillEnabled,
} from "../lib/api";
import type { NotifyFn } from "./useAppController";
import type {
  Agent,
  AuthProfileResponse,
  CircuitBreakerStateResponse,
  ChannelRuntimeAdapterStatusResponse,
  JobStatusResponse,
  MissionControlCalendarJob,
  MissionControlCalendarWeekResponse,
  MissionControlFocusItem,
  PluginManifestResponse,
  PluginRuntimeStatusResponse,
  RuntimeConnectionSettings,
  SkillResponse,
  StatusResponse,
} from "../types";

interface UseMissionControlControllerOptions {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  incidentMode: boolean;
  setNotice: NotifyFn;
}

export function useMissionControlController(options: UseMissionControlControllerOptions) {
  const { settings, agents, incidentMode, setNotice } = options;

  const [calendarWeek, setCalendarWeek] = useState<MissionControlCalendarWeekResponse | null>(
    null
  );
  const [focusItems, setFocusItems] = useState<MissionControlFocusItem[]>([]);
  const [channelStatuses, setChannelStatuses] = useState<ChannelRuntimeAdapterStatusResponse[]>(
    []
  );
  const [jobsById, setJobsById] = useState<Map<string, MissionControlCalendarJob>>(new Map());
  const [approvalsById, setApprovalsById] = useState<Set<string>>(new Set());
  const [gatewayStatus, setGatewayStatus] = useState<StatusResponse | null>(null);
  const [jobsStatus, setJobsStatus] = useState<JobStatusResponse | null>(null);
  const [authProfiles, setAuthProfiles] = useState<AuthProfileResponse[]>([]);
  const [skills, setSkills] = useState<SkillResponse[]>([]);
  const [plugins, setPlugins] = useState<PluginManifestResponse[]>([]);
  const [pluginRuntimeById, setPluginRuntimeById] = useState<
    Map<string, PluginRuntimeStatusResponse>
  >(new Map());
  const [selectedProviderControlAgentId, setSelectedProviderControlAgentId] = useState("");
  const [selectedProviderControlProvider, setSelectedProviderControlProvider] = useState("");
  const [providerProfileOrder, setProviderProfileOrder] = useState<string[]>([]);
  const [providerProfileOrderDirty, setProviderProfileOrderDirty] = useState(false);

  const missionControlRefreshTimer = useRef<number | null>(null);
  const providerOrderRequestIdRef = useRef(0);

  const providerOptions = useMemo(() => {
    return [...new Set(authProfiles.map((profile) => profile.provider))].sort((a, b) =>
      a.localeCompare(b)
    );
  }, [authProfiles]);

  const selectedProviderControlAgentIdEffective = useMemo(() => {
    if (agents.length === 0) {
      return "";
    }
    if (
      selectedProviderControlAgentId &&
      agents.some((agent) => agent.agent_id === selectedProviderControlAgentId)
    ) {
      return selectedProviderControlAgentId;
    }
    return agents[0].agent_id;
  }, [agents, selectedProviderControlAgentId]);

  const selectedProviderControlProviderEffective = useMemo(() => {
    if (providerOptions.length === 0) {
      return "";
    }
    if (
      selectedProviderControlProvider &&
      providerOptions.includes(selectedProviderControlProvider)
    ) {
      return selectedProviderControlProvider;
    }
    return providerOptions[0];
  }, [providerOptions, selectedProviderControlProvider]);

  const providerProfiles = useMemo(() => {
    if (!selectedProviderControlProviderEffective) {
      return [] as AuthProfileResponse[];
    }
    return authProfiles
      .filter((profile) => profile.provider === selectedProviderControlProviderEffective)
      .sort((left, right) => left.display_name.localeCompare(right.display_name));
  }, [authProfiles, selectedProviderControlProviderEffective]);

  const orderedProviderProfiles = useMemo(() => {
    if (providerProfiles.length === 0) {
      return [] as AuthProfileResponse[];
    }
    const byId = new Map(providerProfiles.map((profile) => [profile.auth_profile_id, profile]));
    const ordered: AuthProfileResponse[] = [];
    for (const profileId of providerProfileOrder) {
      const match = byId.get(profileId);
      if (match) {
        ordered.push(match);
        byId.delete(profileId);
      }
    }
    const remaining = [...byId.values()].sort((left, right) =>
      left.display_name.localeCompare(right.display_name)
    );
    return [...ordered, ...remaining];
  }, [providerProfiles, providerProfileOrder]);

  const loadMissionControlReadModels = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      const [
        calendar,
        focus,
        jobs,
        approvals,
        channelRuntime,
        status,
        jobsStatusResponse,
        profiles,
        skillResponse,
        pluginResponse,
        pluginRuntimeResponse,
      ] = await Promise.all([
        getMissionControlCalendarWeek(runtimeSettings),
        getMissionControlFocus(runtimeSettings, 100),
        listJobs(runtimeSettings, 200),
        listApprovals(runtimeSettings, "requested", 200),
        getChannelRuntimeStatus(runtimeSettings),
        getGatewayStatus(runtimeSettings),
        getJobsStatus(runtimeSettings),
        listAuthProfiles(runtimeSettings, { includeDisabled: true }),
        listSkills(runtimeSettings, true),
        listPlugins(runtimeSettings, true),
        listPluginRuntimeStatus(runtimeSettings, true),
      ]);
      setCalendarWeek(calendar);
      setFocusItems(focus.items);
      setJobsById(
        new Map(
          jobs.items.map((item) => [
            item.job_id,
            {
              job_id: item.job_id,
              name: item.name,
              agent_id: item.agent_id,
              enabled: item.enabled,
              schedule_kind: item.schedule_kind,
              interval_seconds: item.interval_seconds,
              cron_expr: item.cron_expr,
              next_run_at: item.next_run_at,
              last_run_at: item.last_run_at,
              last_error: item.last_error,
              lane:
                item.enabled &&
                item.schedule_kind === "interval" &&
                (item.interval_seconds ?? 0) <= 300 &&
                (item.interval_seconds ?? 0) > 0
                  ? "always_running"
                  : "scheduled",
              primary_action: item.enabled ? "pause" : "resume",
            } satisfies MissionControlCalendarJob,
          ])
        )
      );
      setApprovalsById(new Set(approvals.items.map((item) => item.approval_id)));
      setChannelStatuses(channelRuntime.items);
      setGatewayStatus(status);
      setJobsStatus(jobsStatusResponse);
      setAuthProfiles(profiles);
      setSkills(skillResponse.items);
      setPlugins(pluginResponse.items);
      setPluginRuntimeById(
        new Map(pluginRuntimeResponse.items.map((item) => [item.plugin_id, item]))
      );
    },
    [settings]
  );

  const queueMissionControlRefresh = useCallback(
    (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (missionControlRefreshTimer.current) {
        globalThis.clearTimeout(missionControlRefreshTimer.current);
      }
      missionControlRefreshTimer.current = globalThis.setTimeout(() => {
        void loadMissionControlReadModels(runtimeSettings).catch((error: unknown) => {
          setNotice({
            tone: "error",
            message: `Mission Control refresh failed: ${String(error)}`,
          });
        });
      }, 300);
    },
    [loadMissionControlReadModels, setNotice, settings]
  );

  const reloadProviderProfileOrder = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      agentId: string = selectedProviderControlAgentIdEffective,
      provider: string = selectedProviderControlProviderEffective
    ) => {
      const requestId = providerOrderRequestIdRef.current + 1;
      providerOrderRequestIdRef.current = requestId;
      if (!agentId || !provider) {
        setProviderProfileOrder([]);
        setProviderProfileOrderDirty(false);
        return;
      }
      const response = await getAgentProviderProfileOrder(runtimeSettings, agentId, provider);
      if (requestId !== providerOrderRequestIdRef.current) {
        return;
      }
      setProviderProfileOrder(response.profile_ids);
      setProviderProfileOrderDirty(false);
    },
    [
      selectedProviderControlAgentIdEffective,
      selectedProviderControlProviderEffective,
      settings,
    ]
  );

  useEffect(() => {
    if (
      !settings.gateway_url.trim() ||
      !selectedProviderControlAgentIdEffective ||
      !selectedProviderControlProviderEffective
    ) {
      return;
    }
    const timer = globalThis.setTimeout(() => {
      void reloadProviderProfileOrder(settings).catch((error: unknown) => {
        setNotice({
          tone: "error",
          message: `Profile order load failed: ${String(error)}`,
        });
      });
    }, 0);
    return () => {
      globalThis.clearTimeout(timer);
    };
  }, [
    reloadProviderProfileOrder,
    selectedProviderControlAgentIdEffective,
    selectedProviderControlProviderEffective,
    setNotice,
    settings,
  ]);

  useEffect(() => {
    return () => {
      if (missionControlRefreshTimer.current) {
        globalThis.clearTimeout(missionControlRefreshTimer.current);
      }
    };
  }, []);

  const runCalendarJobNow = useCallback(
    async (jobId: string) => {
      try {
        const response = await runJobNow(settings, jobId);
        setNotice({
          tone: "info",
          message: `Job run started (${response.job_run.status})`,
        });
        queueMissionControlRefresh(settings);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Run-now failed: ${String(error)}`,
        });
      }
    },
    [queueMissionControlRefresh, setNotice, settings]
  );

  const toggleCalendarJob = useCallback(
    async (jobId: string, enabled: boolean) => {
      try {
        const response = await setJobEnabledState(settings, jobId, enabled);
        setJobsById((previous) => {
          const next = new Map(previous);
          const existing = next.get(jobId);
          if (existing) {
            next.set(jobId, {
              ...existing,
              enabled: response.job.enabled,
              primary_action: response.job.enabled ? "pause" : "resume",
              next_run_at: response.job.next_run_at,
              last_error: response.job.last_error,
            });
          }
          return next;
        });
        setNotice({
          tone: "info",
          message: enabled ? "Job resumed." : "Job paused.",
        });
        queueMissionControlRefresh(settings);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Job state update failed: ${String(error)}`,
        });
      }
    },
    [queueMissionControlRefresh, setNotice, settings]
  );

  const resolveFocusApproval = useCallback(
    async (approvalId: string, decision: "approve" | "deny") => {
      try {
        const response = await resolveApproval(settings, approvalId, decision);
        setApprovalsById((previous) => {
          const next = new Set(previous);
          if (response.approval.status !== "requested") {
            next.delete(approvalId);
          }
          return next;
        });
        setNotice({
          tone: "info",
          message: `Approval ${decision === "deny" ? "denied" : "approved"}.`,
        });
        queueMissionControlRefresh(settings);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Approval ${decision} failed: ${String(error)}`,
        });
      }
    },
    [queueMissionControlRefresh, setNotice, settings]
  );

  const reconnectFocusChannel = useCallback(
    async (provider: string) => {
      try {
        await reconnectChannelRuntime(settings, provider);
        setNotice({
          tone: "info",
          message: `Channel reconnect requested for ${provider}.`,
        });
        queueMissionControlRefresh(settings);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Channel reconnect failed: ${String(error)}`,
        });
      }
    },
    [queueMissionControlRefresh, setNotice, settings]
  );

  const moveProviderProfile = useCallback(
    (profileId: string, delta: number) => {
      const currentOrder =
        providerProfileOrder.length > 0
          ? providerProfileOrder
          : orderedProviderProfiles.map((item) => item.auth_profile_id);
      const nextOrder = [...currentOrder];
      const index = nextOrder.findIndex((item) => item === profileId);
      if (index < 0) {
        return;
      }
      const target = Math.max(0, Math.min(nextOrder.length - 1, index + delta));
      if (target === index) {
        return;
      }
      const [entry] = nextOrder.splice(index, 1);
      nextOrder.splice(target, 0, entry);
      setProviderProfileOrder(nextOrder);
      setProviderProfileOrderDirty(true);
    },
    [orderedProviderProfiles, providerProfileOrder]
  );

  const saveProviderOrder = useCallback(async () => {
    if (
      !selectedProviderControlAgentIdEffective ||
      !selectedProviderControlProviderEffective
    ) {
      return;
    }
    try {
      const response = await setAgentProviderProfileOrder(
        settings,
        selectedProviderControlAgentIdEffective,
        selectedProviderControlProviderEffective,
        providerProfileOrder
      );
      setProviderProfileOrder(response.profile_ids);
      setProviderProfileOrderDirty(false);
      setNotice({
        tone: "info",
        message: `Saved provider order for ${response.agent_id}/${response.provider}.`,
      });
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Saving provider order failed: ${String(error)}`,
      });
    }
  }, [
    providerProfileOrder,
    selectedProviderControlAgentIdEffective,
    selectedProviderControlProviderEffective,
    setNotice,
    settings,
  ]);

  const toggleSkillState = useCallback(
    async (skillId: string, enabled: boolean) => {
      try {
        const response = await setSkillEnabled(settings, skillId, enabled);
        setSkills((previous) =>
          previous.map((item) => (item.skill_id === skillId ? response.skill : item))
        );
        setNotice({
          tone: "info",
          message: enabled ? `Skill enabled: ${skillId}` : `Skill disabled: ${skillId}`,
        });
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Skill update failed: ${String(error)}`,
        });
      }
    },
    [setNotice, settings]
  );

  const togglePluginState = useCallback(
    async (pluginId: string, enabled: boolean) => {
      const target = plugins.find((item) => item.plugin_id === pluginId);
      if (!target) {
        setNotice({
          tone: "error",
          message: `Plugin not found: ${pluginId}`,
        });
        return;
      }
      try {
        const response = await setPluginEnabled(
          settings,
          target,
          enabled,
          enabled ? "mission-control-enable" : "mission-control-disable"
        );
        setPlugins((previous) =>
          previous.map((item) => (item.plugin_id === pluginId ? response.plugin : item))
        );
        setNotice({
          tone: "info",
          message: enabled ? `Plugin enabled: ${pluginId}` : `Plugin disabled: ${pluginId}`,
        });
        queueMissionControlRefresh(settings);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Plugin update failed: ${String(error)}`,
        });
      }
    },
    [plugins, queueMissionControlRefresh, setNotice, settings]
  );

  const incidentFocusItems = useMemo(() => {
    return focusItems.filter((item) =>
      incidentMode
        ? ["critical", "high", "error"].includes(item.severity.toLowerCase())
        : true
    );
  }, [focusItems, incidentMode]);

  const openBreakers = useMemo(() => {
    const fromStatus = gatewayStatus?.circuit_breakers ?? [];
    const fromJobs = jobsStatus?.circuit_breakers ?? [];
    const merged = new Map<string, CircuitBreakerStateResponse>();
    for (const item of [...fromStatus, ...fromJobs]) {
      const key = `${item.scope}:${item.target_id}`;
      merged.set(key, item);
    }
    return [...merged.values()].filter((item) => item.state.toLowerCase() === "open");
  }, [gatewayStatus, jobsStatus]);

  const openPluginBreakers = useMemo(() => {
    return [...pluginRuntimeById.values()].filter((item) => item.faulted);
  }, [pluginRuntimeById]);

  const calendarAlwaysRunning = useMemo(
    () => calendarWeek?.always_running ?? [],
    [calendarWeek]
  );
  const calendarNextUp = useMemo(() => calendarWeek?.next_up ?? [], [calendarWeek]);
  const calendarJobs = useMemo(
    () => calendarWeek?.jobs ?? Array.from(jobsById.values()),
    [calendarWeek, jobsById]
  );

  return {
    calendarWeek,
    focusItems,
    incidentFocusItems,
    channelStatuses,
    approvalsById,
    gatewayStatus,
    jobsStatus,
    authProfiles,
    skills,
    plugins,
    pluginRuntimeById,
    selectedProviderControlAgentId: selectedProviderControlAgentIdEffective,
    setSelectedProviderControlAgentId,
    selectedProviderControlProvider: selectedProviderControlProviderEffective,
    setSelectedProviderControlProvider,
    providerProfileOrderDirty,
    providerOptions,
    orderedProviderProfiles,
    openBreakers,
    openPluginBreakers,
    calendarAlwaysRunning,
    calendarNextUp,
    calendarJobs,
    loadMissionControlReadModels,
    queueMissionControlRefresh,
    reloadProviderProfileOrder,
    runCalendarJobNow,
    toggleCalendarJob,
    resolveFocusApproval,
    reconnectFocusChannel,
    moveProviderProfile,
    saveProviderOrder,
    toggleSkillState,
    togglePluginState,
  };
}
