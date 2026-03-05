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
let nextAuthProfileCounter = 1;
let nextEventCounter = 1;
let nextCardCounter = 2;
let nextRunCounter = 1;
let nextJobRunCounter = 1;
const wsClients = new Set();

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
    owner_agent_id: "lyra",
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
    agent_id: "lyra",
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
    agent_id: "lyra",
    name: "Lyra",
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "default",
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
    detail: "stub",
    last_error: null,
    reconnect_attempts: 0,
    updated_at: Date.now(),
  },
];

const authProfiles = [];
const providerOrder = new Map();

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

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/health") {
    sendJson(res, 200, {
      status: "ok",
      service: "carsinos-gateway-stub",
      ok: true,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/status") {
    sendJson(res, 200, getStatusPayload());
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
    sendJson(res, 200, {
      agent: existing,
    });
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
    sendJson(res, 200, {
      contract_version: "stub-v1",
      provider,
      auth_profile_id: null,
      items: [
        {
          model_id: "qwen3.5-9b-instruct",
          label: "Qwen 3.5 9B Instruct",
        },
        {
          model_id: "qwen3.5-4b-instruct",
          label: "Qwen 3.5 4B Instruct",
        },
      ],
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
      target = {
        provider,
        lifecycle_state: "running",
        healthy: true,
        detail: "stub",
        last_error: null,
        reconnect_attempts: 0,
        updated_at: Date.now(),
      };
      channelStatuses.push(target);
    }
    target.reconnect_attempts += 1;
    target.lifecycle_state = "running";
    target.healthy = true;
    target.last_error = null;
    target.detail = "reconnected via mock endpoint";
    target.updated_at = Date.now();
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
  const token = requestUrl.searchParams.get("token");
  if (!token || token.trim().length === 0) {
    socket.destroy();
    return;
  }
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
