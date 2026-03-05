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
const wsClients = new Set();

const board = {
  board_id: "ops-board",
  board_key: "ops",
  name: "Operations Board",
  board_type: "kanban",
  created_at: createdAt,
  updated_at: createdAt,
  column_count: 1,
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

const authProfiles = [];
const providerOrder = new Map();

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
    sendJson(res, 200, {
      items: [board],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname.startsWith("/api/v1/boards/")) {
    const boardId = decodeURIComponent(requestUrl.pathname.slice("/api/v1/boards/".length));
    if (boardId !== board.board_id) {
      sendJson(res, 404, { error: "board not found" });
      return;
    }
    sendJson(res, 200, {
      board,
      columns,
      cards,
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
      items: [
        {
          item_id: "focus-001",
          category: "status",
          severity: "normal",
          title: "All systems nominal",
          detail: "Deterministic stub focus event",
          primary_action: "none",
          action_payload: {},
          created_at: Date.now() - 5_000,
        },
      ],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/jobs") {
    sendJson(res, 200, {
      items: jobs,
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/approvals") {
    sendJson(res, 200, {
      items: [],
    });
    return;
  }

  if (req.method === "GET" && requestUrl.pathname === "/api/v1/channels/runtime/status") {
    sendJson(res, 200, {
      updated_at: Date.now(),
      items: [
        {
          provider: agents[0]?.model_provider ?? "ollama",
          lifecycle_state: "connected",
          healthy: true,
          detail: "stub",
          last_error: null,
          reconnect_attempts: 0,
          updated_at: Date.now(),
        },
      ],
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
      ? Math.max(1, Math.min(100, Math.floor(countRaw)))
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
