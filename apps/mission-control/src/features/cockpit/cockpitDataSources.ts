/* ── Cockpit Data Source Registry ──────────────────────────────────────────── */

export interface CockpitDataSourceParam {
  key: string;
  label: string;
  resolver: string;
  resolverLabelField: string;
  resolverValueField: string;
}

export interface CockpitDataSource {
  id: string;
  label: string;
  category: string;
  description: string;
  responseShape: "object" | "array";
  sampleFields: string[];
  params?: CockpitDataSourceParam[];
}

export const COCKPIT_DATA_SOURCES: CockpitDataSource[] = [
  /* ── Health ── */
  {
    id: "getGatewayHealth",
    label: "Gateway Health",
    category: "Health",
    description: "Basic health check (status, version, uptime).",
    responseShape: "object",
    sampleFields: ["status", "service", "ok"],
  },
  {
    id: "getGatewayStatus",
    label: "Gateway Status",
    category: "Health",
    description: "Operational summary (service, agents, sessions, channels).",
    responseShape: "object",
    sampleFields: ["service", "version", "uptime_ms", "open_circuit_breakers"],
  },
  {
    id: "getJobsStatus",
    label: "Scheduler Status",
    category: "Health",
    description: "Scheduler running state and job counts.",
    responseShape: "object",
    sampleFields: ["scheduler_running", "jobs_total", "jobs_enabled", "jobs_due"],
  },
  {
    id: "getChannelRuntimeStatus",
    label: "Channel Runtime Status",
    category: "Health",
    description: "All channel adapter health and lifecycle states.",
    responseShape: "object",
    sampleFields: ["updated_at", "items"],
  },

  /* ── Agents ── */
  {
    id: "listAgents",
    label: "List Agents",
    category: "Agents",
    description: "All registered agents with their configuration.",
    responseShape: "object",
    sampleFields: ["items"],
  },
  {
    id: "getAgentProviderProfileOrder",
    label: "Agent Provider Profile Order",
    category: "Agents",
    description: "Provider profile priority order for a specific agent.",
    responseShape: "object",
    sampleFields: ["agent_id", "provider", "profile_ids"],
    params: [
      {
        key: "agentId",
        label: "Agent",
        resolver: "listAgents",
        resolverLabelField: "name",
        resolverValueField: "agent_id",
      },
      {
        key: "provider",
        label: "Provider",
        resolver: "listAuthProfiles",
        resolverLabelField: "provider",
        resolverValueField: "provider",
      },
    ],
  },

  /* ── Boards ── */
  {
    id: "listBoards",
    label: "List Boards",
    category: "Boards",
    description: "All boards with card/column counts.",
    responseShape: "object",
    sampleFields: ["items"],
  },
  {
    id: "getBoard",
    label: "Board Detail",
    category: "Boards",
    description: "Full board including columns and cards.",
    responseShape: "object",
    sampleFields: ["board_id", "name", "columns"],
    params: [
      {
        key: "boardId",
        label: "Board",
        resolver: "listBoards",
        resolverLabelField: "name",
        resolverValueField: "board_id",
      },
    ],
  },

  /* ── Jobs ── */
  {
    id: "listJobs",
    label: "List Jobs",
    category: "Jobs",
    description: "Scheduled jobs with next-run times and enabled state.",
    responseShape: "object",
    sampleFields: ["items"],
  },

  /* ── Focus ── */
  {
    id: "getMissionControlFocus",
    label: "Focus Queue",
    category: "Focus",
    description: "Operator attention items (approvals, failures, incidents).",
    responseShape: "object",
    sampleFields: ["items"],
  },
  {
    id: "getMissionControlCalendarWeek",
    label: "Calendar Week",
    category: "Focus",
    description: "7-day job schedule overview.",
    responseShape: "object",
    sampleFields: ["days"],
  },

  /* ── Approvals ── */
  {
    id: "listApprovals",
    label: "List Approvals",
    category: "Approvals",
    description: "Pending or resolved approval requests.",
    responseShape: "object",
    sampleFields: ["items"],
    params: [
      {
        key: "status",
        label: "Status",
        resolver: "_static:requested,approved,denied",
        resolverLabelField: "label",
        resolverValueField: "value",
      },
    ],
  },

  /* ── Extensions ── */
  {
    id: "listAuthProfiles",
    label: "Auth Profiles",
    category: "Extensions",
    description: "All authentication provider profiles.",
    responseShape: "array",
    sampleFields: ["auth_profile_id", "display_name", "provider", "auth_mode", "enabled"],
  },
  {
    id: "listSkills",
    label: "Skills",
    category: "Extensions",
    description: "Registered skills with enable/disable state.",
    responseShape: "object",
    sampleFields: ["contract_version", "items"],
  },
  {
    id: "listPlugins",
    label: "Plugins",
    category: "Extensions",
    description: "Installed plugins and runtime configuration.",
    responseShape: "object",
    sampleFields: ["contract_version", "plugin_api_version", "items"],
  },
  {
    id: "listPluginRuntimeStatus",
    label: "Plugin Runtime Status",
    category: "Extensions",
    description: "Plugin health, faulted state, and error codes.",
    responseShape: "object",
    sampleFields: ["contract_version", "items"],
  },

  /* ── Memory ── */
  {
    id: "listMemoryNotes",
    label: "Memory Notes",
    category: "Memory",
    description: "Agent memory notes (key-value pairs).",
    responseShape: "object",
    sampleFields: ["items"],
  },

  /* ── Mail ── */
  {
    id: "listAgentMailThreads",
    label: "Mail Threads",
    category: "Mail",
    description: "Agent mail threads (direct and room).",
    responseShape: "object",
    sampleFields: ["items"],
  },
  {
    id: "getAgentMailThread",
    label: "Mail Thread Detail",
    category: "Mail",
    description: "Single mail thread with metadata.",
    responseShape: "object",
    sampleFields: ["thread_id", "subject", "participants", "kind"],
    params: [
      {
        key: "threadId",
        label: "Thread",
        resolver: "listAgentMailThreads",
        resolverLabelField: "subject",
        resolverValueField: "thread_id",
      },
    ],
  },
  {
    id: "listAgentMailMessages",
    label: "Mail Messages",
    category: "Mail",
    description: "Messages in a specific mail thread.",
    responseShape: "object",
    sampleFields: ["items"],
    params: [
      {
        key: "threadId",
        label: "Thread",
        resolver: "listAgentMailThreads",
        resolverLabelField: "subject",
        resolverValueField: "thread_id",
      },
    ],
  },
  {
    id: "listAgentMailFileLeases",
    label: "File Leases",
    category: "Mail",
    description: "Active file leases across agent mail.",
    responseShape: "array",
    sampleFields: ["lease_id", "holder_principal", "glob_pattern", "expires_at"],
  },
];

export function getDataSourceById(id: string): CockpitDataSource | undefined {
  return COCKPIT_DATA_SOURCES.find((ds) => ds.id === id);
}

export function getDataSourcesByCategory(): Map<string, CockpitDataSource[]> {
  const map = new Map<string, CockpitDataSource[]>();
  for (const ds of COCKPIT_DATA_SOURCES) {
    const list = map.get(ds.category) ?? [];
    list.push(ds);
    map.set(ds.category, list);
  }
  return map;
}
