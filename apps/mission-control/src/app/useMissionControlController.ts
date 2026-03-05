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
  getMissionControlUsage,
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
  MissionControlUsageBudgetThreshold,
  MissionControlUsageByAgent,
  MissionControlUsageByModel,
  MissionControlUsageResponse,
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

type UsageStaleLevel = "fresh" | "stale" | "limited";

interface UsageWindowContract {
  window: "today" | "week";
  timezone: string;
  currency: string;
  windowStartUtc: string;
  windowEndUtc: string;
  estimatedCostTotal: number;
  tokenInputTotal: number;
  tokenOutputTotal: number;
  byAgent: MissionControlUsageByAgent[];
  byModel: MissionControlUsageByModel[];
  byTime: NonNullable<MissionControlUsageResponse["by_time"]>;
  byJob: MissionControlUsageResponse["by_job"];
  byCard: MissionControlUsageResponse["by_card"];
  budgetThresholds: MissionControlUsageBudgetThreshold[];
  updatedAtUtc: string;
}

interface UsageBudgetWarning {
  tone: "warning" | "critical";
  message: string;
}

interface UsageTrendSummary {
  direction: "up" | "down" | "flat" | "limited" | "unknown";
  label: string;
}

function resolveOperatorTimezone(): string {
  try {
    const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
    return typeof tz === "string" && tz.trim().length > 0 ? tz : "UTC";
  } catch {
    return "UTC";
  }
}

