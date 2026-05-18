#!/usr/bin/env node

import http from "node:http";
import { randomUUID } from "node:crypto";
import { WebSocketServer } from "ws";

function parsePort(argv) {
  const idx = argv.findIndex((arg) => arg === "--port" || arg === "-p");
  if (idx >= 0 && idx + 1 < argv.length) {
    const parsed = Number.parseInt(argv[idx + 1], 10);
    if (Number.isFinite(parsed) && parsed > 0 && parsed < 65536) {
      return parsed;
    }
  }
  return 18_789;
}

function readJson(req) {
  return new Promise((resolve, reject) => {
    let raw = "";
    req.on("data", (chunk) => {
      raw += chunk;
    });
    req.on("end", () => {
      if (!raw) {
        resolve({});
        return;
      }
      try {
        resolve(JSON.parse(raw));
      } catch (error) {
        reject(error);
      }
    });
    req.on("error", reject);
  });
}

function setCors(headers) {
  headers["Access-Control-Allow-Origin"] = "*";
  headers["Access-Control-Allow-Methods"] = "GET,POST,OPTIONS";
  headers["Access-Control-Allow-Headers"] = "Authorization,Content-Type";
}

function sendJson(res, code, payload) {
  const headers = {
    "Content-Type": "application/json; charset=utf-8",
  };
  setCors(headers);
  res.writeHead(code, headers);
  res.end(JSON.stringify(payload));
}

function sendText(res, code, message) {
  const headers = {
    "Content-Type": "text/plain; charset=utf-8",
  };
  setCors(headers);
  res.writeHead(code, headers);
  res.end(message);
}

const port = parsePort(process.argv.slice(2));
const verbose = process.env.MC_E2E_VERBOSE === "1";
const startedAtMs = Date.now() - 90_000;
const createdAt = startedAtMs - 12_000;
const ANTHROPIC_SETUP_TOKEN_PREFIX = "sk-ant-oat01-";
const ANTHROPIC_SETUP_TOKEN_MIN_LENGTH = 80;
let nextAuthProfileCounter = 1;
let nextEventCounter = 1;
let nextCardCounter = 2;
let nextRunCounter = 1;
let nextJobRunCounter = 1;
const wsClients = new Set();
const wsTickets = new Map();

function looksLikeAnthropicSetupToken(value) {
  const trimmed = typeof value === "string" ? value.trim() : "";
  return (
    trimmed.startsWith(ANTHROPIC_SETUP_TOKEN_PREFIX) &&
    trimmed.length >= ANTHROPIC_SETUP_TOKEN_MIN_LENGTH
  );
}

const board = {
  board_id: "ops-board",
  board_key: "ops",
  name: "Operations Board",
  board_type: "kanban",
  created_at: createdAt,
  updated_at: createdAt,
  column_count: 3,
  card_count: 1,
};

const columns = [
  {
    column_id: "ops-backlog",
    board_id: board.board_id,
    column_key: "backlog",
    name: "Backlog",
    position: 0,
    created_at: createdAt,
    updated_at: createdAt,
  },
  {
    column_id: "ops-doing",
    board_id: board.board_id,
    column_key: "doing",
    name: "Doing",
    position: 1,
    created_at: createdAt,
    updated_at: createdAt,
  },
  {
    column_id: "ops-done",
    board_id: board.board_id,
    column_key: "done",
    name: "Done",
    position: 2,
    created_at: createdAt,
    updated_at: createdAt,
  },
];

const cards = [
  {
    card_id: "card-ops-1",
    board_id: board.board_id,
    column_id: columns[0].column_id,
    title: "Investigate gateway health",
    description: "Validate ws reconnect and service heartbeat.",
    owner_kind: "agent",
    owner_agent_id: "default",
    owner_human_id: null,
    due_at: null,
    tags: ["ops", "reliability"],
    script_markdown: null,
    linked_session_id: null,
    latest_run_id: null,
    position: 0,
    created_at: createdAt,
    updated_at: createdAt,
    assets: [],
  },
];

const jobs = [
  {
    job_id: "job-heartbeat",
    agent_id: "default",
    name: "Gateway heartbeat check",
    enabled: true,
    schedule_kind: "interval",
    interval_seconds: 300,
    run_at_ms: null,
    cron_expr: null,
    next_run_at: Date.now() + 120_000,
    payload_json: "{}",
    max_retries: 2,
    retry_backoff_ms: 1000,
    timeout_ms: 30_000,
    last_run_at: Date.now() - 60_000,
    last_error: null,
    created_at: createdAt,
    updated_at: createdAt,
  },
];

const agents = [
  {
    agent_id: "agent-root",
    name: "Root",
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "standard",
    role_label: "Operations Director",
    reports_to_agent_id: null,
    memory_binding: null,
  },
  {
    agent_id: "default",
    name: "Local Assistant",
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "default",
    role_label: "Reliability Lead",
    reports_to_agent_id: "agent-root",
    memory_binding: {
      binding_id: "mno-default",
      provider_kind: "modelnumquamoblita",
      base_url: "http://127.0.0.1:4411",
      auth_mode: "none",
      auth_secret_ref: null,
      principal_id: "local-operator",
      principal_display_name: "Local Assistant",
      enabled: true,
      trusted_local_operator_actions: true,
    },
  },
];

function createAgentMemoryLane(agentId, labelPrefix) {
  const checkedAt = new Date(Date.now() - 15_000).toISOString();
  const nodeA = {
    atom_id: `atm-${agentId}-01`,
    card_id: `mem-card-${agentId}-01`,
    kind: "preference_card",
    status: "active",
    summary: `${labelPrefix} prefers a narrow, operator-readable incident summary.`,
  };
  const nodeB = {
    atom_id: `atm-${agentId}-02`,
    card_id: `mem-card-${agentId}-02`,
    kind: "event_card",
    status: "active",
    summary: `${labelPrefix} resolved the gateway heartbeat drift during the last reliability pass.`,
  };
  const nodeC = {
    atom_id: `atm-${agentId}-03`,
    card_id: `mem-card-${agentId}-03`,
    kind: "continuity_card",
    status: "active",
    summary: `${labelPrefix} carries forward reliability context between operator sessions.`,
  };
  const graphLinks = [
    {
      source: nodeA.atom_id,
      target: nodeB.atom_id,
      kind: "supports",
    },
    {
      source: nodeB.atom_id,
      target: nodeC.atom_id,
      kind: "continuity",
    },
  ];
  const turnId = `turn-${agentId}-001`;
  const citationToken = `cite-${agentId}-001`;
  return {
    status: {
      agent_id: agentId,
      binding_status: "available",
      binding: cloneSeed(
        agents.find((agent) => agent.agent_id === agentId)?.memory_binding ?? null
      ),
      native_surface_availability: {
        cards: true,
        card_detail: true,
        atom_detail: true,
        graph_overview: true,
        graph_neighbors: true,
        episodes: true,
        turn_why: true,
        citation_lookup: true,
        runtime_health: true,
        telemetry_summary: true,
        telemetry_turns: true,
        decision_reasons: true,
      },
      orchestration: {
        enabled: true,
        transport: "http",
        health_status: "ok",
        degrade_mode: false,
        last_error_code: null,
        last_error: null,
      },
      native_runtime_status: {
        ok: true,
        status: "ok",
        checked_at: checkedAt,
        checks: [
          { name: "store", status: "ok" },
          { name: "review_queue", status: "ok" },
        ],
      },
      native_runtime_health_mismatch: false,
    },
    cardsPayload: {
      ok: true,
      total: 3,
      cards: [nodeA, nodeB, nodeC],
    },
    cardDetails: {
      [nodeA.card_id]: {
        ok: true,
        card: nodeA,
        atom: {
          atom_id: nodeA.atom_id,
          kind: nodeA.kind,
          status: nodeA.status,
          summary: nodeA.summary,
          tags: ["operator", "summary"],
        },
        provenance_events: [
          {
            kind: "turn_writeback",
            source_kind: "turn",
            source_id: turnId,
          },
        ],
        graph: {
          nodes: [nodeA, nodeB],
          links: [graphLinks[0]],
        },
      },
      [nodeB.card_id]: {
        ok: true,
        card: nodeB,
        atom: {
          atom_id: nodeB.atom_id,
          kind: nodeB.kind,
          status: nodeB.status,
          summary: nodeB.summary,
          tags: ["incident", "repair"],
        },
        provenance_events: [
          {
            kind: "episode_review",
            source_kind: "episode",
            source_id: `episode-${agentId}-001`,
          },
        ],
        graph: {
          nodes: [nodeA, nodeB, nodeC],
          links: graphLinks,
        },
      },
      [nodeC.card_id]: {
        ok: true,
        card: nodeC,
        atom: {
          atom_id: nodeC.atom_id,
          kind: nodeC.kind,
          status: nodeC.status,
          summary: nodeC.summary,
          tags: ["continuity"],
        },
        provenance_events: [],
        graph: {
          nodes: [nodeB, nodeC],
          links: [graphLinks[1]],
        },
      },
    },
    atomDetails: {
      [nodeA.atom_id]: {
        ok: true,
        atom: {
          atom_id: nodeA.atom_id,
          kind: nodeA.kind,
          status: nodeA.status,
          summary: nodeA.summary,
          label: "Operator preference",
        },
        card: nodeA,
        graph: {
          nodes: [nodeA, nodeB],
          links: [graphLinks[0]],
        },
      },
      [nodeB.atom_id]: {
        ok: true,
        atom: {
          atom_id: nodeB.atom_id,
          kind: nodeB.kind,
          status: nodeB.status,
          summary: nodeB.summary,
          label: "Incident event",
        },
        card: nodeB,
        graph: {
          nodes: [nodeA, nodeB, nodeC],
          links: graphLinks,
        },
      },
      [nodeC.atom_id]: {
        ok: true,
        atom: {
          atom_id: nodeC.atom_id,
          kind: nodeC.kind,
          status: nodeC.status,
          summary: nodeC.summary,
          label: "Continuity record",
        },
        card: nodeC,
        graph: {
          nodes: [nodeB, nodeC],
          links: [graphLinks[1]],
        },
      },
    },
    episodesPayload: {
      ok: true,
      total: 2,
      episodes: [
        {
          episode_id: `episode-${agentId}-001`,
          label: `${labelPrefix} heartbeat recovery`,
          status: "reviewed",
          run_id: "run-assistant-001",
          card_id: nodeB.card_id,
          updated_at_utc: checkedAt,
        },
        {
          episode_id: `episode-${agentId}-002`,
          label: `${labelPrefix} operator continuity refresh`,
          status: "active",
          run_id: "run-strategy-001",
          card_id: nodeC.card_id,
          updated_at_utc: checkedAt,
        },
      ],
    },
    graphMapPayload: {
      ok: true,
      total: 3,
      nodes: [nodeA, nodeB, nodeC],
      links: graphLinks,
      truncated: false,
      snapshot_available: true,
    },
    graphNeighborsByAtomId: {
      [nodeA.atom_id]: {
        ok: true,
        node: nodeA,
        neighbors: [
          {
            ...nodeB,
            distance: 1,
            via_edge_kind: "supports",
          },
        ],
        links: [graphLinks[0]],
        depth: 1,
        node_limit: 36,
        link_limit: 72,
        requests_used: 1,
        truncated: false,
        truncation: {
          node_limit_hit: false,
          link_limit_hit: false,
          request_budget_hit: false,
          dropped_shared_language: false,
        },
      },
      [nodeB.atom_id]: {
        ok: true,
        node: nodeB,
        neighbors: [
          {
            ...nodeA,
            distance: 1,
            via_edge_kind: "supports",
          },
          {
            ...nodeC,
            distance: 1,
            via_edge_kind: "continuity",
          },
        ],
        links: graphLinks,
        depth: 1,
        node_limit: 36,
        link_limit: 72,
        requests_used: 1,
        truncated: false,
        truncation: {
          node_limit_hit: false,
          link_limit_hit: false,
          request_budget_hit: false,
          dropped_shared_language: false,
        },
      },
      [nodeC.atom_id]: {
        ok: true,
        node: nodeC,
        neighbors: [
          {
            ...nodeB,
            distance: 1,
            via_edge_kind: "continuity",
          },
        ],
        links: [graphLinks[1]],
        depth: 1,
        node_limit: 36,
        link_limit: 72,
        requests_used: 1,
        truncated: false,
        truncation: {
          node_limit_hit: false,
          link_limit_hit: false,
          request_budget_hit: false,
          dropped_shared_language: false,
        },
      },
    },
    runtimeHealthPayload: {
      ok: true,
      status: "ok",
      checked_at: checkedAt,
      checks: [
        { name: "store", status: "ok" },
        { name: "continuity", status: "ok" },
      ],
    },
    telemetrySummaryPayload: {
      ok: true,
      limit: 12,
      summary: [
        { label: "context.build", route: "ltm_light", count: 4 },
        { label: "writeback.propose", route: "proposal_review", count: 2 },
      ],
    },
    telemetryTurnsPayload: {
      ok: true,
      limit: 12,
      turns: [
        {
          turn_id: turnId,
          route: "ltm_light",
          decision_reason: `${labelPrefix} memory route selected due to continuity evidence.`,
          latency_ms: 182,
          created_at_utc: checkedAt,
        },
      ],
      warn_turns: [],
    },
    decisionReasonsPayload: {
      ok: true,
      routes: [{ route: "ltm_light", count: 4 }],
      memory_preferences: [{ label: "operator readability", weight: 0.92 }],
      reasons: [
        {
          label: "Continuity preference retained",
          reason: `${labelPrefix} keeps reliability context lane-local and readable.`,
        },
      ],
    },
    whyByTurnId: {
      [turnId]: {
        ok: true,
        why: {
          decision: "ltm_light",
          decision_reason: `${labelPrefix} used continuity evidence from the reliability lane.`,
          evidence_time_window: "7d",
          top_evidence: [
            {
              source_id: nodeB.atom_id,
              excerpt: nodeB.summary,
              confidence: 0.92,
            },
          ],
          citations: [
            {
              citation_token: citationToken,
              label: "runtime card excerpt",
              source_id: nodeB.atom_id,
            },
          ],
          citations_hidden: false,
        },
      },
    },
    citationsByToken: {
      [citationToken]: {
        ok: true,
        citation: "runtime card excerpt",
        source_id: nodeB.atom_id,
        matches: [
          {
            line_number: 18,
            excerpt: nodeB.summary,
          },
        ],
      },
    },
  };
}

const agentMemoryLanes = {
  default: createAgentMemoryLane("default", "Local Assistant"),
};

const strategyGoals = [
  {
    goal_id: "goal-reliability",
    slug: "reliability-ops",
    title: "Reliability Operations",
    summary: "Keep core gateway workflows healthy and review throughput stable.",
    status: "active",
    owner_agent_id: "agent-root",
    target_date: Date.now() + 14 * 86_400_000,
    progress_pct: 68,
    created_at: createdAt,
    updated_at: Date.now() - 30_000,
  },
];

const strategyProjects = [
  {
    project_id: "project-gateway",
    goal_id: "goal-reliability",
    slug: "gateway-health",
    name: "Gateway Health",
    summary: "Track jobs, approvals, and board execution against the gateway health objective.",
    status: "active",
    owner_agent_id: "default",
    workspace_root: ".",
    budget_month_usd: 120,
    created_at: createdAt,
    updated_at: Date.now() - 20_000,
  },
];

const strategyTasks = [
  {
    task_id: "task-ops-1",
    project_id: "project-gateway",
    parent_task_id: null,
    title: "Investigate gateway health",
    detail: "Keep the board card, scheduled job, and approval queue aligned around gateway heartbeat health.",
    status: "in_progress",
    priority: "high",
    owner_agent_id: "default",
    due_at: Date.now() + 2 * 86_400_000,
    blocked_reason: null,
    linked_board_card_id: "card-ops-1",
    linked_job_id: "job-heartbeat",
    latest_run_id: "run-strategy-001",
    latest_session_id: "session-strategy-001",
    created_at: createdAt,
    updated_at: Date.now() - 10_000,
  },
  {
    task_id: "task-ops-2",
    project_id: "project-gateway",
    parent_task_id: null,
    title: "Close approval backlog",
    detail: "Resolve any lingering approval requests that block runtime execution.",
    status: "blocked",
    priority: "critical",
    owner_agent_id: "default",
    due_at: Date.now() + 86_400_000,
    blocked_reason: "Pending operator approval on the shell command request.",
    linked_board_card_id: null,
    linked_job_id: null,
    latest_run_id: null,
    latest_session_id: null,
    created_at: createdAt,
    updated_at: Date.now() - 5 * 86_400_000,
  },
];

const bootstrapPresets = [
  {
    schema_version: "bootstrap-preset-v1",
    preset_key: "reliability-lead",
    display_name: "Reliability Lead",
    description: "Default manager and workspace for reliability operators.",
    role_label: "Reliability Lead",
    provider_path: "local",
    default_model_provider: "ollama",
    default_model_id: "qwen3.5-9b-instruct",
    default_tool_profile: "standard",
    default_workspace_root: ".",
    default_reports_to_agent_id: "agent-root",
    setup_notes: "Used for e2e strategy coverage.",
    created_at: createdAt,
    updated_at: Date.now() - 10_000,
  },
];

const approvals = [
  {
    approval_id: "approval-001",
    run_id: "run-pending-001",
    kind: "tool_call",
    status: "requested",
    request_summary: "Allow shell command: ls -la",
    requested_at: Date.now() - 8_000,
    decided_at: null,
  },
  {
    approval_id: "approval-002",
    run_id: "run-pending-002",
    kind: "tool_call",
    status: "requested",
    request_summary: "Allow file edit: docs/release.md",
    requested_at: Date.now() - 7_000,
    decided_at: null,
  },
];

const channelStatuses = [
  {
    provider: "ollama",
    lifecycle_state: "running",
    healthy: true,
    session_state: "gateway_connected",
    proof_state: "roundtrip_confirmed",
    detail: "stub",
    proof_detail: "Mock runtime already proved a roundtrip.",
    last_error: null,
    last_inbound_at: Date.now() - 5_000,
    last_outbound_at: Date.now() - 4_000,
    last_proven_at: Date.now() - 4_000,
    reconnect_attempts: 0,
    updated_at: Date.now(),
  },
];
const channelConfig = {
  discord: {
    require_mention_in_guild_channels: true,
    allowlisted_user_ids: [],
    auto_run_enabled: true,
    default_agent_id: "default",
    default_model_provider: "ollama",
    default_model_id: "qwen3.5-9b-instruct",
  },
  telegram: {
    require_mention_in_groups: true,
    allowlisted_user_ids: [],
    dm_policy: "pairing",
    group_policy: "allowlist",
    group_allowlisted_user_ids: [],
    allowlisted_chat_ids: [],
    auto_leave_unauthorized_groups: true,
    pairing_code_ttl_seconds: 3600,
    pairing_max_pending: 3,
    unauthorized_spam_threshold: 4,
    unauthorized_spam_block_seconds: 3600,
    auto_run_enabled: true,
    default_agent_id: "default",
    default_model_provider: "ollama",
    default_model_id: "qwen3.5-9b-instruct",
  },
  updated_at: Date.now(),
};
const telegramPairingStatus = {
  dm_policy: "pairing",
  group_policy: "allowlist",
  auto_leave_unauthorized_groups: true,
  pending_requests: [],
  blocked_senders: [],
  updated_at: Date.now(),
};
const runtimeConfig = {
  schema_version: "runtime.config.v1",
  global: {
    jwt_issuer_allowlist: [],
    jwt_audience_allowlist: [],
    trusted_proxy_allowlist: [],
    tls_termination_mode: "edge",
    public_base_url: null,
    assistant_system_prompt: null,
  },
  providers: [],
  channels: {
    discord: {
      enabled: false,
      bot_token_secret_ref: null,
      operation_mode: "transport",
      api_base_url: null,
      transport_timeout_ms: null,
      transport_retry_attempts: null,
      application_id: null,
      intents: [],
      staging_guild_ids: [],
      staging_channel_ids: [],
    },
    telegram: {
      enabled: false,
      bot_token_secret_ref: null,
      operation_mode: "transport",
      api_base_url: null,
      transport_timeout_ms: null,
      transport_retry_attempts: null,
      long_poll_timeout_seconds: null,
      webhook_mode: "long_poll",
      webhook_url: null,
      staging_chat_ids: [],
    },
  },
  routing: {
    enabled: false,
    use_channel_defaults_as_fallback: false,
    local_operator_human_identity_id: "local-operator",
    dm_unmapped_policy: "approval_required",
    shared_unmapped_policy: "block",
    human_identities: [
      {
        human_identity_id: "local-operator",
        display_name: "You",
        enabled: true,
      },
    ],
    platform_identity_links: [],
    assistant_assignments: [],
    lane_memory_policies: [],
  },
  memory: {
    blend_mode: "local_augment",
    memory_md_sources: [],
    numquam: {
      enabled: false,
      integration_base_url: null,
      managed_runtime_enabled: false,
      managed_repo_root: null,
      managed_lanes_root: null,
      managed_python_bin: null,
      managed_runtime_port_base: 4410,
      managed_mcp_port_base: 4510,
      managed_launch_timeout_ms: 120000,
      transport: "http",
      context_build_timeout_ms: 15000,
      writeback_propose_timeout_ms: 15000,
      writeback_resolve_timeout_ms: 15000,
      handshake_timeout_ms: 5000,
      handshake_interval_ms: 60000,
      token_secret_ref: null,
      principal_id: null,
      principal_display_name: null,
    },
  },
  extensions: {
    plugin_daemon_allowlist: [],
    plugin_bundle_root: null,
    assistant_tools: {
      limits: {
        max_spawn_depth: 4,
        max_children_per_root_run: 24,
        max_active_workers_total: 32,
      },
      templates: [],
    },
  },
  security: {
    audit_hot_retention_days: 90,
    audit_archive_retention_days: 365,
  },
  autonomy_guardrails: {
    max_run_ms: 300000,
    max_tool_calls_per_run: 32,
    max_provider_input_chars: 120000,
    max_tool_output_chars_total: 120000,
    max_provider_attempts: 3,
    max_consecutive_failures_before_breaker: 3,
    heartbeat_max_run_ms: 10000,
  },
  updated_at: Date.now(),
};
const runtimeSecrets = {};

const authProfiles = [];
const connectorCatalog = [
  {
    catalog_item_id: "catalog-github-openapi",
    slug: "github-rest",
    display_name: "GitHub REST",
    source_kind: "openapi",
    summary: "Curated REST scaffold for GitHub-style OpenAPI imports.",
    publisher: "carsinOS",
    trust_class: "trusted_curated",
    available_versions: ["v1"],
    marketplace_origin: "built-in",
    importable: true,
    future_marketplace_metadata: {},
  },
  {
    catalog_item_id: "catalog-linear-graphql",
    slug: "linear-graphql",
    display_name: "Linear GraphQL",
    source_kind: "graphql",
    summary: "GraphQL connector scaffold for operator-safe issue orchestration.",
    publisher: "carsinOS",
    trust_class: "trusted_curated",
    available_versions: ["v1"],
    marketplace_origin: "built-in",
    importable: true,
    future_marketplace_metadata: {},
  },
  {
    catalog_item_id: "catalog-slack-mcp",
    slug: "slack-mcp",
    display_name: "Slack MCP",
    source_kind: "mcp",
    summary: "MCP tool scaffold for Slack-compatible tool servers.",
    publisher: "carsinOS",
    trust_class: "trusted_curated",
    available_versions: ["v1"],
    marketplace_origin: "built-in",
    importable: true,
    future_marketplace_metadata: {},
  },
];
const connectorSources = [];
const connectorVersions = [];
const connectorConversions = [];
const connectorPublishedTools = [];
const connectorAssignments = [];
const connectorAuthBindings = [];
const connectorInteractions = [];
const providerOrder = new Map();
let usageMode = "available";
let usageUpdatedAtMs = Date.now();

function cloneSeed(value) {
  return JSON.parse(JSON.stringify(value));
}

function replaceArray(target, seed) {
  target.splice(0, target.length, ...cloneSeed(seed));
}

const seedState = {
  board: cloneSeed(board),
  columns: cloneSeed(columns),
  cards: cloneSeed(cards),
  jobs: cloneSeed(jobs),
  agents: cloneSeed(agents),
  agentMemoryLanes: cloneSeed(agentMemoryLanes),
  strategyGoals: cloneSeed(strategyGoals),
  strategyProjects: cloneSeed(strategyProjects),
  strategyTasks: cloneSeed(strategyTasks),
  bootstrapPresets: cloneSeed(bootstrapPresets),
  approvals: cloneSeed(approvals),
  channelStatuses: cloneSeed(channelStatuses),
  channelConfig: cloneSeed(channelConfig),
  telegramPairingStatus: cloneSeed(telegramPairingStatus),
  runtimeConfig: cloneSeed(runtimeConfig),
  runtimeSecrets: cloneSeed(runtimeSecrets),
  authProfiles: cloneSeed(authProfiles),
  connectorSources: cloneSeed(connectorSources),
  connectorVersions: cloneSeed(connectorVersions),
  connectorConversions: cloneSeed(connectorConversions),
  connectorPublishedTools: cloneSeed(connectorPublishedTools),
  connectorAssignments: cloneSeed(connectorAssignments),
  connectorAuthBindings: cloneSeed(connectorAuthBindings),
  connectorInteractions: cloneSeed(connectorInteractions),
};

function resetMockState() {
  Object.assign(board, cloneSeed(seedState.board));
  replaceArray(columns, seedState.columns);
  replaceArray(cards, seedState.cards);
  replaceArray(jobs, seedState.jobs);
  replaceArray(agents, seedState.agents);
  Object.keys(agentMemoryLanes).forEach((key) => {
    delete agentMemoryLanes[key];
  });
  Object.assign(agentMemoryLanes, cloneSeed(seedState.agentMemoryLanes));
  replaceArray(strategyGoals, seedState.strategyGoals);
  replaceArray(strategyProjects, seedState.strategyProjects);
  replaceArray(strategyTasks, seedState.strategyTasks);
  replaceArray(bootstrapPresets, seedState.bootstrapPresets);
  replaceArray(approvals, seedState.approvals);
  replaceArray(channelStatuses, seedState.channelStatuses);
  Object.assign(channelConfig, cloneSeed(seedState.channelConfig));
  Object.assign(telegramPairingStatus, cloneSeed(seedState.telegramPairingStatus));
  Object.assign(runtimeConfig, cloneSeed(seedState.runtimeConfig));
  Object.keys(runtimeSecrets).forEach((key) => {
    delete runtimeSecrets[key];
  });
  Object.assign(runtimeSecrets, cloneSeed(seedState.runtimeSecrets));
  replaceArray(authProfiles, seedState.authProfiles);
  replaceArray(connectorSources, seedState.connectorSources);
  replaceArray(connectorVersions, seedState.connectorVersions);
  replaceArray(connectorConversions, seedState.connectorConversions);
  replaceArray(connectorPublishedTools, seedState.connectorPublishedTools);
  replaceArray(connectorAssignments, seedState.connectorAssignments);
  replaceArray(connectorAuthBindings, seedState.connectorAuthBindings);
  replaceArray(connectorInteractions, seedState.connectorInteractions);
  providerOrder.clear();
  nextAuthProfileCounter = 1;
  nextEventCounter = 1;
  nextCardCounter = 2;
  nextRunCounter = 1;
  nextJobRunCounter = 1;
  wsTickets.clear();
  usageMode = "available";
  usageUpdatedAtMs = Date.now();
}

function runtimeSecretKeyFromScope(scope) {
  return `runtime.${String(scope ?? "").trim().replaceAll("/", ".")}`;
}

function runtimeSecretRefFromKey(key) {
  return `secret://${key}`;
}

function runtimeSecretKeyFromRef(ref) {
  const trimmed = String(ref ?? "").trim();
  return trimmed.startsWith("secret://") ? trimmed.slice("secret://".length) : trimmed;
}

function upsertMockChannelStatus(provider, partial) {
  let target = channelStatuses.find((item) => item.provider === provider);
  if (!target) {
    target = {
      provider,
      lifecycle_state: "stopped",
      healthy: false,
      session_state: "offline",
      proof_state: "unproven",
      detail: null,
      proof_detail: null,
      last_error: null,
      last_inbound_at: null,
      last_outbound_at: null,
      last_proven_at: null,
      reconnect_attempts: 0,
      updated_at: Date.now(),
    };
    channelStatuses.push(target);
  }
  Object.assign(target, partial, { updated_at: Date.now() });
  return target;
}

function reconcileMockChannelStatus(provider) {
  const config = runtimeConfig.channels?.[provider];
  if (!config) {
    return null;
  }
  if (!config.enabled) {
    return upsertMockChannelStatus(provider, {
      lifecycle_state: "stopped",
      healthy: false,
      session_state: "offline",
      proof_state: "unproven",
      detail: "disabled in runtime config",
      proof_detail: null,
      last_error: null,
      last_inbound_at: null,
      last_outbound_at: null,
      last_proven_at: null,
    });
  }
  const secretRef = String(config.bot_token_secret_ref ?? "").trim();
  if (!secretRef) {
    return upsertMockChannelStatus(provider, {
      lifecycle_state: "waiting_for_auth",
      healthy: false,
      session_state: "offline",
      proof_state: "unproven",
      detail: "missing bot token",
      proof_detail: null,
      last_error: "missing bot token",
      last_inbound_at: null,
      last_outbound_at: null,
      last_proven_at: null,
    });
  }
  const secretValue = runtimeSecrets[runtimeSecretKeyFromRef(secretRef)];
  if (!secretValue) {
    return upsertMockChannelStatus(provider, {
      lifecycle_state: "waiting_for_auth",
      healthy: false,
      session_state: "offline",
      proof_state: "unproven",
      detail: "missing stored secret",
      proof_detail: null,
      last_error: "missing stored secret",
      last_inbound_at: null,
      last_outbound_at: null,
      last_proven_at: null,
    });
  }
  if (String(secretValue).toLowerCase().includes("bad")) {
    return upsertMockChannelStatus(provider, {
      lifecycle_state: "error",
      healthy: false,
      session_state: "offline",
      proof_state: "unproven",
      detail: "mock rejected the supplied bot token",
      proof_detail: null,
      last_error: "invalid bot token",
      last_inbound_at: null,
      last_outbound_at: null,
      last_proven_at: null,
    });
  }
  return upsertMockChannelStatus(provider, {
    lifecycle_state: "running",
    healthy: true,
    session_state: provider === "discord" ? "gateway_connected" : "listening",
    proof_state: "unproven",
    detail: "connected via mock runtime",
    proof_detail: "Mock runtime is live and waiting for the first proven roundtrip.",
    last_error: null,
    last_inbound_at: null,
    last_outbound_at: null,
    last_proven_at: null,
  });
}

const CONNECTOR_READ_METHODS = new Set(["get", "head"]);