function resolveTzOffsetMinutes(): number {
  return -new Date().getTimezoneOffset();
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function parseUsageWindowContract(
  raw: MissionControlUsageResponse | null,
  expectedWindow: "today" | "week"
): { data: UsageWindowContract | null; reason: string | null } {
  if (!raw) {
    return { data: null, reason: `Usage contract missing for ${expectedWindow}.` };
  }
  if (raw.available !== true) {
    const detail = raw.detail?.trim();
    const reasonCode = raw.reason_code?.trim();
    const reason = detail || reasonCode || "Usage contract unavailable.";
    return { data: null, reason };
  }
  if (raw.window !== expectedWindow) {
    return {
      data: null,
      reason: `Usage contract window mismatch: expected ${expectedWindow}, got ${raw.window}.`,
    };
  }
  if (
    typeof raw.window_start_utc !== "string" ||
    typeof raw.window_end_utc !== "string" ||
    !isFiniteNumber(raw.estimated_cost_total) ||
    !isFiniteNumber(raw.token_input_total) ||
    !isFiniteNumber(raw.token_output_total) ||
    !Array.isArray(raw.by_agent) ||
    !Array.isArray(raw.by_model) ||
    !Array.isArray(raw.by_time) ||
    typeof raw.updated_at_utc !== "string"
  ) {
    return {
      data: null,
      reason: `Usage contract invalid for ${expectedWindow}: missing required fields.`,
    };
  }
  return {
    data: {
      window: expectedWindow,
      timezone: raw.timezone,
      currency: raw.currency,
      windowStartUtc: raw.window_start_utc,
      windowEndUtc: raw.window_end_utc,
      estimatedCostTotal: raw.estimated_cost_total,
      tokenInputTotal: raw.token_input_total,
      tokenOutputTotal: raw.token_output_total,
      byAgent: raw.by_agent,
      byModel: raw.by_model,
      byTime: raw.by_time,
      byJob: raw.by_job,
      byCard: raw.by_card,
      budgetThresholds: Array.isArray(raw.budget_thresholds) ? raw.budget_thresholds : [],
      updatedAtUtc: raw.updated_at_utc,
    },
    reason: null,
  };
}

function usageUnavailableFallback(
  window: "today" | "week",
  timezone: string,
  detail: string
): MissionControlUsageResponse {
  return {
    contract_version: "mc-usage-v1",
    available: false,
    window,
    timezone,
    currency: "USD",
    window_start_utc: null,
    window_end_utc: null,
    estimated_cost_total: null,
    token_input_total: null,
    token_output_total: null,
    by_agent: null,
    by_model: null,
    by_provider: null,
    by_time: null,
    by_job: null,
    by_card: null,
    budget_thresholds: null,
    updated_at_utc: null,
    reason_code: "USAGE_ENDPOINT_UNAVAILABLE",
    detail,
  };
}

function parseIsoMs(value: string | null | undefined): number | null {
  if (!value) {
    return null;
  }
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function usageStaleLevel(updatedAtUtc: string | null): UsageStaleLevel {
  const updatedAtMs = parseIsoMs(updatedAtUtc);
  if (updatedAtMs === null) {
    return "limited";
  }
  const ageMs = Date.now() - updatedAtMs;
  if (ageMs > 60 * 60_000) {
    return "limited";
  }
  if (ageMs > 15 * 60_000) {
    return "stale";
  }
  return "fresh";
}

function computeUsageTrend(
  today: UsageWindowContract | null,
  week: UsageWindowContract | null,
  staleLevel: UsageStaleLevel
): UsageTrendSummary {
  if (!today || !week) {
    return { direction: "unknown", label: "Trend unavailable" };
  }
  if (staleLevel === "limited") {
    return { direction: "limited", label: "Trend limited (stale data)" };
  }
  const buckets = week.byTime.map((item) => item.estimated_cost_total);
  if (buckets.length < 2) {
    return { direction: "unknown", label: "Trend unavailable" };
  }
  const prior = buckets.slice(0, -1);
  const priorAvg = prior.reduce((sum, value) => sum + value, 0) / Math.max(1, prior.length);
  const todayCost = today.estimatedCostTotal;
  if (priorAvg <= 0 && todayCost <= 0) {
    return { direction: "flat", label: "Flat vs recent average" };
  }
  if (priorAvg <= 0 && todayCost > 0) {
    return { direction: "up", label: "Up vs recent average" };
  }
  const ratio = todayCost / priorAvg;
  if (ratio >= 1.15) {
    return { direction: "up", label: "Up vs recent average" };
  }
  if (ratio <= 0.85) {
    return { direction: "down", label: "Down vs recent average" };
  }
  return { direction: "flat", label: "Flat vs recent average" };
}

function buildBudgetWarnings(
  thresholds: MissionControlUsageBudgetThreshold[]
): UsageBudgetWarning[] {
  const warnings: UsageBudgetWarning[] = [];
  for (const entry of thresholds) {
    const highestRatio = Math.max(
      entry.cost_ratio ?? 0,
      entry.token_ratio ?? 0
    );
    if (highestRatio < 0.8) {
      continue;
    }
    const pct = Math.round(highestRatio * 100);
    if (highestRatio >= 1.0) {
      warnings.push({
        tone: "critical",
        message: `${entry.provider}: budget exceeded (${pct}%).`,
      });
      continue;
    }
    warnings.push({
      tone: "warning",
      message: `${entry.provider}: budget nearing limit (${pct}%).`,
    });
  }
  return warnings.slice(0, 2);
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
  const [usageTodayRaw, setUsageTodayRaw] = useState<MissionControlUsageResponse | null>(null);
  const [usageWeekRaw, setUsageWeekRaw] = useState<MissionControlUsageResponse | null>(null);
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
      const timezone = resolveOperatorTimezone();
      const tzOffsetMinutes = resolveTzOffsetMinutes();
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
        usageTodayResponse,
        usageWeekResponse,
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
        getMissionControlUsage(runtimeSettings, {
          window: "today",
          timezone,
          tz_offset_minutes: tzOffsetMinutes,
        }).catch((error: unknown) =>
          usageUnavailableFallback("today", timezone, String(error))
        ),
        getMissionControlUsage(runtimeSettings, {
          window: "week",
          timezone,
          tz_offset_minutes: tzOffsetMinutes,
        }).catch((error: unknown) =>
          usageUnavailableFallback("week", timezone, String(error))
        ),
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
      setUsageTodayRaw(usageTodayResponse);
      setUsageWeekRaw(usageWeekResponse);
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
  const usageToday = useMemo(
    () => parseUsageWindowContract(usageTodayRaw, "today"),
    [usageTodayRaw]
  );
  const usageWeek = useMemo(
    () => parseUsageWindowContract(usageWeekRaw, "week"),
    [usageWeekRaw]
  );
  const usageUnavailableReason = useMemo(() => {
    return usageToday.reason ?? usageWeek.reason ?? null;
  }, [usageToday.reason, usageWeek.reason]);
  const usageAvailable = usageToday.data !== null && usageWeek.data !== null;
  const usageCorrelationAvailable = useMemo(() => {
    if (!usageAvailable) {
      return false;
    }
    return Array.isArray(usageToday.data?.byJob) && Array.isArray(usageToday.data?.byCard);
  }, [usageAvailable, usageToday.data]);
  const usageUpdatedAtUtc = useMemo(() => {
    const timestamps = [usageToday.data?.updatedAtUtc, usageWeek.data?.updatedAtUtc]
      .map((value) => parseIsoMs(value))
      .filter((value): value is number => value !== null);
    if (timestamps.length === 0) {
      return null;
    }
    return new Date(Math.max(...timestamps)).toISOString();
  }, [usageToday.data, usageWeek.data]);
  const usageFreshness = useMemo(
    () => usageStaleLevel(usageUpdatedAtUtc),
    [usageUpdatedAtUtc]
  );
  const usageTrend = useMemo(
    () => computeUsageTrend(usageToday.data, usageWeek.data, usageFreshness),
    [usageFreshness, usageToday.data, usageWeek.data]
  );
  const usageBudgetWarnings = useMemo(
    () => buildBudgetWarnings(usageToday.data?.budgetThresholds ?? []),
    [usageToday.data]
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
    usageToday: usageToday.data,
    usageWeek: usageWeek.data,
    usageAvailable,
    usageUnavailableReason,
    usageCorrelationAvailable,
    usageFreshness,
    usageTrend,
    usageBudgetWarnings,
    usageUpdatedAtUtc,
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