function normalizeConnectorSlug(value) {
  return String(value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

function normalizeOperationSlug(value) {
  return String(value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function parseConnectorSourceDocument(payload) {
  if (payload.source_json && typeof payload.source_json === "object") {
    return cloneSeed(payload.source_json);
  }
  const sourceText = String(payload.source_text ?? "").trim();
  if (!sourceText) {
    return {};
  }
  return JSON.parse(sourceText);
}

function connectorById(connectorId) {
  return connectorSources.find((item) => item.connector_id === connectorId) ?? null;
}

function connectorVersionById(versionId) {
  return connectorVersions.find((item) => item.version_id === versionId) ?? null;
}

function connectorConversionById(conversionId) {
  return connectorConversions.find((item) => item.conversion_id === conversionId) ?? null;
}

function connectorPublishedToolById(publishedToolId) {
  return (
    connectorPublishedTools.find((item) => item.published_tool_id === publishedToolId) ?? null
  );
}

function listConnectorVersions(connectorId) {
  return connectorVersions
    .filter((item) => item.connector_id === connectorId)
    .sort((left, right) => right.created_at - left.created_at);
}

function listConnectorConversions(connectorId) {
  return connectorConversions
    .filter((item) => item.connector_id === connectorId)
    .sort((left, right) => right.created_at - left.created_at);
}

function listConnectorPublishedTools(connectorId) {
  return connectorPublishedTools
    .filter((item) => item.connector_id === connectorId)
    .sort((left, right) => right.published_at - left.published_at);
}

function listConnectorAssignments(connectorId) {
  return connectorAssignments
    .filter((item) => item.connector_id === connectorId)
    .sort((left, right) => left.agent_id.localeCompare(right.agent_id));
}

function listConnectorAuthBindings(connectorId) {
  return connectorAuthBindings
    .filter((item) => item.connector_id === connectorId)
    .sort((left, right) => right.updated_at - left.updated_at);
}

function listConnectorInteractions(connectorId = null) {
  return connectorInteractions
    .filter((item) => !connectorId || item.connector_id === connectorId)
    .sort((left, right) => right.updated_at - left.updated_at);
}

function recomputeConnectorStats(connectorId) {
  const connector = connectorById(connectorId);
  if (!connector) {
    return null;
  }
  connector.published_tool_count = listConnectorPublishedTools(connectorId).filter(
    (item) => item.unpublished_at == null
  ).length;
  connector.assigned_agent_count = listConnectorAssignments(connectorId).filter(
    (item) => item.enabled
  ).length;
  connector.updated_at = Date.now();
  return connector;
}

function buildConnectorHealth(connectorId) {
  const connector = connectorById(connectorId);
  if (!connector) {
    return null;
  }
  const bindings = listConnectorAuthBindings(connectorId).filter((item) =>
    item.status === "ready" || item.status === "configured"
  );
  const authRequired =
    connector.status === "enabled" &&
    connector.published_tool_count > 0 &&
    bindings.length === 0;
  return {
    connector_id: connector.connector_id,
    status: connector.status,
    degraded_reason: authRequired ? "Connector is enabled but missing a ready auth binding." : null,
    auth_required: authRequired,
    last_checked_at: Date.now(),
    published_tool_count: connector.published_tool_count,
    assigned_agent_count: connector.assigned_agent_count,
  };
}

function upsertPendingAuthInteraction(connector, agentId = null) {
  const existing = connectorInteractions.find(
    (item) =>
      item.connector_id === connector.connector_id &&
      item.status === "pending" &&
      item.interaction_kind === "auth_repair" &&
      item.agent_id === agentId
  );
  if (existing) {
    existing.updated_at = Date.now();
    return existing;
  }
  const created = {
    interaction_id: randomUUID(),
    connector_id: connector.connector_id,
    agent_id: agentId,
    interaction_kind: "auth_repair",
    status: "pending",
    prompt_summary: `Reconnect auth for ${connector.display_name}`,
    resume_token: `resume-${connector.connector_id}`,
    expires_at: Date.now() + 30 * 60_000,
    consumed_at: null,
    detail: {
      source_kind: connector.source_kind,
    },
    created_at: Date.now(),
    updated_at: Date.now(),
  };
  connectorInteractions.push(created);
  return created;
}

function createConnectorConversion(connector, version) {
  const sourceDocument =
    version.import_metadata?.source_json && typeof version.import_metadata.source_json === "object"
      ? version.import_metadata.source_json
      : {};
  const proposedTools = [];
  if (connector.source_kind === "openapi") {
    const paths = sourceDocument.paths && typeof sourceDocument.paths === "object"
      ? sourceDocument.paths
      : {};
    Object.entries(paths).forEach(([path, methods]) => {
      if (!methods || typeof methods !== "object") {
        return;
      }
      Object.entries(methods).forEach(([method, operation]) => {
        const normalizedMethod = method.toLowerCase();
        if (!["get", "post", "put", "patch", "delete", "head"].includes(normalizedMethod)) {
          return;
        }
        const operationId = String(
          operation?.operationId ?? `${normalizedMethod}-${path}`
        );
        const operationSlug = normalizeOperationSlug(operationId || `${normalizedMethod}-${path}`);
        proposedTools.push({
          candidate_id: `cand-${connector.connector_id}-${operationSlug}`,
          operation_key: operationId,
          proposed_tool_name: `connector.${connector.slug}.${operationSlug}`,
          display_name: operation?.summary ?? operationId,
          description: operation?.description ?? `${normalizedMethod.toUpperCase()} ${path}`,
          input_schema: {
            type: "object",
            properties: {},
          },
          write_classification: CONNECTOR_READ_METHODS.has(normalizedMethod)
            ? "read_only"
            : "operator_write_gated",
          review_blocked: false,
          review_block_reason: null,
        });
      });
    });
  } else if (connector.source_kind === "graphql") {
    const operations = Array.isArray(sourceDocument.operations)
      ? sourceDocument.operations
      : [];
    operations.forEach((operation) => {
      const operationName = String(operation?.name ?? operation?.operation_key ?? "graphql-operation");
      const operationSlug = normalizeOperationSlug(operationName);
      proposedTools.push({
        candidate_id: `cand-${connector.connector_id}-${operationSlug}`,
        operation_key: operationName,
        proposed_tool_name: `connector.${connector.slug}.${operationSlug}`,
        display_name: operation?.display_name ?? operationName,
        description: operation?.description ?? "GraphQL operation",
        input_schema: operation?.input_schema ?? {
          type: "object",
          properties: {},
        },
        write_classification:
          String(operation?.kind ?? "query").toLowerCase() === "query"
            ? "read_only"
            : "operator_write_gated",
        review_blocked: false,
        review_block_reason: null,
      });
    });
  } else if (connector.source_kind === "mcp") {
    const tools = Array.isArray(sourceDocument.tools) ? sourceDocument.tools : [];
    tools.forEach((tool) => {
      const toolName = String(tool?.name ?? "mcp-tool");
      const operationSlug = normalizeOperationSlug(toolName);
      proposedTools.push({
        candidate_id: `cand-${connector.connector_id}-${operationSlug}`,
        operation_key: toolName,
        proposed_tool_name: `connector.${connector.slug}.${operationSlug}`,
        display_name: tool?.display_name ?? toolName,
        description: tool?.description ?? "MCP tool",
        input_schema: tool?.input_schema ?? {
          type: "object",
          properties: {},
        },
        write_classification:
          tool?.write_classification === "read_only"
            ? "read_only"
            : "operator_write_gated",
        review_blocked: false,
        review_block_reason: null,
      });
    });
  }
  const conversion = {
    conversion_id: randomUUID(),
    connector_id: connector.connector_id,
    version_id: version.version_id,
    status: "succeeded",
    warnings:
      proposedTools.length === 0
        ? [
            {
              code: "NO_OPERATIONS",
              message: "No reviewable operations were found in the connector source.",
              blocking: true,
            },
          ]
        : [],
    proposed_tools: proposedTools,
    write_capable_tools: proposedTools.filter(
      (item) => item.write_classification !== "read_only"
    ).length,
    unsupported_operations: [],
    normalization_notes: [
      `source_kind=${connector.source_kind}`,
      `connector_slug=${connector.slug}`,
    ],
    diff_from_previous: {
      previous_conversion_id:
        connectorConversions.find((item) => item.connector_id === connector.connector_id)
          ?.conversion_id ?? null,
      proposed_tool_count: proposedTools.length,
    },
    created_at: Date.now(),
    updated_at: Date.now(),
  };
  connectorConversions.push(conversion);
  version.latest_conversion_id = conversion.conversion_id;
  version.updated_at = Date.now();
  connector.last_conversion_at = Date.now();
  connector.status = proposedTools.length > 0 ? "converted" : "draft";
  connector.updated_at = Date.now();
  return conversion;
}

function findBoard(boardId) {
  return boardId === board.board_id ? board : null;
}

function sortByPosition(items) {
  return [...items].sort((left, right) => left.position - right.position);
}

function normalizeColumnPositions(boardId, columnId) {
  const target = sortByPosition(
    cards.filter((card) => card.board_id === boardId && card.column_id === columnId)
  );
  for (let i = 0; i < target.length; i += 1) {
    target[i].position = i;
    target[i].updated_at = Date.now();
  }
}

function recalcBoardSummary() {
  board.column_count = columns.filter((column) => column.board_id === board.board_id).length;
  board.card_count = cards.filter((card) => card.board_id === board.board_id).length;
  board.updated_at = Date.now();
}

function getApprovals(status) {
  if (!status) {
    return approvals;
  }
  return approvals.filter((item) => item.status === status);
}

function getMissionControlFocusItems() {
  const now = Date.now();
  const pendingApprovals = getApprovals("requested");
  const approvalItems = pendingApprovals.map((approval, index) => ({
    item_id: `focus-approval-${approval.approval_id}`,
    category: "approval",
    severity: "high",
    title: `Approval requested: ${approval.request_summary}`,
    detail: `Approval ${approval.approval_id} requires operator decision.`,
    primary_action: "review",
    action_payload: {
      approval_id: approval.approval_id,
      request_summary: approval.request_summary,
      run_id: approval.run_id,
      provider: "ollama",
    },
    created_at: approval.requested_at - index,
  }));

  const degradedChannels = channelStatuses.filter(
    (item) => !item.healthy || item.lifecycle_state !== "running"
  );
  const channelItems = degradedChannels.map((channel) => ({
    item_id: `focus-channel-${channel.provider}`,
    category: "channel_health",
    severity: "warning",
    title: `${channel.provider} degraded`,
    detail: channel.last_error ?? channel.detail ?? "Channel adapter is degraded.",
    primary_action: "reconnect",
    action_payload: {
      provider: channel.provider,
    },
    created_at: now - 2_000,
  }));

  if (approvalItems.length === 0 && channelItems.length === 0) {
    return [
      {
        item_id: "focus-001",
        category: "status",
        severity: "normal",
        title: "All systems nominal",
        detail: "Deterministic stub focus event",
        primary_action: "none",
        action_payload: {},
        created_at: now - 5_000,
      },
    ];
  }

  return [...approvalItems, ...channelItems];
}

function toStrategyTaskListItem(taskId) {
  const task = strategyTasks.find((item) => item.task_id === taskId);
  if (!task) {
    return null;
  }
  const project = strategyProjects.find((item) => item.project_id === task.project_id);
  const goal = project
    ? strategyGoals.find((item) => item.goal_id === project.goal_id) ?? null
    : null;
  const owner = task.owner_agent_id
    ? agents.find((item) => item.agent_id === task.owner_agent_id) ?? null
    : null;
  return {
    task_id: task.task_id,
    title: task.title,
    status: task.status,
    priority: task.priority,
    owner_agent_id: task.owner_agent_id,
    owner_name: owner?.name ?? null,
    project_id: project?.project_id ?? "unknown-project",
    project_name: project?.name ?? "Unknown project",
    goal_id: goal?.goal_id ?? "unknown-goal",
    goal_title: goal?.title ?? "Unknown goal",
    updated_at: task.updated_at,
    due_at: task.due_at,
    blocked_reason: task.blocked_reason,
  };
}

function getStrategySummaryPayload() {
  const blockedTasks = strategyTasks
    .filter((task) => task.status === "blocked")
    .map((task) => toStrategyTaskListItem(task.task_id))
    .filter(Boolean);
  const staleTasks = strategyTasks
    .filter((task) => Date.now() - task.updated_at > 48 * 3_600_000)
    .map((task) => toStrategyTaskListItem(task.task_id))
    .filter(Boolean);

  return {
    generated_at_ms: Date.now(),
    currency: "USD",
    blocked_task_count: blockedTasks.length,
    blocked_tasks: blockedTasks,
    stale_task_count: staleTasks.length,
    stale_tasks: staleTasks,
    spend_by_agent: [
      {
        agent_id: "default",
        agent_name: "Local Assistant",
        estimated_cost_total: 18.4,
        linked_task_count: 2,
      },
    ],
    spend_by_project: [
      {
        project_id: "project-gateway",
        project_name: "Gateway Health",
        goal_id: "goal-reliability",
        goal_title: "Reliability Operations",
        estimated_cost_total: 18.4,
        attributed_run_count: 3,
      },
    ],
    unattributed_spend_total: 2.6,
    goal_progress: [
      {
        goal_id: "goal-reliability",
        title: "Reliability Operations",
        progress_pct: 68,
        open_task_count: 2,
        blocked_task_count: 1,
      },
    ],
    critical_approval_backlog_count: 1,
    critical_approval_backlog: [
      {
        approval_id: "approval-001",
        kind: "tool_call",
        summary: "Allow shell command: ls -la",
        linked_task_id: "task-ops-1",
        requested_at: Date.now() - 8_000,
      },
    ],
  };
}

function getAgentLabel(agentId) {
  if (!agentId) {
    return null;
  }
  return agents.find((item) => item.agent_id === agentId)?.name ?? agentId;
}

function createRunbookDeepLinkTarget(tab, targetKind, targetId = null, context = null) {
  return {
    tab,
    target_kind: targetKind,
    target_id: targetId,
    context,
  };
}

function createRunbookEntityRef(entityKind, entityId, displayLabel, deepLink) {
  return {
    entity_kind: entityKind,
    entity_id: entityId,
    display_label: displayLabel,
    deep_link: deepLink,
  };
}

function getRunbookRecords() {
  const now = Date.now();
  const heartbeatCard = cards[0];
  const heartbeatJob = jobs[0];
  const activeTask = strategyTasks.find((item) => item.task_id === "task-ops-1") ?? strategyTasks[0];
  const blockedTask = strategyTasks.find((item) => item.task_id === "task-ops-2") ?? strategyTasks[0];
  const gatewayProject = strategyProjects.find((item) => item.project_id === "project-gateway")
    ?? strategyProjects[0];
  const reliabilityGoal = strategyGoals.find((item) => item.goal_id === "goal-reliability")
    ?? strategyGoals[0];
  const assistantSessionRef = createRunbookEntityRef(
    "session",
    "session-assistant-001",
    "Incident recovery session",
    createRunbookDeepLinkTarget("assistant", "session", "session-assistant-001", "runbook")
  );
  const assistantApprovalRef = createRunbookEntityRef(
    "approval",
    approvals[0]?.approval_id ?? "approval-runbook-001",
    approvals[0]?.request_summary ?? "Await operator approval",
    createRunbookDeepLinkTarget("focus", "approval", approvals[0]?.approval_id ?? null, "runbook")
  );
  const heartbeatCardRef = createRunbookEntityRef(
    "board_card",
    heartbeatCard.card_id,
    heartbeatCard.title,
    createRunbookDeepLinkTarget("boards", "board_card", heartbeatCard.card_id, "runbook")
  );
  const heartbeatJobRef = createRunbookEntityRef(
    "job",
    heartbeatJob.job_id,
    heartbeatJob.name,
    createRunbookDeepLinkTarget("calendar", "job", heartbeatJob.job_id, "runbook")
  );
  const activeTaskRef = createRunbookEntityRef(
    "task",
    activeTask.task_id,
    activeTask.title,
    createRunbookDeepLinkTarget("strategy", "task", activeTask.task_id, "runbook")
  );
  const blockedTaskRef = createRunbookEntityRef(
    "task",
    blockedTask.task_id,
    blockedTask.title,
    createRunbookDeepLinkTarget("strategy", "task", blockedTask.task_id, "runbook")
  );
  const gatewayProjectRef = createRunbookEntityRef(
    "project",
    gatewayProject.project_id,
    gatewayProject.name,
    createRunbookDeepLinkTarget("strategy", "project", gatewayProject.project_id, "runbook")
  );
  const reliabilityGoalRef = createRunbookEntityRef(
    "goal",
    reliabilityGoal.goal_id,
    reliabilityGoal.title,
    createRunbookDeepLinkTarget("strategy", "goal", reliabilityGoal.goal_id, "runbook")
  );

  return [
    {
      updated_at_ms: now - 5_000,
      primary_entity_label: "Incident recovery session",
      detail: {
        runbook_id: "assistant_session_run:run-assistant-001",
        runbook_kind: "assistant_session_run",
        template_id: "assistant-session-run",
        template_version: "mc-runbook-v1",
        anchor_kind: "run",
        anchor_id: "run-assistant-001",
        title: "Approval gate for incident recovery session",
        status: "waiting",
        status_reason: "Shell command approval is still pending.",
        generated_at_ms: now,
        selected_execution_ref: {
          entity_kind: "run",
          entity_id: "run-assistant-001",
          created_at_ms: now - 18_000,
          started_at_ms: now - 17_000,
          waiting_since_ms: now - 8_000,
          finished_at_ms: null,
        },
        active_step_id: "approval_wait",
        next_step_ids: ["run_executing"],
        linked_entities: [assistantSessionRef, assistantApprovalRef],
        steps: [
          {
            step_id: "session_intake",
            label: "Receive operator request",
            kind: "intake",
            state: "completed",
            state_reason: null,
            started_at_ms: now - 18_000,
            finished_at_ms: now - 17_000,
            waiting_since_ms: null,
            linked_entity_refs: [assistantSessionRef],
            action_refs: ["open_assistant_session"],
            template_index: 0,
          },
          {
            step_id: "approval_wait",
            label: "Await approval",
            kind: "approval",
            state: "waiting",
            state_reason: "Operator approval is required before execution can continue.",
            started_at_ms: now - 17_000,
            finished_at_ms: null,
            waiting_since_ms: now - 8_000,
            linked_entity_refs: [assistantApprovalRef],
            action_refs: ["open_approval", "open_assistant_session"],
            template_index: 1,
          },
          {
            step_id: "run_executing",
            label: "Execute run",
            kind: "execution",
            state: "pending",
            state_reason: null,
            started_at_ms: null,
            finished_at_ms: null,
            waiting_since_ms: null,
            linked_entity_refs: [assistantSessionRef],
            action_refs: ["open_assistant_session"],
            template_index: 2,
          },
        ],
        history: [
          {
            history_id: "assistant-history-001",
            event_kind: "session.created",
            label: "Session created",
            detail: "Operator created a recovery session.",
            occurred_at_ms: now - 18_000,
            step_id: "session_intake",
            entity_refs: [assistantSessionRef],
          },
          {
            history_id: "assistant-history-002",
            event_kind: "approval.requested",
            label: "Approval requested",
            detail: "Shell command execution requires operator approval.",
            occurred_at_ms: now - 8_000,
            step_id: "approval_wait",
            entity_refs: [assistantApprovalRef],
          },
        ],
        actions: [
          {
            action_id: "open_assistant_session",
            action_kind: "open_session",
            label: "Open session",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: assistantSessionRef,
          },
          {
            action_id: "open_approval",
            action_kind: "open_approval",
            label: "Review approval",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: assistantApprovalRef,
          },
        ],
        source_facts: [
          {
            fact_id: "assistant-fact-run",
            fact_kind: "run_record",
            entity_ref: assistantSessionRef,
            occurred_at_ms: now - 18_000,
            partial: false,
          },
          {
            fact_id: "assistant-fact-approval",
            fact_kind: "approval_record",
            entity_ref: assistantApprovalRef,
            occurred_at_ms: now - 8_000,
            partial: false,
          },
        ],
        availability: {
          is_limited: false,
          is_stale: false,
          last_refresh_at_ms: now - 2_000,
          missing_source_kinds: [],
          stale_reason: null,
        },
        warnings: [
          {
            warning_id: "assistant-warning-approval-pending",
            warning_kind: "approval_pending",
            message: "Operator approval is still pending for the next command.",
          },
        ],
        owner_agent_id: "agent-root",
        owner_agent_label: getAgentLabel("agent-root"),
      },
    },
    {
      updated_at_ms: heartbeatCard.updated_at,
      primary_entity_label: heartbeatCard.title,
      detail: {
        runbook_id: `board_card_run:${heartbeatCard.card_id}`,
        runbook_kind: "board_card_run",
        template_id: "board-card-run",
        template_version: "mc-runbook-v1",
        anchor_kind: "card",
        anchor_id: heartbeatCard.card_id,
        title: heartbeatCard.title,
        status: "completed",
        status_reason: "Gateway heartbeat validation was captured on the board card.",
        generated_at_ms: now,
        selected_execution_ref: {
          entity_kind: "board_card",
          entity_id: heartbeatCard.card_id,
          created_at_ms: heartbeatCard.created_at,
          started_at_ms: heartbeatCard.created_at,
          waiting_since_ms: null,
          finished_at_ms: heartbeatCard.updated_at,
        },
        active_step_id: "card_run_completed",
        next_step_ids: [],
        linked_entities: [heartbeatCardRef, activeTaskRef],
        steps: [
          {
            step_id: "card_created",
            label: "Card created",
            kind: "card_state",
            state: "completed",
            state_reason: null,
            started_at_ms: heartbeatCard.created_at,
            finished_at_ms: heartbeatCard.created_at,
            waiting_since_ms: null,
            linked_entity_refs: [heartbeatCardRef],
            action_refs: ["open_board_card"],
            template_index: 0,
          },
          {
            step_id: "card_investigation",
            label: "Investigate card",
            kind: "card_state",
            state: "completed",
            state_reason: null,
            started_at_ms: heartbeatCard.created_at + 10_000,
            finished_at_ms: heartbeatCard.updated_at - 15_000,
            waiting_since_ms: null,
            linked_entity_refs: [heartbeatCardRef, activeTaskRef],
            action_refs: ["open_board_card", "open_linked_task"],
            template_index: 1,
          },
          {
            step_id: "card_run_completed",
            label: "Complete card execution",
            kind: "card_state",
            state: "completed",
            state_reason: null,
            started_at_ms: heartbeatCard.updated_at - 15_000,
            finished_at_ms: heartbeatCard.updated_at,
            waiting_since_ms: null,
            linked_entity_refs: [heartbeatCardRef],
            action_refs: ["open_board_card"],
            template_index: 2,
          },
        ],
        history: [
          {
            history_id: "board-history-001",
            event_kind: "board.card.created",
            label: "Board card created",
            detail: heartbeatCard.description,
            occurred_at_ms: heartbeatCard.created_at,
            step_id: "card_created",
            entity_refs: [heartbeatCardRef],
          },
          {
            history_id: "board-history-002",
            event_kind: "board.card.completed",
            label: "Board card completed",
            detail: "The investigation outcome was recorded on the board.",
            occurred_at_ms: heartbeatCard.updated_at,
            step_id: "card_run_completed",
            entity_refs: [heartbeatCardRef],
          },
        ],
        actions: [
          {
            action_id: "open_board_card",
            action_kind: "open_board_card",
            label: "Open board card",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: heartbeatCardRef,
          },
          {
            action_id: "open_linked_task",
            action_kind: "open_task",
            label: "Open linked task",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: activeTaskRef,
          },
        ],
        source_facts: [
          {
            fact_id: "board-fact-card",
            fact_kind: "board_card_record",
            entity_ref: heartbeatCardRef,
            occurred_at_ms: heartbeatCard.updated_at,
            partial: false,
          },
          {
            fact_id: "board-fact-task",
            fact_kind: "task_link",
            entity_ref: activeTaskRef,
            occurred_at_ms: activeTask.updated_at,
            partial: false,
          },
        ],
        availability: {
          is_limited: false,
          is_stale: true,
          last_refresh_at_ms: now - 180_000,
          missing_source_kinds: [],
          stale_reason: "Board card timeline is older than the live event window.",
        },
        warnings: [],
        owner_agent_id: heartbeatCard.owner_agent_id,
        owner_agent_label: getAgentLabel(heartbeatCard.owner_agent_id),
      },
    },
    {
      updated_at_ms: now - 12_000,
      primary_entity_label: heartbeatJob.name,
      detail: {
        runbook_id: `scheduled_job_run:${heartbeatJob.job_id}`,
        runbook_kind: "scheduled_job_run",
        template_id: "scheduled-job-run",
        template_version: "mc-runbook-v1",
        anchor_kind: "job",
        anchor_id: heartbeatJob.job_id,
        title: heartbeatJob.name,
        status: "active",
        status_reason: "Heartbeat execution is currently processing.",
        generated_at_ms: now,
        selected_execution_ref: {
          entity_kind: "job_run",
          entity_id: "job-run-heartbeat-001",
          created_at_ms: now - 30_000,
          started_at_ms: now - 28_000,
          waiting_since_ms: null,
          finished_at_ms: null,
        },
        active_step_id: "job_processing",
        next_step_ids: ["approval_wait", "job_run_succeeded", "job_run_failed"],
        linked_entities: [heartbeatJobRef, activeTaskRef],
        steps: [
          {
            step_id: "job_scheduled",
            label: "Schedule job",
            kind: "job_state",
            state: "completed",
            state_reason: null,
            started_at_ms: heartbeatJob.created_at,
            finished_at_ms: heartbeatJob.created_at,
            waiting_since_ms: null,
            linked_entity_refs: [heartbeatJobRef],
            action_refs: ["open_job"],
            template_index: 0,
          },
          {
            step_id: "job_processing",
            label: "Process job",
            kind: "job_state",
            state: "active",
            state_reason: "The current execution is still running.",
            started_at_ms: now - 28_000,
            finished_at_ms: null,
            waiting_since_ms: null,
            linked_entity_refs: [heartbeatJobRef, activeTaskRef],
            action_refs: ["open_job", "open_linked_task"],
            template_index: 1,
          },
          {
            step_id: "job_run_succeeded",
            label: "Mark success",
            kind: "job_state",
            state: "pending",
            state_reason: null,
            started_at_ms: null,
            finished_at_ms: null,
            waiting_since_ms: null,
            linked_entity_refs: [heartbeatJobRef],
            action_refs: ["open_job"],
            template_index: 2,
          },
          {
            step_id: "job_run_failed",
            label: "Handle failure",
            kind: "job_state",
            state: "pending",
            state_reason: null,
            started_at_ms: null,
            finished_at_ms: null,
            waiting_since_ms: null,
            linked_entity_refs: [heartbeatJobRef],
            action_refs: ["open_job"],
            template_index: 3,
          },
        ],
        history: [
          {
            history_id: "job-history-001",
            event_kind: "job.created",
            label: "Job created",
            detail: "Recurring heartbeat job was registered.",
            occurred_at_ms: heartbeatJob.created_at,
            step_id: "job_scheduled",
            entity_refs: [heartbeatJobRef],
          },
          {
            history_id: "job-history-002",
            event_kind: "job.run.started",
            label: "Job run started",
            detail: "Manual execution triggered from Mission Control.",
            occurred_at_ms: now - 28_000,
            step_id: "job_processing",
            entity_refs: [heartbeatJobRef],
          },
        ],
        actions: [
          {
            action_id: "open_job",
            action_kind: "open_job",
            label: "Open job",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: heartbeatJobRef,
          },
          {
            action_id: "open_linked_task",
            action_kind: "open_task",
            label: "Open linked task",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: activeTaskRef,
          },
        ],
        source_facts: [
          {
            fact_id: "job-fact-job",
            fact_kind: "job_record",
            entity_ref: heartbeatJobRef,
            occurred_at_ms: heartbeatJob.updated_at,
            partial: false,
          },
          {
            fact_id: "job-fact-task",
            fact_kind: "task_link",
            entity_ref: activeTaskRef,
            occurred_at_ms: activeTask.updated_at,
            partial: false,
          },
        ],
        availability: {
          is_limited: false,
          is_stale: false,
          last_refresh_at_ms: now - 1_000,
          missing_source_kinds: [],
          stale_reason: null,
        },
        warnings: [],
        owner_agent_id: heartbeatJob.agent_id,
        owner_agent_label: getAgentLabel(heartbeatJob.agent_id),
      },
    },
    {
      updated_at_ms: blockedTask.updated_at,
      primary_entity_label: blockedTask.title,
      detail: {
        runbook_id: `strategy_task_execution:${blockedTask.task_id}`,
        runbook_kind: "strategy_task_execution",
        template_id: "strategy-task-execution",
        template_version: "mc-runbook-v1",
        anchor_kind: "task",
        anchor_id: blockedTask.task_id,
        title: blockedTask.title,
        status: "blocked",
        status_reason: blockedTask.blocked_reason,
        generated_at_ms: now,
        selected_execution_ref: null,
        active_step_id: "blocked",
        next_step_ids: ["resume_execution"],
        linked_entities: [blockedTaskRef, gatewayProjectRef, reliabilityGoalRef],
        steps: [
          {
            step_id: "task_created",
            label: "Create task",
            kind: "task_state",
            state: "completed",
            state_reason: null,
            started_at_ms: blockedTask.created_at,
            finished_at_ms: blockedTask.created_at,
            waiting_since_ms: null,
            linked_entity_refs: [blockedTaskRef],
            action_refs: ["open_task"],
            template_index: 0,
          },
          {
            step_id: "blocked",
            label: "Blocked",
            kind: "task_state",
            state: "blocked",
            state_reason: blockedTask.blocked_reason,
            started_at_ms: blockedTask.updated_at - 60_000,
            finished_at_ms: null,
            waiting_since_ms: blockedTask.updated_at - 60_000,
            linked_entity_refs: [blockedTaskRef, gatewayProjectRef, reliabilityGoalRef],
            action_refs: ["open_task", "open_project", "open_goal"],
            template_index: 1,
          },
          {
            step_id: "resume_execution",
            label: "Resume execution",
            kind: "task_state",
            state: "pending",
            state_reason: null,
            started_at_ms: null,
            finished_at_ms: null,
            waiting_since_ms: null,
            linked_entity_refs: [blockedTaskRef],
            action_refs: ["open_task"],
            template_index: 2,
          },
        ],
        history: [
          {
            history_id: "task-history-001",
            event_kind: "task.created",
            label: "Task created",
            detail: blockedTask.detail,
            occurred_at_ms: blockedTask.created_at,
            step_id: "task_created",
            entity_refs: [blockedTaskRef],
          },
          {
            history_id: "task-history-002",
            event_kind: "task.blocked",
            label: "Task blocked",
            detail: blockedTask.blocked_reason,
            occurred_at_ms: blockedTask.updated_at,
            step_id: "blocked",
            entity_refs: [blockedTaskRef, gatewayProjectRef, reliabilityGoalRef],
          },
        ],
        actions: [
          {
            action_id: "open_task",
            action_kind: "open_task",
            label: "Open task",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: blockedTaskRef,
          },
          {
            action_id: "open_project",
            action_kind: "open_project",
            label: "Open project",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: gatewayProjectRef,
          },
          {
            action_id: "open_goal",
            action_kind: "open_goal",
            label: "Open goal",
            availability: "enabled",
            disabled_reason: null,
            target_entity_ref: reliabilityGoalRef,
          },
        ],
        source_facts: [
          {
            fact_id: "task-fact-task",
            fact_kind: "task_record",
            entity_ref: blockedTaskRef,
            occurred_at_ms: blockedTask.updated_at,
            partial: false,
          },
          {
            fact_id: "task-fact-project",
            fact_kind: "project_record",
            entity_ref: gatewayProjectRef,
            occurred_at_ms: gatewayProject.updated_at,
            partial: false,
          },
          {
            fact_id: "task-fact-goal",
            fact_kind: "goal_record",
            entity_ref: reliabilityGoalRef,
            occurred_at_ms: reliabilityGoal.updated_at,
            partial: false,
          },
        ],
        availability: {
          is_limited: false,
          is_stale: false,
          last_refresh_at_ms: now - 3_000,
          missing_source_kinds: [],
          stale_reason: null,
        },
        warnings: [
          {
            warning_id: "task-warning-blocked",
            warning_kind: "upstream_blocker",
            message: blockedTask.blocked_reason ?? "Task is waiting on an upstream blocker.",
          },
        ],
        owner_agent_id: blockedTask.owner_agent_id,
        owner_agent_label: getAgentLabel(blockedTask.owner_agent_id),
      },
    },
  ];
}

function compareRunbookRecords(left, right) {
  return (right.updated_at_ms - left.updated_at_ms)
    || left.detail.title.localeCompare(right.detail.title)
    || left.detail.runbook_id.localeCompare(right.detail.runbook_id);
}

function buildRunbookSummary(record) {
  const activeStep = record.detail.active_step_id
    ? record.detail.steps.find((item) => item.step_id === record.detail.active_step_id) ?? null
    : null;
  return {
    runbook_id: record.detail.runbook_id,
    runbook_kind: record.detail.runbook_kind,
    anchor_kind: record.detail.anchor_kind,
    anchor_id: record.detail.anchor_id,
    title: record.detail.title,
    status: record.detail.status,
    status_reason: record.detail.status_reason,
    owner_agent_id: record.detail.owner_agent_id,
    owner_agent_label: record.detail.owner_agent_label,
    primary_entity_label: record.primary_entity_label,
    updated_at_ms: record.updated_at_ms,
    current_step_label: activeStep?.label ?? null,
    warning_count: record.detail.warnings.length,
    linked_entities: record.detail.linked_entities,
    availability: record.detail.availability,
  };
}

function buildRunbookStatusCounts(records) {
  const counts = {
    pending: 0,
    active: 0,
    waiting: 0,
    blocked: 0,
    failed: 0,
    completed: 0,
    limited: 0,
    _unexpected: 0,
  };
  for (const record of records) {
    if (Object.hasOwn(counts, record.detail.status)) {
      counts[record.detail.status] += 1;
    } else {
      counts._unexpected += 1;
      console.warn("unexpected runbook status in mock gateway", record.detail.status, record);
    }
  }
  return counts;
}

function runbookHasEntity(detail, entityKind, entityId) {
  if (detail.anchor_kind === entityKind && detail.anchor_id === entityId) {
    return true;
  }
  return detail.linked_entities.some(
    (entity) => entity.entity_kind === entityKind && entity.entity_id === entityId
  );
}

function runbookMatchesSearch(record, query) {
  if (!query) {
    return true;
  }
  const haystack = [
    record.detail.title,
    record.primary_entity_label,
    record.detail.status_reason ?? "",
    record.detail.owner_agent_label ?? "",
    ...record.detail.linked_entities.map((entity) => entity.display_label),
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(query);
}

function buildRunbookListPayload(requestUrl) {
  const kind = requestUrl.searchParams.get("kind")?.trim() || null;
  const status = requestUrl.searchParams.get("status")?.trim() || null;
  const ownerAgentId = requestUrl.searchParams.get("owner_agent_id")?.trim() || null;
  const query = requestUrl.searchParams.get("query")?.trim().toLowerCase() || "";
  const linkedTaskId = requestUrl.searchParams.get("linked_task_id")?.trim() || null;
  const linkedProjectId = requestUrl.searchParams.get("linked_project_id")?.trim() || null;
  const linkedGoalId = requestUrl.searchParams.get("linked_goal_id")?.trim() || null;
  const limitRaw = Number(requestUrl.searchParams.get("limit") ?? "50");
  const limit = Number.isFinite(limitRaw) && limitRaw > 0
    ? Math.min(200, Math.trunc(limitRaw))
    : 50;
  const cursor = requestUrl.searchParams.get("cursor")?.trim() || null;
  const records = getRunbookRecords()
    .sort(compareRunbookRecords)
    .filter((record) => !kind || record.detail.runbook_kind === kind)
    .filter((record) => !status || record.detail.status === status)
    .filter((record) => !ownerAgentId || record.detail.owner_agent_id === ownerAgentId)
    .filter((record) => runbookMatchesSearch(record, query))
    .filter((record) => !linkedTaskId || runbookHasEntity(record.detail, "task", linkedTaskId))
    .filter((record) => !linkedProjectId || runbookHasEntity(record.detail, "project", linkedProjectId))
    .filter((record) => !linkedGoalId || runbookHasEntity(record.detail, "goal", linkedGoalId));
  const countsByStatus = buildRunbookStatusCounts(records);

  let startIndex = 0;
  if (cursor) {
    const cursorIndex = records.findIndex((record) => record.detail.runbook_id === cursor);
    if (cursorIndex < 0) {
      return { error: "invalid_cursor" };
    }
    startIndex = cursorIndex + 1;
  }

  const page = records.slice(startIndex, startIndex + limit);
  return {
    generated_at_ms: Date.now(),
    items: page.map(buildRunbookSummary),
    counts_by_status: countsByStatus,
    next_cursor:
      startIndex + page.length < records.length
        ? page[page.length - 1]?.detail.runbook_id ?? null
        : null,
  };
}

function getRunbookDetailPayload(runbookKind, anchorId) {
  return getRunbookRecords().find(
    (record) => record.detail.runbook_kind === runbookKind && record.detail.anchor_id === anchorId
  )?.detail ?? null;
}

function emptyMemorySurfaceAvailability() {
  return {
    cards: false,
    card_detail: false,
    atom_detail: false,
    graph_overview: false,
    graph_neighbors: false,
    episodes: false,
    turn_why: false,
    citation_lookup: false,
    runtime_health: false,
    telemetry_summary: false,
    telemetry_turns: false,
    decision_reasons: false,
  };
}

function emptyMemoryOrchestration() {
  return {
    enabled: false,
    transport: "http",
    health_status: "down",
    degrade_mode: false,
    last_error_code: null,
    last_error: null,
  };
}

function getAgentMemoryLane(agentId) {
  return agentMemoryLanes[agentId] ?? null;
}

function getAgentMemoryStatusPayload(agent) {
  const lane = getAgentMemoryLane(agent.agent_id);
  if (!agent.memory_binding || !agent.memory_binding.enabled || !lane) {
    return {
      agent_id: agent.agent_id,
      binding_status: "unconfigured",
      binding: agent.memory_binding ?? null,
      native_surface_availability: emptyMemorySurfaceAvailability(),
      orchestration: emptyMemoryOrchestration(),
      native_runtime_status: null,
      native_runtime_health_mismatch: false,
    };
  }
  return cloneSeed(lane.status);
}

function sendAgentMemoryJson(res, agentId, data) {
  const lane = getAgentMemoryLane(agentId);
  const bindingId =
    lane?.status?.binding?.binding_id ??
    agents.find((agent) => agent.agent_id === agentId)?.memory_binding?.binding_id ??
    `memory-${agentId}`;
  sendJson(res, 200, {
    agent_id: agentId,
    binding_id: bindingId,
    data,
  });
}

function closeAllWsConnections(code = 1012, reason = "e2e-ws-flap") {
  for (const client of wsClients) {
    if (client.readyState === 1) {
      client.close(code, reason);
    }
  }
}

function sendMalformedWsPayload(raw = "{malformed") {
  for (const client of wsClients) {
    if (client.readyState === 1) {
      client.send(raw);
    }
  }
}

function requireAuth(req, res) {
  const auth = req.headers.authorization;
  if (!auth || !auth.startsWith("Bearer ") || auth.slice(7).trim().length === 0) {
    sendJson(res, 401, {
      error: "missing or invalid bearer token",
    });
    return false;
  }
  return true;
}

function getSchedulerLock() {
  return {
    enabled: true,
    lock_path: "/tmp/carsinos-scheduler.lock",
    owner: "stub-gateway",
    detail: null,
  };
}

function getStatusPayload() {
  return {
    service: "carsinos-gateway-stub",
    version: "stub-1",
    started_at_utc: new Date(startedAtMs).toISOString(),
    uptime_ms: Date.now() - startedAtMs,
    db_path: "/tmp/carsinos-stub.db",
    attachments_path: "/tmp/carsinos-attachments",
    trust_contract_lock: {
      enforced: true,
      lock_path: "/tmp/carsinos-trust.lock",
      trust_hash: "stub-trust-hash",
      locked_at: startedAtMs,
      drift_detected: false,
    },
    scheduler_lock: getSchedulerLock(),
    open_circuit_breakers: 0,
    circuit_breakers: [],
    open_plugin_breakers: 0,
    plugin_breakers: [],
    top_stop_reasons: [],
  };
}

function getJobsStatusPayload() {
  const jobsEnabled = jobs.filter((job) => job.enabled).length;
  return {
    scheduler_running: true,
    scheduler_lock: getSchedulerLock(),
    jobs_total: jobs.length,
    jobs_enabled: jobsEnabled,
    jobs_due: 0,
    open_circuit_breakers: 0,
    circuit_breakers: [],
    top_stop_reasons: [],
    now_utc: new Date().toISOString(),
  };
}

function buildCalendarWeek() {
  const now = Date.now();
  const weekStart = now - (now % (7 * 24 * 60 * 60 * 1000));
  const weekEnd = weekStart + 7 * 24 * 60 * 60 * 1000;
  return {
    week_start_ms: weekStart,
    week_end_ms: weekEnd,
    generated_at_ms: now,
    always_running: [],
    next_up: [
      {
        job_id: jobs[0].job_id,
        name: jobs[0].name,
        agent_id: jobs[0].agent_id,
        enabled: jobs[0].enabled,
        schedule_kind: jobs[0].schedule_kind,
        interval_seconds: jobs[0].interval_seconds,
        cron_expr: jobs[0].cron_expr,
        next_run_at: jobs[0].next_run_at,
        last_run_at: jobs[0].last_run_at,
        last_error: jobs[0].last_error,
        lane: "scheduled",
        primary_action: "pause",
      },
    ],
    jobs: [
      {
        job_id: jobs[0].job_id,
        name: jobs[0].name,
        agent_id: jobs[0].agent_id,
        enabled: jobs[0].enabled,
        schedule_kind: jobs[0].schedule_kind,
        interval_seconds: jobs[0].interval_seconds,
        cron_expr: jobs[0].cron_expr,
        next_run_at: jobs[0].next_run_at,
        last_run_at: jobs[0].last_run_at,
        last_error: jobs[0].last_error,
        lane: "scheduled",
        primary_action: "pause",
      },
    ],
  };
}

function usageDayStartMs(nowMs, tzOffsetMinutes) {
  const offsetMs = tzOffsetMinutes * 60_000;
  const localNow = new Date(nowMs + offsetMs);
  localNow.setUTCHours(0, 0, 0, 0);
  return localNow.getTime() - offsetMs;
}

function usageWeekStartMs(nowMs, tzOffsetMinutes) {
  const offsetMs = tzOffsetMinutes * 60_000;
  const localNow = new Date(nowMs + offsetMs);
  localNow.setUTCHours(0, 0, 0, 0);
  const mondayOffset = (localNow.getUTCDay() + 6) % 7;
  localNow.setUTCDate(localNow.getUTCDate() - mondayOffset);
  return localNow.getTime() - offsetMs;
}

function buildUsageBuckets(window, windowStartMs, windowEndMs) {
  const bucketCount = window === "today" ? 24 : 7;
  const bucketSpanMs = Math.max(1, Math.floor((windowEndMs - windowStartMs) / bucketCount));
  const buckets = [];
  for (let i = 0; i < bucketCount; i += 1) {
    const start = windowStartMs + i * bucketSpanMs;
    const end = i === bucketCount - 1 ? windowEndMs : start + bucketSpanMs;
    const loadFactor = window === "today" ? (i === bucketCount - 1 ? 0.28 : i === bucketCount - 2 ? 0.18 : 0.03) : (i >= bucketCount - 2 ? 0.24 : 0.08);
    buckets.push({
      bucket_start_utc: new Date(start).toISOString(),
      bucket_end_utc: new Date(end).toISOString(),
      estimated_cost_total: Number((loadFactor * 2.6).toFixed(4)),
      token_input_total: Math.round(loadFactor * 12_000),
      token_output_total: Math.round(loadFactor * 5_200),
    });
  }
  return buckets;
}

function buildMissionControlUsage(requestUrl) {
  const now = Date.now();
  const window = requestUrl.searchParams.get("window") === "today" ? "today" : "week";
  const timezone = requestUrl.searchParams.get("timezone") || "UTC";
  const tzOffsetRaw = Number(requestUrl.searchParams.get("tz_offset_minutes") ?? "0");
  const tzOffsetMinutes = Number.isFinite(tzOffsetRaw) ? Math.trunc(tzOffsetRaw) : 0;
  const explicitStartRaw = requestUrl.searchParams.get("window_start_ms");
  const explicitEndRaw = requestUrl.searchParams.get("window_end_ms");
  const explicitStart = explicitStartRaw === null ? Number.NaN : Number(explicitStartRaw);
  const explicitEnd = explicitEndRaw === null ? Number.NaN : Number(explicitEndRaw);
  const hasExplicitWindow = Number.isFinite(explicitStart)
    && Number.isFinite(explicitEnd)
    && explicitEnd > explicitStart;

  const windowStartMs = hasExplicitWindow
    ? Math.trunc(explicitStart)
    : window === "today"
      ? usageDayStartMs(now, tzOffsetMinutes)
      : usageWeekStartMs(now, tzOffsetMinutes);
  const windowEndMs = hasExplicitWindow
    ? Math.trunc(explicitEnd)
    : window === "today"
      ? windowStartMs + 24 * 60 * 60_000
      : windowStartMs + 7 * 24 * 60 * 60_000;

  if (usageMode === "unavailable") {
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
      reason_code: "USAGE_UNAVAILABLE",
      detail: "Stub usage mode set to unavailable.",
    };
  }

  const estimatedCostTotal = window === "today" ? 1.284 : 6.742;
  const tokenInputTotal = window === "today" ? 18_200 : 82_900;
  const tokenOutputTotal = window === "today" ? 7_420 : 31_000;
  const byAgent = [
    {
      agent_id: "default",
      agent_name: "Local Assistant",
      estimated_cost_total: Number((estimatedCostTotal * 0.62).toFixed(4)),
      token_input_total: Math.round(tokenInputTotal * 0.6),
      token_output_total: Math.round(tokenOutputTotal * 0.58),
    },
    {
      agent_id: "default",
      agent_name: "Default Agent",
      estimated_cost_total: Number((estimatedCostTotal * 0.38).toFixed(4)),
      token_input_total: Math.round(tokenInputTotal * 0.4),
      token_output_total: Math.round(tokenOutputTotal * 0.42),
    },
  ];
  const byModel = [
    {
      model_provider: "ollama",
      model_id: "qwen3.5-9b-instruct",
      estimated_cost_total: Number((estimatedCostTotal * 0.74).toFixed(4)),
      token_input_total: Math.round(tokenInputTotal * 0.73),
      token_output_total: Math.round(tokenOutputTotal * 0.7),
    },
    {
      model_provider: "ollama",
      model_id: "qwen3.5-4b-instruct",
      estimated_cost_total: Number((estimatedCostTotal * 0.26).toFixed(4)),
      token_input_total: Math.round(tokenInputTotal * 0.27),
      token_output_total: Math.round(tokenOutputTotal * 0.3),
    },
  ];
  const byProvider = [
    {
      provider: "ollama",
      estimated_cost_total: estimatedCostTotal,
      token_input_total: tokenInputTotal,
      token_output_total: tokenOutputTotal,
    },
  ];

  const payload = {
    contract_version: "mc-usage-v1",
    available: true,
    window,
    timezone,
    currency: "USD",
    window_start_utc: new Date(windowStartMs).toISOString(),
    window_end_utc: new Date(windowEndMs).toISOString(),
    estimated_cost_total: estimatedCostTotal,
    token_input_total: tokenInputTotal,
    token_output_total: tokenOutputTotal,
    by_agent: byAgent,
    by_model: byModel,
    by_provider: byProvider,
    by_time: buildUsageBuckets(window, windowStartMs, windowEndMs),
    by_job: [],
    by_card: [],
    budget_thresholds: [
      {
        provider: "ollama",
        daily_token_budget: 100_000,
        daily_cost_usd_budget: 1.4,
        token_usage_total: tokenInputTotal + tokenOutputTotal,
        cost_usage_total: estimatedCostTotal,
        token_ratio: (tokenInputTotal + tokenOutputTotal) / 100_000,
        cost_ratio: estimatedCostTotal / 1.4,
      },
    ],
    updated_at_utc: new Date(usageUpdatedAtMs).toISOString(),
  };

  if (usageMode === "missing-optional") {
    payload.by_job = null;
    payload.by_card = null;
  }
  if (usageMode === "invalid-required") {
    delete payload.by_model;
  }
  return payload;
}

function buildWsEvent(overrides = {}) {
  const payload = overrides.payload && typeof overrides.payload === "object" ? overrides.payload : {};
  const mergedPayload = {
    domain: "system",
    severity: "normal",
    summary: "Stub gateway connected",
    ...payload,
  };
  return {
    schema_version: "v1",
    event_id: `evt-${String(nextEventCounter++).padStart(4, "0")}`,
    event_type: typeof overrides.event_type === "string" ? overrides.event_type : "gateway.notice",
    ts_unix_ms: typeof overrides.ts_unix_ms === "number" ? overrides.ts_unix_ms : Date.now(),
    request_id: null,
    entity: typeof overrides.entity === "string" ? overrides.entity : "system",
    payload: mergedPayload,
  };
}

function broadcastWsEvent(overrides = {}) {
  const event = buildWsEvent(overrides);
  const message = JSON.stringify(event);
  for (const client of wsClients) {
    if (client.readyState === 1) {
      client.send(message);
    }
  }
  return event;
}

function assistantDeskWorkItem(overrides) {
  const now = Date.now();
  return {
    id: "run:desk-default",
    kind: "run",
    bucket: "working",
    status: "working",
    title: "Tracking assistant handoff",
    owner_label: "ExecAss",
    task_label: "Assistant work",
    current_action: "Collecting updates for the operator.",
    last_event_at: new Date(now - 20_000).toISOString(),
    last_event_at_ms: now - 20_000,
    source_refs: [{ source: "run", id: "desk-default", label: "Assistant work" }],
    deep_links: [],
    details: {
      provider_label: "LM Studio",
      model_label: "qwen3.5-9b-instruct",
      workspace_label: "carsinos",
      source_health: "fresh",
      last_error: null,
    },
    transcript_id: "transcript:run:desk-default",
    can_open_transcript: true,
    transcript_unavailable_reason: null,
    artifact_count: 0,
    changed_file_count: 0,
    ...overrides,
  };
}

function buildAssistantDeskPayload() {
  const now = Date.now();
  const needsYou = [
    assistantDeskWorkItem({
      id: "run:approval-001",
      kind: "approval",
      bucket: "needs_you",
      status: "needs_you",
      title: "Review shell approval",
      task_label: "Operator approval",
      current_action: "Waiting for you to approve or deny a shell command.",
      last_event_at: new Date(now - 8_000).toISOString(),
      last_event_at_ms: now - 8_000,
      source_refs: [
        { source: "run", id: "run-pending-001", label: "Assistant work" },
        { source: "approval", id: "approval-001", label: "Approval" },
      ],
      transcript_id: "transcript:run:approval-001",
    }),
  ];
  const working = [
    assistantDeskWorkItem({
      id: "run:worker-001",
      title: "Checking local model route",
      current_action: "Confirming the selected assistant can answer through the local provider.",
      last_event_at: new Date(now - 24_000).toISOString(),
      last_event_at_ms: now - 24_000,
      transcript_id: "transcript:run:worker-001",
    }),
    assistantDeskWorkItem({
      id: "run:worker-002",
      title: "Reading current workspace state",
      current_action: "Looking at open tasks and recent messages.",
      last_event_at: new Date(now - 55_000).toISOString(),
      last_event_at_ms: now - 55_000,
      transcript_id: "transcript:run:worker-002",
      changed_file_count: 2,
    }),
    assistantDeskWorkItem({
      id: "run:worker-003",
      title: "Preparing a short status note",
      current_action: "Summarizing what changed and what still needs attention.",
      last_event_at: new Date(now - 92_000).toISOString(),
      last_event_at_ms: now - 92_000,
      transcript_id: "transcript:run:worker-003",
      artifact_count: 1,
    }),
    assistantDeskWorkItem({
      id: "run:worker-004",
      title: "Watching channel health",
      current_action: "Keeping an eye on connected message sources.",
      last_event_at: new Date(now - 118_000).toISOString(),
      last_event_at_ms: now - 118_000,
      transcript_id: "transcript:run:worker-004",
    }),
  ];
  const doneRecently = [
    assistantDeskWorkItem({
      id: "run:done-001",
      bucket: "done_recently",
      status: "done",
      title: "Saved assistant notes",
      current_action: "Finished and ready to review.",
      last_event_at: new Date(now - 180_000).toISOString(),
      last_event_at_ms: now - 180_000,
      transcript_id: "transcript:run:done-001",
      artifact_count: 1,
    }),
  ];
  return {
    generated_at: new Date(now).toISOString(),
    stale: false,
    buckets: {
      needs_you: needsYou,
      working,
      done_recently: doneRecently,
    },
    summary: {
      needs_you_count: needsYou.length,
      working_count: working.length,
      done_recently_count: doneRecently.length,
      stale_count: 0,
    },
  };
}

function buildAssistantDeskTranscript(workItemId) {
  const now = Date.now();
  return {
    work_item_id: workItemId,
    transcript_id: `transcript:${workItemId}`,
    title: workItemId.includes("approval") ? "Review shell approval" : "Assistant work",
    complete: true,
    next_cursor: null,
    events: [
      {
        id: `${workItemId}:event:1`,
        at: new Date(now - 90_000).toISOString(),
        role: "system",
        source: "assistant",
        title: "Started",
        text: "ExecAss started tracking this item.",
        body_markdown: "ExecAss started tracking **this item** and attached it to the Desk.",
        artifact_refs: [],
      },
      {
        id: `${workItemId}:event:2`,
        at: new Date(now - 45_000).toISOString(),
        role: "assistant",
        source: "assistant",
        title: "Current note",
        text: "The next action is ready for review.",
        body_markdown: "- Current action is visible.\n- Transcript text renders as markdown.",
        artifact_refs: [],
      },
    ],
    artifacts: [],
  };
}

async function routeRequest(req, res) {
  const requestUrl = new URL(req.url ?? "/", `http://${req.headers.host ?? "127.0.0.1"}`);
  if (verbose) {
    console.log(`[mock-gateway] ${req.method} ${requestUrl.pathname}${requestUrl.search}`);
  }

  if (req.method === "OPTIONS") {
    const headers = {};
    setCors(headers);
    res.writeHead(204, headers);
    res.end();
    return;
  }

  if (!requestUrl.pathname.startsWith("/api/v1/")) {
    sendText(res, 404, "not found");
    return;
  }

  if (!requireAuth(req, res)) {
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/e2e/reset") {
    resetMockState();
    sendJson(res, 200, {
      ok: true,
      reset_at_ms: Date.now(),
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/health") {
    sendJson(res, 200, {
      status: "ok",
      service: "carsinos-gateway-stub",
      ok: true,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/ws-ticket") {
    const ticket = `mock-ws-ticket-${randomUUID()}`;
    const expiresAt = Date.now() + 30_000;
    wsTickets.set(ticket, expiresAt);
    sendJson(res, 200, {
      ticket,
      expires_at: expiresAt,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/status") {
    sendJson(res, 200, getStatusPayload());
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/assistant-desk") {
    sendJson(res, 200, buildAssistantDeskPayload());
    return;
  }

  const assistantDeskTranscriptMatch = requestUrl.pathname.match(
    /^\/api\/v1\/assistant-desk\/([^/]+)\/transcript$/
  );
  if (req.method === "GET" && assistantDeskTranscriptMatch) {
    const workItemId = decodeURIComponent(assistantDeskTranscriptMatch[1]);
    sendJson(res, 200, buildAssistantDeskTranscript(workItemId));
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/boards") {
    recalcBoardSummary();
    sendJson(res, 200, {
      items: [board],
    });
    return;
  }

  const boardCreateMatch = requestUrl.pathname.match(
    /^\/api\/v1\/boards\/([^/]+)\/cards\/create$/
  );
  if (req.method === "POST" && boardCreateMatch) {
    const boardId = decodeURIComponent(boardCreateMatch[1]);
    if (!findBoard(boardId)) {
      sendJson(res, 404, { error: "board not found" });
      return;
    }
    const payload = await readJson(req);
    const title = String(payload.title ?? "").trim();
    const columnId = String(payload.column_id ?? "").trim();
    if (!title || !columnId) {
      sendJson(res, 400, { error: "title and column_id are required" });
      return;
    }
    const targetColumn = columns.find(
      (column) => column.board_id === boardId && column.column_id === columnId
    );
    if (!targetColumn) {
      sendJson(res, 400, { error: "column_id is invalid" });
      return;
    }
    const currentColumnCards = cards.filter(
      (card) => card.board_id === boardId && card.column_id === columnId
    );
    const now = Date.now();
    const created = {
      card_id: `card-ops-${String(nextCardCounter++).padStart(2, "0")}`,
      board_id: boardId,
      column_id: columnId,
      title,
      description: null,
      owner_kind: String(payload.owner_kind ?? "unassigned"),
      owner_agent_id:
        typeof payload.owner_agent_id === "string" && payload.owner_agent_id.trim().length > 0
          ? payload.owner_agent_id.trim()
          : null,
      owner_human_id:
        typeof payload.owner_human_id === "string" && payload.owner_human_id.trim().length > 0
          ? payload.owner_human_id.trim()
          : null,
      due_at: null,
      tags: [],
      script_markdown: null,
      linked_session_id: null,
      latest_run_id: null,
      position: currentColumnCards.length,
      created_at: now,
      updated_at: now,
      assets: [],
    };
    cards.push(created);
    recalcBoardSummary();
    broadcastWsEvent({
      event_type: "board.card.created",
      entity: "board",
      payload: {
        domain: "boards",
        severity: "normal",
        summary: `Card created: ${created.title}`,
        board_id: boardId,
        card_id: created.card_id,
        column_id: created.column_id,
        title: created.title,
      },
    });
    sendJson(res, 200, {
      card: created,
    });
    return;
  }

  const boardUpdateMatch = requestUrl.pathname.match(
    /^\/api\/v1\/boards\/([^/]+)\/cards\/([^/]+)\/update$/
  );
  if (req.method === "POST" && boardUpdateMatch) {
    const boardId = decodeURIComponent(boardUpdateMatch[1]);
    const cardId = decodeURIComponent(boardUpdateMatch[2]);
    const existing = cards.find(
      (card) => card.board_id === boardId && card.card_id === cardId
    );
    if (!existing) {
      sendJson(res, 404, { error: "card not found" });
      return;
    }
    const payload = await readJson(req);
    if (typeof payload.title === "string" && payload.title.trim().length > 0) {
      existing.title = payload.title.trim();
    }
    if (payload.description === null || typeof payload.description === "string") {
      existing.description = payload.description;
    }
    if (typeof payload.owner_kind === "string") {
      existing.owner_kind = payload.owner_kind;
    }
    if (payload.owner_agent_id === null || typeof payload.owner_agent_id === "string") {
      existing.owner_agent_id =
        typeof payload.owner_agent_id === "string" && payload.owner_agent_id.trim().length > 0
          ? payload.owner_agent_id.trim()
          : null;
    }
    if (payload.owner_human_id === null || typeof payload.owner_human_id === "string") {
      existing.owner_human_id =
        typeof payload.owner_human_id === "string" && payload.owner_human_id.trim().length > 0
          ? payload.owner_human_id.trim()
          : null;
    }
    if (payload.due_at === null || typeof payload.due_at === "number") {
      existing.due_at = payload.due_at;
    }
    if (payload.script_markdown === null || typeof payload.script_markdown === "string") {
      existing.script_markdown = payload.script_markdown;
    }
    if (Array.isArray(payload.tags)) {
      existing.tags = payload.tags.filter((item) => typeof item === "string");
    } else if (payload.tags === null) {
      existing.tags = [];
    }
    existing.updated_at = Date.now();
    recalcBoardSummary();
    sendJson(res, 200, {
      card: existing,
    });
    return;
  }

  const boardMoveMatch = requestUrl.pathname.match(
    /^\/api\/v1\/boards\/([^/]+)\/cards\/([^/]+)\/move$/
  );
  if (req.method === "POST" && boardMoveMatch) {
    const boardId = decodeURIComponent(boardMoveMatch[1]);
    const cardId = decodeURIComponent(boardMoveMatch[2]);
    const existing = cards.find(
      (card) => card.board_id === boardId && card.card_id === cardId
    );
    if (!existing) {
      sendJson(res, 404, { error: "card not found" });
      return;
    }
    const payload = await readJson(req);
    const columnId = String(payload.column_id ?? "").trim();
    const beforeCardId =
      typeof payload.before_card_id === "string" && payload.before_card_id.trim().length > 0
        ? payload.before_card_id.trim()
        : null;
    if (!columnId) {
      sendJson(res, 400, { error: "column_id is required" });
      return;
    }
    const targetColumn = columns.find(
      (column) => column.board_id === boardId && column.column_id === columnId
    );
    if (!targetColumn) {
      sendJson(res, 400, { error: "column_id is invalid" });
      return;
    }
    const previousColumnId = existing.column_id;
    existing.column_id = columnId;
    existing.updated_at = Date.now();

    const siblings = sortByPosition(
      cards.filter(
        (card) =>
          card.board_id === boardId &&
          card.column_id === columnId &&
          card.card_id !== existing.card_id
      )
    );
    const beforeIndex =
      beforeCardId === null
        ? -1
        : siblings.findIndex((card) => card.card_id === beforeCardId);
    const insertIndex =
      beforeIndex < 0 ? siblings.length : beforeIndex;
    siblings.splice(insertIndex, 0, existing);
    for (let i = 0; i < siblings.length; i += 1) {
      siblings[i].position = i;
      siblings[i].updated_at = Date.now();
    }
    normalizeColumnPositions(boardId, previousColumnId);
    normalizeColumnPositions(boardId, columnId);
    recalcBoardSummary();
    broadcastWsEvent({
      event_type: "board.card.moved",
      entity: "board",
      payload: {
        domain: "boards",
        severity: "normal",
        summary: `Card moved: ${existing.title}`,
        board_id: boardId,
        card_id: existing.card_id,
        column_id: existing.column_id,
        position: existing.position,
      },
    });
    sendJson(res, 200, {
      card: existing,
    });
    return;
  }

  const boardRunMatch = requestUrl.pathname.match(
    /^\/api\/v1\/boards\/([^/]+)\/cards\/([^/]+)\/run$/
  );
  if (req.method === "POST" && boardRunMatch) {
    const boardId = decodeURIComponent(boardRunMatch[1]);
    const cardId = decodeURIComponent(boardRunMatch[2]);
    const existing = cards.find(
      (card) => card.board_id === boardId && card.card_id === cardId
    );
    if (!existing) {
      sendJson(res, 404, { error: "card not found" });
      return;
    }
    const runId = `run-${String(nextRunCounter++).padStart(4, "0")}`;
    existing.latest_run_id = runId;
    existing.updated_at = Date.now();
    recalcBoardSummary();
    broadcastWsEvent({
      event_type: "board.card.run",
      entity: "board",
      payload: {
        domain: "boards",
        severity: "high",
        summary: `Card run queued: ${existing.title}`,
        board_id: boardId,
        card_id: existing.card_id,
        run_id: runId,
      },
    });
    sendJson(res, 200, {
      card: existing,
      run: {
        run_id: runId,
        status: "queued",
      },
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname.startsWith("/api/v1/boards/")) {
    const boardId = decodeURIComponent(requestUrl.pathname.slice("/api/v1/boards/".length));
    if (boardId !== board.board_id) {
      sendJson(res, 404, { error: "board not found" });
      return;
    }
    recalcBoardSummary();
    sendJson(res, 200, {
      board,
      columns: sortByPosition(columns),
      cards: sortByPosition(cards),
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/agents") {
    sendJson(res, 200, {
      items: agents,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/agents") {
    const payload = await readJson(req);
    const agentId = String(payload.agent_id ?? "").trim();
    if (!agentId) {
      sendJson(res, 400, { error: "agent_id is required" });
      return;
    }
    const existing = agents.find((agent) => agent.agent_id === agentId);
    if (existing) {
      sendJson(res, 200, { agent: existing });
      return;
    }
    const created = {
      agent_id: agentId,
      name: String(payload.name ?? agentId),
      model_provider: String(payload.model_provider ?? "ollama"),
      model_id: String(payload.model_id ?? "qwen3.5-9b-instruct"),
      workspace_root: String(payload.workspace_root ?? "."),
      tool_profile: String(payload.tool_profile ?? "default"),
      role_label:
        payload.role_label === null || payload.role_label === undefined
          ? null
          : String(payload.role_label),
      reports_to_agent_id:
        payload.reports_to_agent_id === null || payload.reports_to_agent_id === undefined
          ? null
          : String(payload.reports_to_agent_id),
      memory_binding:
        payload.memory_binding && typeof payload.memory_binding === "object"
          ? cloneSeed(payload.memory_binding)
          : null,
    };
    agents.push(created);
    sendJson(res, 200, {
      agent: created,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname.startsWith("/api/v1/agents/")) {
    const agentId = decodeURIComponent(requestUrl.pathname.slice("/api/v1/agents/".length));
    const payload = await readJson(req);
    const existing = agents.find((agent) => agent.agent_id === agentId);
    if (!existing) {
      sendJson(res, 404, { error: "agent not found" });
      return;
    }
    if (typeof payload.name === "string") {
      existing.name = payload.name;
    }
    if (typeof payload.workspace_root === "string") {
      existing.workspace_root = payload.workspace_root;
    }
    if (typeof payload.model_provider === "string") {
      existing.model_provider = payload.model_provider;
    }
    if (typeof payload.model_id === "string") {
      existing.model_id = payload.model_id;
    }
    if (typeof payload.tool_profile === "string") {
      existing.tool_profile = payload.tool_profile;
    }
    if (payload.role_label === null || typeof payload.role_label === "string") {
      existing.role_label = payload.role_label;
    }
    if (
      payload.reports_to_agent_id === null ||
      typeof payload.reports_to_agent_id === "string"
    ) {
      existing.reports_to_agent_id = payload.reports_to_agent_id;
    }
    if (payload.memory_binding === null || typeof payload.memory_binding === "object") {
      existing.memory_binding =
        payload.memory_binding === null ? null : cloneSeed(payload.memory_binding);
    }
    sendJson(res, 200, {
      agent: existing,
    });
    return;
  }

  const agentMemoryStatusMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/status$/
  );
  if (req.method === "GET" && agentMemoryStatusMatch) {
    const agentId = decodeURIComponent(agentMemoryStatusMatch[1]);
    const agent = agents.find((item) => item.agent_id === agentId);
    if (!agent) {
      sendJson(res, 404, { error: "agent not found" });
      return;
    }
    sendJson(res, 200, {
      status: getAgentMemoryStatusPayload(agent),
    });
    return;
  }

  const agentMemoryCardsMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/cards$/
  );
  if (req.method === "GET" && agentMemoryCardsMatch) {
    const agentId = decodeURIComponent(agentMemoryCardsMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    const query = (requestUrl.searchParams.get("q") ?? "").trim().toLowerCase();
    const statusFilter = (requestUrl.searchParams.get("status") ?? "all").trim().toLowerCase();
    const filteredCards = lane.cardsPayload.cards.filter((card) => {
      if (statusFilter !== "all" && String(card.status ?? "").toLowerCase() !== statusFilter) {
        return false;
      }
      if (!query) {
        return true;
      }
      const haystack = [card.summary, card.atom_id, card.kind]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
    sendAgentMemoryJson(res, agentId, {
      ok: true,
      total: filteredCards.length,
      cards: filteredCards,
    });
    return;
  }

  const agentMemoryCardMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/cards\/([^/]+)$/
  );
  if (req.method === "GET" && agentMemoryCardMatch) {
    const agentId = decodeURIComponent(agentMemoryCardMatch[1]);
    const cardId = decodeURIComponent(agentMemoryCardMatch[2]);
    const lane = getAgentMemoryLane(agentId);
    const detail = lane?.cardDetails?.[cardId] ?? null;
    if (!detail) {
      sendJson(res, 404, { error: "memory card not found" });
      return;
    }
    sendAgentMemoryJson(res, agentId, detail);
    return;
  }

  const agentMemoryAtomMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/atom\/([^/]+)$/
  );
  if (req.method === "GET" && agentMemoryAtomMatch) {
    const agentId = decodeURIComponent(agentMemoryAtomMatch[1]);
    const atomId = decodeURIComponent(agentMemoryAtomMatch[2]);
    const lane = getAgentMemoryLane(agentId);
    const detail = lane?.atomDetails?.[atomId] ?? null;
    if (!detail) {
      sendJson(res, 404, { error: "memory atom not found" });
      return;
    }
    sendAgentMemoryJson(res, agentId, detail);
    return;
  }

  const agentMemoryEpisodesMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/episodes$/
  );
  if (req.method === "GET" && agentMemoryEpisodesMatch) {
    const agentId = decodeURIComponent(agentMemoryEpisodesMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    const query = (requestUrl.searchParams.get("q") ?? "").trim().toLowerCase();
    const episodes = lane.episodesPayload.episodes.filter((episode) => {
      if (!query) {
        return true;
      }
      const haystack = [episode.label, episode.run_id, episode.card_id]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
    sendAgentMemoryJson(res, agentId, {
      ok: true,
      total: episodes.length,
      episodes,
    });
    return;
  }

  const agentMemoryGraphMapMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/graph-map$/
  );
  if (req.method === "GET" && agentMemoryGraphMapMatch) {
    const agentId = decodeURIComponent(agentMemoryGraphMapMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    sendAgentMemoryJson(res, agentId, lane.graphMapPayload);
    return;
  }

  const agentMemoryNeighborsMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/graph\/neighbors$/
  );
  if (req.method === "GET" && agentMemoryNeighborsMatch) {
    const agentId = decodeURIComponent(agentMemoryNeighborsMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    const atomId = (requestUrl.searchParams.get("atom_id") ?? "").trim();
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    if (!atomId) {
      sendJson(res, 400, { error: "atom_id is required" });
      return;
    }
    const payload = lane.graphNeighborsByAtomId[atomId];
    if (!payload) {
      sendJson(res, 404, { error: "memory graph neighborhood not found" });
      return;
    }
    sendAgentMemoryJson(res, agentId, payload);
    return;
  }

  const agentMemoryWhyMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/turns\/([^/]+)\/why$/
  );
  if (req.method === "GET" && agentMemoryWhyMatch) {
    const agentId = decodeURIComponent(agentMemoryWhyMatch[1]);
    const turnId = decodeURIComponent(agentMemoryWhyMatch[2]);
    const lane = getAgentMemoryLane(agentId);
    const payload = lane?.whyByTurnId?.[turnId] ?? null;
    if (!payload) {
      sendJson(res, 404, { error: "memory why not found" });
      return;
    }
    sendAgentMemoryJson(res, agentId, payload);
    return;
  }

  const agentMemoryCitationMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/citations\/([^/]+)$/
  );
  if (req.method === "GET" && agentMemoryCitationMatch) {
    const agentId = decodeURIComponent(agentMemoryCitationMatch[1]);
    const citationToken = decodeURIComponent(agentMemoryCitationMatch[2]);
    const lane = getAgentMemoryLane(agentId);
    const payload = lane?.citationsByToken?.[citationToken] ?? null;
    if (!payload) {
      sendJson(res, 404, { error: "memory citation not found" });
      return;
    }
    sendAgentMemoryJson(res, agentId, payload);
    return;
  }

  const agentMemoryRuntimeHealthMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/runtime\/health$/
  );
  if (req.method === "GET" && agentMemoryRuntimeHealthMatch) {
    const agentId = decodeURIComponent(agentMemoryRuntimeHealthMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    sendAgentMemoryJson(res, agentId, lane.runtimeHealthPayload);
    return;
  }

  const agentMemoryTelemetrySummaryMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/runtime\/telemetry\/summary$/
  );
  if (req.method === "GET" && agentMemoryTelemetrySummaryMatch) {
    const agentId = decodeURIComponent(agentMemoryTelemetrySummaryMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    sendAgentMemoryJson(res, agentId, lane.telemetrySummaryPayload);
    return;
  }

  const agentMemoryTelemetryTurnsMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/runtime\/telemetry\/turns$/
  );
  if (req.method === "GET" && agentMemoryTelemetryTurnsMatch) {
    const agentId = decodeURIComponent(agentMemoryTelemetryTurnsMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    sendAgentMemoryJson(res, agentId, lane.telemetryTurnsPayload);
    return;
  }

  const agentMemoryDecisionReasonsMatch = requestUrl.pathname.match(
    /^\/api\/v1\/agents\/([^/]+)\/memory\/runtime\/decision-reasons$/
  );
  if (req.method === "GET" && agentMemoryDecisionReasonsMatch) {
    const agentId = decodeURIComponent(agentMemoryDecisionReasonsMatch[1]);
    const lane = getAgentMemoryLane(agentId);
    if (!lane) {
      sendJson(res, 424, { error: "assistant memory is unconfigured" });
      return;
    }
    sendAgentMemoryJson(res, agentId, lane.decisionReasonsPayload);
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/connectors/catalog") {
    sendJson(res, 200, {
      contract_version: "connector-registry-v1",
      items: connectorCatalog,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/connectors") {
    sendJson(res, 200, {
      contract_version: "connector-registry-v1",
      items: connectorSources,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/connectors/interactions") {
    sendJson(res, 200, {
      contract_version: "connector-registry-v1",
      items: listConnectorInteractions(),
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/connectors/import") {
    const payload = await readJson(req);
    const displayName = String(payload.display_name ?? "").trim();
    const sourceKind = String(payload.source_kind ?? "").trim().toLowerCase();
    if (!displayName) {
      sendJson(res, 400, { error: "display_name is required" });
      return;
    }
    if (!["openapi", "graphql", "mcp"].includes(sourceKind)) {
      sendJson(res, 400, { error: "source_kind must be openapi, graphql, or mcp" });
      return;
    }
    let sourceDocument = {};
    try {
      sourceDocument = parseConnectorSourceDocument(payload);
    } catch (error) {
      sendJson(res, 400, { error: String(error instanceof Error ? error.message : error) });
      return;
    }
    const slug =
      normalizeConnectorSlug(payload.slug || displayName) || `connector-${connectorSources.length + 1}`;
    const connectorId = `connector-${slug}`;
    const now = Date.now();
    const connector = {
      connector_id: connectorId,
      slug,
      display_name: displayName,
      source_kind: sourceKind,
      origin_kind: String(payload.origin_kind ?? (payload.catalog_item_id ? "curated" : "imported_local")),
      catalog_item_id:
        payload.catalog_item_id == null ? null : String(payload.catalog_item_id),
      current_version_id: null,
      latest_imported_version_id: null,
      status: "draft",
      trust_state: payload.catalog_item_id ? "trusted_curated" : "local_untrusted",
      assigned_agent_count: 0,
      published_tool_count: 0,
      last_conversion_at: null,
      last_review_at: null,
      last_enabled_at: null,
      last_disabled_at: null,
      created_at: now,
      updated_at: now,
    };
    const version = {
      version_id: randomUUID(),
      connector_id: connectorId,
      version_label: String(payload.version_label ?? "v1"),
      source_digest: randomUUID().replaceAll("-", ""),
      raw_source_location:
        payload.import_url != null
          ? String(payload.import_url)
          : payload.endpoint_url != null
            ? String(payload.endpoint_url)
            : null,
      import_metadata: {
        source_kind: sourceKind,
        catalog_item_id: connector.catalog_item_id,
        import_url: payload.import_url ?? null,
        endpoint_url: payload.endpoint_url ?? null,
        source_json: sourceDocument,
      },
      schema_summary: {
        source_kind: sourceKind,
      },
      latest_conversion_id: null,
      external_reference_policy: String(payload.external_reference_policy ?? "inline_only"),
      created_at: now,
      updated_at: now,
    };
    connector.latest_imported_version_id = version.version_id;
    connectorSources.push(connector);
    connectorVersions.push(version);
    sendJson(res, 200, {
      connector,
      version,
    });
    return;
  }

  const connectorDetailMatch = requestUrl.pathname.match(/^\/api\/v1\/connectors\/([^/]+)$/);
  if (req.method === "GET" && connectorDetailMatch) {
    const connectorId = decodeURIComponent(connectorDetailMatch[1]);
    const connector = connectorById(connectorId);
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    sendJson(res, 200, {
      connector,
      versions: listConnectorVersions(connectorId),
      conversions: listConnectorConversions(connectorId),
      published_tools: listConnectorPublishedTools(connectorId),
      assignments: listConnectorAssignments(connectorId),
      auth_bindings: listConnectorAuthBindings(connectorId),
      interactions: listConnectorInteractions(connectorId),
    });
    return;
  }

  const connectorConvertMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/convert$/
  );
  if (req.method === "POST" && connectorConvertMatch) {
    const connectorId = decodeURIComponent(connectorConvertMatch[1]);
    const payload = await readJson(req);
    const connector = connectorById(connectorId);
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    const version =
      (payload.version_id ? connectorVersionById(String(payload.version_id)) : null) ??
      (connector.latest_imported_version_id
        ? connectorVersionById(connector.latest_imported_version_id)
        : null) ??
      (connector.current_version_id ? connectorVersionById(connector.current_version_id) : null);
    if (!version) {
      sendJson(res, 400, { error: "connector has no version to convert" });
      return;
    }
    const conversion = createConnectorConversion(connector, version);
    recomputeConnectorStats(connectorId);
    sendJson(res, 200, {
      connector,
      version,
      conversion,
    });
    return;
  }

  const connectorPublishMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/publish$/
  );
  if (req.method === "POST" && connectorPublishMatch) {
    const connectorId = decodeURIComponent(connectorPublishMatch[1]);
    const payload = await readJson(req);
    const connector = connectorById(connectorId);
    const conversion = connectorConversionById(String(payload.conversion_id ?? ""));
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    if (!conversion || conversion.connector_id !== connectorId) {
      sendJson(res, 404, { error: "connector conversion not found" });
      return;
    }
    const version = connectorVersionById(conversion.version_id);
    if (!version) {
      sendJson(res, 404, { error: "connector version not found" });
      return;
    }
    const selectedIds = Array.isArray(payload.selected_candidate_ids)
      ? payload.selected_candidate_ids.map((item) => String(item))
      : [];
    const aliasOverrides = new Map(
      Array.isArray(payload.alias_overrides)
        ? payload.alias_overrides.map((item) => [
            String(item.candidate_id),
            String(item.alias ?? ""),
          ])
        : []
    );
    const published = conversion.proposed_tools
      .filter((candidate) => selectedIds.includes(candidate.candidate_id))
      .map((candidate) => {
        const alias = aliasOverrides.get(candidate.candidate_id)?.trim();
        const created = {
          published_tool_id: randomUUID(),
          connector_id: connectorId,
          version_id: version.version_id,
          conversion_id: conversion.conversion_id,
          tool_name: alias || candidate.proposed_tool_name,
          display_name: candidate.display_name,
          tool_schema: candidate.input_schema,
          origin_metadata: {
            operation_key: candidate.operation_key,
            description: candidate.description ?? null,
            source_kind: connector.source_kind,
          },
          write_classification: candidate.write_classification,
          published_at: Date.now(),
          unpublished_at: null,
          superseded_by_published_tool_id: null,
          deprecation_state: "active",
        };
        connectorPublishedTools.push(created);
        return created;
      });
    connector.current_version_id = version.version_id;
    connector.last_review_at = Date.now();
    connector.status = payload.enable_after_publish ? "enabled" : "converted";
    if (payload.enable_after_publish) {
      connector.last_enabled_at = Date.now();
      if (listConnectorAuthBindings(connectorId).length === 0) {
        upsertPendingAuthInteraction(connector, null);
      }
    }
    recomputeConnectorStats(connectorId);
    sendJson(res, 200, {
      connector,
      version,
      published_tools: listConnectorPublishedTools(connectorId),
    });
    return;
  }

  const connectorUnpublishMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/unpublish$/
  );
  if (req.method === "POST" && connectorUnpublishMatch) {
    const connectorId = decodeURIComponent(connectorUnpublishMatch[1]);
    const payload = await readJson(req);
    const connector = connectorById(connectorId);
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    const selectedIds = Array.isArray(payload.published_tool_ids)
      ? payload.published_tool_ids.map((item) => String(item))
      : [];
    selectedIds.forEach((publishedToolId) => {
      const tool = connectorPublishedToolById(publishedToolId);
      if (tool && tool.connector_id === connectorId) {
        tool.unpublished_at = Date.now();
        tool.deprecation_state = "unpublished";
      }
    });
    recomputeConnectorStats(connectorId);
    sendJson(res, 200, {
      connector,
      published_tools: listConnectorPublishedTools(connectorId),
    });
    return;
  }

  const connectorRollbackMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/rollback$/
  );
  if (req.method === "POST" && connectorRollbackMatch) {
    const connectorId = decodeURIComponent(connectorRollbackMatch[1]);
    const payload = await readJson(req);
    const connector = connectorById(connectorId);
    const version = connectorVersionById(String(payload.version_id ?? ""));
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    if (!version || version.connector_id !== connectorId) {
      sendJson(res, 404, { error: "connector version not found" });
      return;
    }
    connector.current_version_id = version.version_id;
    connector.status = "enabled";
    connector.last_enabled_at = Date.now();
    recomputeConnectorStats(connectorId);
    sendJson(res, 200, {
      connector,
      version,
      published_tools: listConnectorPublishedTools(connectorId).filter(
        (item) => item.version_id === version.version_id
      ),
    });
    return;
  }

  const connectorStateMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/state$/
  );
  if (req.method === "POST" && connectorStateMatch) {
    const connectorId = decodeURIComponent(connectorStateMatch[1]);
    const payload = await readJson(req);
    const connector = connectorById(connectorId);
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    const nextEnabled = Boolean(payload.enabled);
    connector.status = nextEnabled ? "enabled" : "disabled";
    if (nextEnabled) {
      connector.last_enabled_at = Date.now();
      if (listConnectorAuthBindings(connectorId).length === 0) {
        upsertPendingAuthInteraction(connector, null);
      }
    } else {
      connector.last_disabled_at = Date.now();
    }
    connector.updated_at = Date.now();
    recomputeConnectorStats(connectorId);
    sendJson(res, 200, {
      connector,
    });
    return;
  }

  const connectorAssignmentMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/assignments$/
  );
  if (req.method === "POST" && connectorAssignmentMatch) {
    const connectorId = decodeURIComponent(connectorAssignmentMatch[1]);
    const payload = await readJson(req);
    const connector = connectorById(connectorId);
    const agentId = String(payload.agent_id ?? "").trim();
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    if (!agentId) {
      sendJson(res, 400, { error: "agent_id is required" });
      return;
    }
    const existing =
      connectorAssignments.find(
        (item) => item.connector_id === connectorId && item.agent_id === agentId
      ) ?? null;
    if (existing) {
      existing.enabled = payload.enabled !== false;
      existing.auth_mode = String(payload.auth_mode ?? existing.auth_mode);
      existing.updated_at = Date.now();
      recomputeConnectorStats(connectorId);
      sendJson(res, 200, { assignment: existing });
      return;
    }
    const assignment = {
      assignment_id: randomUUID(),
      connector_id: connectorId,
      agent_id: agentId,
      enabled: payload.enabled !== false,
      auth_mode: String(payload.auth_mode ?? "shared_default"),
      created_at: Date.now(),
      updated_at: Date.now(),
    };
    connectorAssignments.push(assignment);
    recomputeConnectorStats(connectorId);
    sendJson(res, 200, { assignment });
    return;
  }

  const connectorAuthBindingMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/auth-bindings$/
  );
  if (req.method === "POST" && connectorAuthBindingMatch) {
    const connectorId = decodeURIComponent(connectorAuthBindingMatch[1]);
    const payload = await readJson(req);
    const connector = connectorById(connectorId);
    if (!connector) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    const agentId =
      payload.agent_id == null || String(payload.agent_id).trim() === ""
        ? null
        : String(payload.agent_id).trim();
    const existing =
      connectorAuthBindings.find(
        (item) => item.connector_id === connectorId && item.agent_id === agentId
      ) ?? null;
    if (existing) {
      existing.auth_kind = String(payload.auth_kind ?? existing.auth_kind);
      existing.secret_ref =
        payload.secret_ref == null || String(payload.secret_ref).trim() === ""
          ? null
          : String(payload.secret_ref).trim();
      existing.oauth_session_id =
        payload.oauth_session_id == null || String(payload.oauth_session_id).trim() === ""
          ? null
          : String(payload.oauth_session_id).trim();
      existing.status = String(payload.status ?? existing.status);
      existing.auth_metadata =
        payload.auth_metadata && typeof payload.auth_metadata === "object"
          ? cloneSeed(payload.auth_metadata)
          : {};
      existing.updated_at = Date.now();
      existing.last_success_at = Date.now();
      sendJson(res, 200, { binding: existing });
      return;
    }
    const binding = {
      auth_binding_id: randomUUID(),
      connector_id: connectorId,
      agent_id: agentId,
      auth_kind: String(payload.auth_kind ?? "none"),
      secret_ref:
        payload.secret_ref == null || String(payload.secret_ref).trim() === ""
          ? null
          : String(payload.secret_ref).trim(),
      oauth_session_id:
        payload.oauth_session_id == null || String(payload.oauth_session_id).trim() === ""
          ? null
          : String(payload.oauth_session_id).trim(),
      status: String(payload.status ?? "ready"),
      auth_metadata:
        payload.auth_metadata && typeof payload.auth_metadata === "object"
          ? cloneSeed(payload.auth_metadata)
          : {},
      last_success_at: Date.now(),
      last_error: null,
      last_rotated_at: Date.now(),
      created_at: Date.now(),
      updated_at: Date.now(),
    };
    connectorAuthBindings.push(binding);
    sendJson(res, 200, { binding });
    return;
  }

  const connectorHealthMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/health$/
  );
  if (req.method === "GET" && connectorHealthMatch) {
    const connectorId = decodeURIComponent(connectorHealthMatch[1]);
    const health = buildConnectorHealth(connectorId);
    if (!health) {
      sendJson(res, 404, { error: "connector not found" });
      return;
    }
    sendJson(res, 200, { health });
    return;
  }

  const connectorToolDetailMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/([^/]+)\/tools\/([^/]+)$/
  );
  if (req.method === "GET" && connectorToolDetailMatch) {
    const connectorId = decodeURIComponent(connectorToolDetailMatch[1]);
    const publishedToolId = decodeURIComponent(connectorToolDetailMatch[2]);
    const connector = connectorById(connectorId);
    const publishedTool = connectorPublishedToolById(publishedToolId);
    if (!connector || !publishedTool || publishedTool.connector_id !== connectorId) {
      sendJson(res, 404, { error: "connector tool not found" });
      return;
    }
    sendJson(res, 200, {
      connector,
      published_tool: publishedTool,
    });
    return;
  }

  const connectorInteractionResumeMatch = requestUrl.pathname.match(
    /^\/api\/v1\/connectors\/interactions\/([^/]+)\/resume$/
  );
  if (req.method === "POST" && connectorInteractionResumeMatch) {
    const interactionId = decodeURIComponent(connectorInteractionResumeMatch[1]);
    const interaction = connectorInteractions.find((item) => item.interaction_id === interactionId);
    if (!interaction) {
      sendJson(res, 404, { error: "interaction not found" });
      return;
    }
    interaction.status = "resumed";
    interaction.consumed_at = Date.now();
    interaction.updated_at = Date.now();
    interaction.resume_token = null;
    sendJson(res, 200, { interaction });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/providers/capabilities") {
    sendJson(res, 200, {
      contract_version: "stub-v1",
      items: [
        {
          provider: "ollama",
          supports_streaming: true,
          supports_tools: true,
          supports_json_mode: true,
          supports_vision: false,
          max_context_tokens: null,
          error_classes: [],
          retryable_error_classes: [],
        },
        {
          provider: "lmstudio",
          supports_streaming: true,
          supports_tools: true,
          supports_json_mode: true,
          supports_vision: false,
          max_context_tokens: null,
          error_classes: [],
          retryable_error_classes: [],
        },
      ],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/providers/models") {
    const provider = requestUrl.searchParams.get("provider") ?? "ollama";
    const items =
      provider === "anthropic"
        ? [
            {
              model_id: "claude-sonnet-4-5",
              label: "Claude Sonnet 4.5",
            },
            {
              model_id: "claude-opus-4-5",
              label: "Claude Opus 4.5",
            },
            {
              model_id: "claude-sonnet-4-6",
              label: "Claude Sonnet 4.6",
            },
            {
              model_id: "claude-opus-4-6",
              label: "Claude Opus 4.6",
            },
            {
              model_id: "claude-haiku-3-5",
              label: "Claude Haiku 3.5",
            },
          ]
        : provider === "openai"
          ? [
              {
                model_id: "gpt-5.4",
                label: "GPT-5.4",
              },
              {
                model_id: "gpt-5.4-mini",
                label: "GPT-5.4 Mini",
              },
            ]
          : [
              {
                model_id: "qwen3.5-9b-instruct",
                label: "Qwen 3.5 9B Instruct",
              },
              {
                model_id: "qwen3.5-4b-instruct",
                label: "Qwen 3.5 4B Instruct",
              },
            ];
    sendJson(res, 200, {
      contract_version: "stub-v1",
      provider,
      auth_profile_id: null,
      items,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/mission-control/calendar/week") {
    sendJson(res, 200, buildCalendarWeek());
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/mission-control/focus") {
    sendJson(res, 200, {
      generated_at_ms: Date.now(),
      items: getMissionControlFocusItems(),
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/mission-control/strategy/summary") {
    sendJson(res, 200, getStrategySummaryPayload());
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/mission-control/usage") {
    sendJson(res, 200, buildMissionControlUsage(requestUrl));
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/mission-control/runbooks") {
    const payload = buildRunbookListPayload(requestUrl);
    if (payload.error) {
      sendJson(res, 400, payload);
      return;
    }
    sendJson(res, 200, payload);
    return;
  }

  const runbookDetailMatch = requestUrl.pathname.match(
    /^\/api\/v1\/mission-control\/runbooks\/([^/]+)\/([^/]+)$/
  );
  if (req.method === "GET" && runbookDetailMatch) {
    const runbookKind = decodeURIComponent(runbookDetailMatch[1]);
    const anchorId = decodeURIComponent(runbookDetailMatch[2]);
    const detail = getRunbookDetailPayload(runbookKind, anchorId);
    if (!detail) {
      sendJson(res, 404, { error: "runbook not found" });
      return;
    }
    sendJson(res, 200, detail);
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/goals") {
    sendJson(res, 200, {
      items: strategyGoals,
      next_cursor: null,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/projects") {
    sendJson(res, 200, {
      items: strategyProjects,
      next_cursor: null,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/tasks") {
    sendJson(res, 200, {
      items: strategyTasks,
      next_cursor: null,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/bootstrap-presets") {
    sendJson(res, 200, {
      items: bootstrapPresets,
      next_cursor: null,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/jobs") {
    sendJson(res, 200, {
      items: jobs,
    });
    return;
  }

  const jobRunMatch = requestUrl.pathname.match(/^\/api\/v1\/jobs\/([^/]+)\/run$/);
  if (req.method === "POST" && jobRunMatch) {
    const jobId = decodeURIComponent(jobRunMatch[1]);
    const job = jobs.find((item) => item.job_id === jobId);
    if (!job) {
      sendJson(res, 404, { error: "job not found" });
      return;
    }
    const now = Date.now();
    job.last_run_at = now;
    job.last_error = null;
    job.updated_at = now;
    const run = {
      job_run_id: `job-run-${String(nextJobRunCounter++).padStart(4, "0")}`,
      status: "queued",
      attempt: 1,
      started_at: now,
      ended_at: null,
      error_text: null,
      output_json: null,
    };
    broadcastWsEvent({
      event_type: "job.run.queued",
      entity: "job",
      payload: {
        domain: "jobs",
        severity: "normal",
        summary: `Job run queued: ${job.name}`,
        job_id: job.job_id,
      },
    });
    sendJson(res, 200, {
      job_run: run,
    });
    return;
  }

  const jobUpdateMatch = requestUrl.pathname.match(/^\/api\/v1\/jobs\/([^/]+)\/update$/);
  if (req.method === "POST" && jobUpdateMatch) {
    const jobId = decodeURIComponent(jobUpdateMatch[1]);
    const job = jobs.find((item) => item.job_id === jobId);
    if (!job) {
      sendJson(res, 404, { error: "job not found" });
      return;
    }
    const payload = await readJson(req);
    if (typeof payload.enabled === "boolean") {
      job.enabled = payload.enabled;
      job.updated_at = Date.now();
    }
    sendJson(res, 200, {
      job,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/approvals") {
    const status = requestUrl.searchParams.get("status") ?? "";
    const limitRaw = Number(requestUrl.searchParams.get("limit") ?? "100");
    const limit = Number.isFinite(limitRaw)
      ? Math.max(1, Math.min(500, Math.floor(limitRaw)))
      : 100;
    const filtered = getApprovals(status).slice(0, limit);
    sendJson(res, 200, {
      items: filtered,
    });
    return;
  }

  const approvalResolveMatch = requestUrl.pathname.match(
    /^\/api\/v1\/approvals\/([^/]+)\/resolve$/
  );
  if (req.method === "POST" && approvalResolveMatch) {
    const approvalId = decodeURIComponent(approvalResolveMatch[1]);
    const approval = approvals.find((item) => item.approval_id === approvalId);
    if (!approval) {
      sendJson(res, 404, { error: "approval not found" });
      return;
    }
    const payload = await readJson(req);
    const decision = payload.decision === "deny" ? "deny" : "approve";
    approval.status = decision === "deny" ? "denied" : "approved";
    approval.decided_at = Date.now();
    broadcastWsEvent({
      event_type: "approval.resolved",
      entity: "approval",
      payload: {
        domain: "approvals",
        severity: "high",
        summary: `Approval ${decision}: ${approval.approval_id}`,
        approval_id: approval.approval_id,
        status: approval.status,
      },
    });
    sendJson(res, 200, {
      approval,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/channels/runtime/status") {
    reconcileMockChannelStatus("discord");
    reconcileMockChannelStatus("telegram");
    sendJson(res, 200, {
      updated_at: Date.now(),
      items: channelStatuses,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/channels/runtime/reconnect") {
    const payload = await readJson(req);
    const provider =
      typeof payload.provider === "string" && payload.provider.trim().length > 0
        ? payload.provider.trim()
        : "ollama";
    let target = channelStatuses.find((item) => item.provider === provider);
    if (!target) {
      target = upsertMockChannelStatus(provider, {
        lifecycle_state: "stopped",
        healthy: false,
        detail: "not configured yet",
        last_error: null,
        reconnect_attempts: 0,
      });
    }
    target.reconnect_attempts += 1;
    if (provider === "discord" || provider === "telegram") {
      target = reconcileMockChannelStatus(provider) ?? target;
      target.reconnect_attempts += 0;
    } else {
      target.lifecycle_state = "running";
      target.healthy = true;
      target.last_error = null;
      target.detail = "reconnected via mock endpoint";
      target.updated_at = Date.now();
    }
    broadcastWsEvent({
      event_type: "channel.runtime.reconnected",
      entity: "channel",
      payload: {
        domain: "channels",
        severity: "normal",
        summary: `Channel reconnected: ${provider}`,
        provider,
      },
    });
    sendJson(res, 200, {
      status: {
        provider: target.provider,
        healthy: target.healthy,
        lifecycle_state: target.lifecycle_state,
      },
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/config/channels") {
    sendJson(res, 200, {
      config: channelConfig,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/config/channels") {
    const payload = await readJson(req);
    if (payload.discord) {
      channelConfig.discord = {
        ...channelConfig.discord,
        ...payload.discord,
      };
    }
    if (payload.telegram) {
      channelConfig.telegram = {
        ...channelConfig.telegram,
        ...payload.telegram,
      };
    }
    channelConfig.updated_at = Date.now();
    telegramPairingStatus.dm_policy = channelConfig.telegram.dm_policy;
    telegramPairingStatus.group_policy = channelConfig.telegram.group_policy;
    telegramPairingStatus.auto_leave_unauthorized_groups =
      channelConfig.telegram.auto_leave_unauthorized_groups;
    sendJson(res, 200, {
      config: channelConfig,
    });
    return;
  }

  if (
    req.method === "GET"
    && requestUrl.pathname === "/api/v1/channels/telegram/pairing/status"
  ) {
    sendJson(res, 200, telegramPairingStatus);
    return;
  }

  if (
    req.method === "POST"
    && requestUrl.pathname === "/api/v1/channels/telegram/pairing/approve"
  ) {
    const payload = await readJson(req);
    const code = String(payload.code ?? "").trim().toUpperCase();
    const match = telegramPairingStatus.pending_requests.find((item) => item.code === code);
    if (!match) {
      sendJson(res, 404, { error: "pairing code not found" });
      return;
    }
    if (!channelConfig.telegram.allowlisted_user_ids.includes(match.user_id)) {
      channelConfig.telegram.allowlisted_user_ids.push(match.user_id);
    }
    telegramPairingStatus.pending_requests = telegramPairingStatus.pending_requests.filter(
      (item) => item.code !== code
    );
    telegramPairingStatus.updated_at = Date.now();
    sendJson(res, 200, {
      status: telegramPairingStatus,
      approved_user_id: match.user_id,
    });
    return;
  }

  if (
    req.method === "POST"
    && requestUrl.pathname === "/api/v1/channels/telegram/pairing/deny"
  ) {
    const payload = await readJson(req);
    const code = String(payload.code ?? "").trim().toUpperCase();
    const match = telegramPairingStatus.pending_requests.find((item) => item.code === code);
    if (!match) {
      sendJson(res, 404, { error: "pairing code not found" });
      return;
    }
    telegramPairingStatus.pending_requests = telegramPairingStatus.pending_requests.filter(
      (item) => item.code !== code
    );
    telegramPairingStatus.blocked_senders.push({
      user_id: match.user_id,
      blocked_until: Date.now() + 60 * 60 * 1000,
      reason: "operator_denied",
      attempt_count: Math.max(1, match.attempt_count ?? 1),
      last_attempt_at: Date.now(),
    });
    telegramPairingStatus.updated_at = Date.now();
    sendJson(res, 200, {
      status: telegramPairingStatus,
      approved_user_id: null,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/config/runtime") {
    sendJson(res, 200, {
      config: runtimeConfig,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/config/runtime") {
    const payload = await readJson(req);
    if (payload.global) {
      runtimeConfig.global = {
        ...runtimeConfig.global,
        ...payload.global,
      };
    }
    if (payload.channels) {
      runtimeConfig.channels = {
        ...runtimeConfig.channels,
      };
      for (const [channel, update] of Object.entries(payload.channels)) {
        runtimeConfig.channels[channel] = {
          ...(runtimeConfig.channels[channel] ?? {}),
          ...(update ?? {}),
        };
      }
    }
    if (payload.routing) {
      runtimeConfig.routing = {
        ...runtimeConfig.routing,
        ...payload.routing,
      };
    }
    if (payload.memory) {
      runtimeConfig.memory = {
        ...runtimeConfig.memory,
        ...payload.memory,
        numquam: {
          ...runtimeConfig.memory.numquam,
          ...(payload.memory.numquam ?? {}),
        },
      };
    }
    if (payload.extensions) {
      runtimeConfig.extensions = {
        ...runtimeConfig.extensions,
        ...payload.extensions,
        assistant_tools: {
          ...runtimeConfig.extensions.assistant_tools,
          ...(payload.extensions.assistant_tools ?? {}),
          limits: {
            ...runtimeConfig.extensions.assistant_tools.limits,
            ...(payload.extensions.assistant_tools?.limits ?? {}),
          },
        },
      };
    }
    if (payload.security) {
      runtimeConfig.security = {
        ...runtimeConfig.security,
        ...payload.security,
      };
    }
    if (payload.autonomy_guardrails) {
      runtimeConfig.autonomy_guardrails = {
        ...runtimeConfig.autonomy_guardrails,
        ...payload.autonomy_guardrails,
      };
    }
    runtimeConfig.updated_at = Date.now();
    reconcileMockChannelStatus("discord");
    reconcileMockChannelStatus("telegram");
    sendJson(res, 200, {
      config: runtimeConfig,
    });
    return;
  }

  if (
    req.method === "POST"
    && requestUrl.pathname === "/api/v1/config/runtime/secrets/upsert"
  ) {
    const payload = await readJson(req);
    const scope = String(payload.scope ?? "").trim();
    const secretValue = String(payload.secret_value ?? "").trim();
    if (!scope || !secretValue) {
      sendJson(res, 400, { error: "scope and secret_value are required" });
      return;
    }
    const secretKey = runtimeSecretKeyFromScope(scope);
    runtimeSecrets[secretKey] = secretValue;
    const secretRef = runtimeSecretRefFromKey(secretKey);
    const previousRef = String(payload.previous_secret_ref ?? "").trim();
    if (previousRef) {
      const previousKey = runtimeSecretKeyFromRef(previousRef);
      if (previousKey && previousKey !== secretKey) {
        delete runtimeSecrets[previousKey];
      }
    }
    sendJson(res, 200, {
      secret_ref: secretRef,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/jobs/status") {
    sendJson(res, 200, getJobsStatusPayload());
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/auth/profiles") {
    sendJson(res, 200, {
      items: authProfiles,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/auth/anthropic/setup-token/validate") {
    const payload = await readJson(req);
    const setupToken = typeof payload.setup_token === "string" ? payload.setup_token.trim() : "";
    if (!setupToken) {
      sendJson(res, 400, { error: "setup token required" });
      return;
    }
    sendJson(res, 200, { valid: looksLikeAnthropicSetupToken(setupToken) });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/auth/anthropic/setup-token/ingest") {
    const payload = await readJson(req);
    const setupToken = typeof payload.setup_token === "string" ? payload.setup_token.trim() : "";
    if (!setupToken) {
      sendJson(res, 400, { error: "setup token required" });
      return;
    }
    if (!looksLikeAnthropicSetupToken(setupToken)) {
      sendJson(res, 400, { error: "setup token validation failed" });
      return;
    }
    const now = Date.now();
    const displayName = String(payload.display_name ?? "claude-primary");
    const existingIndex = authProfiles.findIndex(
      (profile) => profile.provider === "anthropic" && profile.display_name === displayName
    );
    const profile =
      existingIndex >= 0
        ? {
            ...authProfiles[existingIndex],
            auth_mode: "api_key",
            risk_level: "low",
            enabled: payload.enabled !== false,
            kill_switch_scope: String(payload.kill_switch_scope ?? "none"),
            api_base_url: typeof payload.api_base_url === "string" ? payload.api_base_url : null,
            updated_at: now,
          }
        : {
            auth_profile_id: `profile-${String(nextAuthProfileCounter++).padStart(3, "0")}`,
            provider: "anthropic",
            display_name: displayName,
            auth_mode: "api_key",
            risk_level: "low",
            enabled: payload.enabled !== false,
            kill_switch_scope: String(payload.kill_switch_scope ?? "none"),
            api_base_url: typeof payload.api_base_url === "string" ? payload.api_base_url : null,
            created_at: now,
            updated_at: now,
          };
    if (existingIndex >= 0) {
      authProfiles[existingIndex] = profile;
      sendJson(res, 200, { profile });
      return;
    }
    authProfiles.push(profile);
    sendJson(res, 200, { profile });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/auth/profiles") {
    const payload = await readJson(req);
    const now = Date.now();
    const profile = {
      auth_profile_id: `profile-${String(nextAuthProfileCounter++).padStart(3, "0")}`,
      provider: String(payload.provider ?? "unknown"),
      display_name: String(payload.display_name ?? "stub-profile"),
      auth_mode: String(payload.auth_mode ?? "api_key"),
      risk_level: String(payload.risk_level ?? "low"),
      enabled: payload.enabled !== false,
      kill_switch_scope: String(payload.kill_switch_scope ?? "none"),
      api_base_url: typeof payload.api_base_url === "string" ? payload.api_base_url : null,
      created_at: now,
      updated_at: now,
    };
    authProfiles.push(profile);
    sendJson(res, 200, {
      profile,
    });
    return;
  }

  if (
    requestUrl.pathname.startsWith("/api/v1/auth/agents/") &&
    requestUrl.pathname.endsWith("/profile-order")
  ) {
    const parts = requestUrl.pathname.split("/");
    const agentId = decodeURIComponent(parts[5] ?? "");
    const provider = decodeURIComponent(parts[7] ?? "");
    const key = `${agentId}::${provider}`;

    if (req.method === "GET") {
      sendJson(res, 200, {
        agent_id: agentId,
        provider,
        profile_ids: providerOrder.get(key) ?? [],
      });
      return;
    }

    if (req.method === "POST") {
      const payload = await readJson(req);
      const profileIds = Array.isArray(payload.profile_ids)
        ? payload.profile_ids.filter((value) => typeof value === "string")
        : [];
      providerOrder.set(key, profileIds);
      sendJson(res, 200, {
        agent_id: agentId,
        provider,
        profile_ids: profileIds,
      });
      return;
    }
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/extensions/skills") {
    sendJson(res, 200, {
      contract_version: "stub-v1",
      items: [],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/extensions/plugins") {
    sendJson(res, 200, {
      contract_version: "stub-v1",
      plugin_api_version: "v1",
      items: [],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/extensions/plugins/status") {
    sendJson(res, 200, {
      contract_version: "stub-v1",
      items: [],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/agent-mail/threads") {
    sendJson(res, 200, {
      items: [],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/agent-mail/leases") {
    sendJson(res, 200, {
      items: [],
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/e2e/ws-event") {
    const payload = await readJson(req);
    const event = broadcastWsEvent({
      event_type:
        typeof payload.event_type === "string" ? payload.event_type : "gateway.notice",
      entity: typeof payload.entity === "string" ? payload.entity : "system",
      payload: payload.payload && typeof payload.payload === "object" ? payload.payload : {},
    });
    sendJson(res, 200, {
      event,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/e2e/usage-mode") {
    const payload = await readJson(req);
    const mode =
      typeof payload.mode === "string" && payload.mode.trim().length > 0
        ? payload.mode.trim()
        : "available";
    if (!["available", "unavailable", "missing-optional", "invalid-required"].includes(mode)) {
      sendJson(res, 400, {
        error: "mode must be one of available|unavailable|missing-optional|invalid-required",
      });
      return;
    }
    usageMode = mode;
    if (Number.isFinite(payload.updated_at_ms)) {
      usageUpdatedAtMs = Math.trunc(Number(payload.updated_at_ms));
    } else if (Number.isFinite(payload.age_minutes)) {
      usageUpdatedAtMs = Date.now() - Math.max(0, Math.trunc(Number(payload.age_minutes))) * 60_000;
    } else {
      usageUpdatedAtMs = Date.now();
    }
    sendJson(res, 200, {
      mode: usageMode,
      updated_at_ms: usageUpdatedAtMs,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/e2e/ws-burst") {
    const payload = await readJson(req);
    const countRaw = Number(payload.count ?? 5);
    const count = Number.isFinite(countRaw)
      ? Math.max(1, Math.min(500, Math.floor(countRaw)))
      : 5;
    const eventType =
      typeof payload.event_type === "string" ? payload.event_type : "gateway.notice";
    const entity = typeof payload.entity === "string" ? payload.entity : "system";
    const eventPayload =
      payload.payload && typeof payload.payload === "object" ? payload.payload : {};

    const emitted = [];
    for (let i = 0; i < count; i += 1) {
      emitted.push(
        broadcastWsEvent({
          event_type: eventType,
          entity,
          payload: eventPayload,
        })
      );
    }
    sendJson(res, 200, {
      count,
      events: emitted,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/e2e/ws-malformed") {
    const payload = await readJson(req);
    const raw =
      typeof payload.raw === "string" && payload.raw.length > 0
        ? payload.raw
        : "{malformed-json";
    sendMalformedWsPayload(raw);
    sendJson(res, 200, {
      delivered: wsClients.size,
      raw,
    });
    return;
  }

  if (req.method === "POST" && requestUrl.pathname === "/api/v1/e2e/ws-flap") {
    const payload = await readJson(req);
    const countRaw = Number(payload.count ?? 1);
    const intervalRaw = Number(payload.interval_ms ?? 75);
    const count = Number.isFinite(countRaw)
      ? Math.max(1, Math.min(8, Math.floor(countRaw)))
      : 1;
    const intervalMs = Number.isFinite(intervalRaw)
      ? Math.max(0, Math.min(2_000, Math.floor(intervalRaw)))
      : 75;
    const codeRaw = Number(payload.code ?? 1012);
    const code = Number.isFinite(codeRaw) ? Math.max(1000, Math.min(4999, Math.floor(codeRaw))) : 1012;
    for (let i = 0; i < count; i += 1) {
      setTimeout(() => {
        closeAllWsConnections(code, "e2e-flap");
      }, i * intervalMs);
    }
    sendJson(res, 200, {
      count,
      interval_ms: intervalMs,
      code,
      ws_clients: wsClients.size,
    });
    return;
  }

  sendText(res, 404, `stub route not found: ${req.method} ${requestUrl.pathname}`);
}

const server = http.createServer((req, res) => {
  void routeRequest(req, res).catch((error) => {
    sendJson(res, 500, {
      error: `stub gateway internal error: ${String(error)}`,
    });
  });
});

const wsServer = new WebSocketServer({ noServer: true });

wsServer.on("connection", (socket) => {
  wsClients.add(socket);
  socket.send(JSON.stringify(buildWsEvent()));
  const intervalId = setInterval(() => {
    if (socket.readyState === 1) {
      socket.send(JSON.stringify(buildWsEvent()));
    }
  }, 2_000);
  socket.on("close", () => {
    clearInterval(intervalId);
    wsClients.delete(socket);
  });
});

server.on("upgrade", (req, socket, head) => {
  const requestUrl = new URL(req.url ?? "/", `http://${req.headers.host ?? "127.0.0.1"}`);
  if (requestUrl.pathname !== "/api/v1/ws") {
    socket.destroy();
    return;
  }
  const ticket = requestUrl.searchParams.get("ticket");
  const expiresAt = ticket ? wsTickets.get(ticket) : undefined;
  if (!ticket || !expiresAt || Date.now() > expiresAt) {
    if (ticket) {
      wsTickets.delete(ticket);
    }
    socket.destroy();
    return;
  }
  wsTickets.delete(ticket);
  wsServer.handleUpgrade(req, socket, head, (ws) => {
    wsServer.emit("connection", ws, req);
  });
});

server.listen(port, "127.0.0.1", () => {
  console.log(`[mock-gateway] listening on http://127.0.0.1:${port}`);
});

function shutdown() {
  wsServer.clients.forEach((client) => {
    try {
      client.close();
    } catch {
      // no-op
    }
  });
  wsServer.close();
  server.close(() => {
    process.exit(0);
  });
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
process.on("uncaughtException", (error) => {
  console.error("[mock-gateway] uncaught exception", error);
  shutdown();
});
