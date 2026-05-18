const fs = require("node:fs");
const path = require("node:path");
const { spawn } = require("node:child_process");
const { assertAllowedRoot, redact, splitPathList } = require("./safe.js");

const DEFAULT_ALLOWED_ROOTS = [
  "Z:\\carsinos-codex-work",
  "\\\\192.168.1.83\\Documents\\openclaw replacement\\carsinos",
];

function defaultCodexHome() {
  return process.env.CODEX_HOME || path.join(process.env.USERPROFILE || "", ".codex");
}

function readSessionIndex(codexHome = defaultCodexHome(), limit = 25) {
  const indexPath = path.join(codexHome, "session_index.jsonl");
  if (!fs.existsSync(indexPath)) return { indexPath, items: [] };
  const lines = fs
    .readFileSync(indexPath, "utf8")
    .split(/\r?\n/)
    .filter(Boolean)
    .slice(-Math.max(1, Math.min(Number(limit) || 25, 200)));
  const items = [];
  for (const line of lines) {
    try {
      const parsed = JSON.parse(line);
      items.push({
        id: parsed.id || null,
        threadName: parsed.thread_name || parsed.threadName || null,
        updatedAt: parsed.updated_at || parsed.updatedAt || null,
      });
    } catch {
      items.push({ raw: line.slice(0, 200) });
    }
  }
  return { indexPath, items: items.reverse() };
}

function parseProcessRows(raw) {
  const value = JSON.parse(raw || "[]");
  const rows = Array.isArray(value) ? value : [value];
  return rows
    .filter((row) => row && (row.Name || row.ProcessName))
    .filter((row) => {
      const name = String(row.Name || row.ProcessName || "").toLowerCase();
      const commandLine = String(row.CommandLine || "").toLowerCase();
      return name === "codex.exe" || name === "codex" || name === "codex.exe" || commandLine.includes("codex.exe app-server") || commandLine.includes("codex app-server");
    })
    .map((row) => redact({
      pid: row.ProcessId || row.Id,
      parentPid: row.ParentProcessId || null,
      name: row.Name || row.ProcessName,
      commandLine: row.CommandLine || null,
      path: row.Path || null,
      mainWindowTitle: row.MainWindowTitle || null,
      startTime: row.StartTime || null,
    }));
}

function truncateText(value, maxChars = 600) {
  const text = String(value || "");
  if (text.length <= maxChars) return text;
  return `${text.slice(0, maxChars)}... [truncated ${text.length - maxChars} chars]`;
}

function compactThread(thread, previewChars = 600) {
  if (!thread || typeof thread !== "object") return thread;
  return redact({
    id: thread.id || null,
    name: thread.name || thread.threadName || null,
    status: thread.status || null,
    source: thread.source || null,
    cwd: thread.cwd || null,
    modelProvider: thread.modelProvider || null,
    createdAt: thread.createdAt || null,
    updatedAt: thread.updatedAt || null,
    preview: truncateText(thread.preview || "", previewChars),
    gitInfo: thread.gitInfo || null,
    path: thread.path || null,
  });
}

function compactThreadListResponse(response, previewChars = 600) {
  if (!response?.result?.data || !Array.isArray(response.result.data)) return redact(response);
  return {
    ...response,
    result: {
      ...response.result,
      data: response.result.data.map((thread) => compactThread(thread, previewChars)),
    },
  };
}

function compactThreadReadResponse(response, previewChars = 1200) {
  if (!response?.result?.thread) return redact(response);
  return {
    ...response,
    result: {
      ...response.result,
      thread: compactThread(response.result.thread, previewChars),
    },
  };
}

function extractLatestAgentText(thread) {
  const turns = Array.isArray(thread?.turns) ? thread.turns : [];
  for (const turn of turns.slice().reverse()) {
    const items = Array.isArray(turn.items) ? turn.items : [];
    for (const item of items.slice().reverse()) {
      if (item?.type === "agentMessage" && typeof item.text === "string") {
        return item.text;
      }
      if (item?.type === "message" && item.role === "assistant") {
        const content = Array.isArray(item.content) ? item.content : [];
        const text = content
          .map((part) => part?.text || "")
          .filter(Boolean)
          .join("");
        if (text) return text;
      }
    }
  }
  return "";
}

function listCodexProcesses() {
  if (process.platform !== "win32") return [];
  const ps = [
    "Get-CimInstance Win32_Process |",
    "Where-Object { $_.Name -match '^(Codex|codex)(\\\\.exe)?$' -or $_.CommandLine -match 'codex(\\\\.exe)? app-server' } |",
    "Select-Object ProcessId,ParentProcessId,Name,CommandLine | ConvertTo-Json -Depth 4",
  ].join(" ");
  const result = require("node:child_process").spawnSync("powershell.exe", [
    "-NoProfile",
    "-Command",
    ps,
  ], { encoding: "utf8", windowsHide: true });
  if (result.status !== 0) return [];
  return parseProcessRows(result.stdout);
}

class JsonRpcWsClient {
  constructor(url) {
    this.url = url;
    this.nextId = 1;
    this.pending = new Map();
    this.notifications = [];
    this.waiters = [];
  }

  async connect() {
    this.ws = new WebSocket(this.url);
    this.ws.onmessage = (event) => {
      const raw = typeof event.data === "string" ? event.data : String(event.data);
      let msg;
      try {
        msg = JSON.parse(raw);
      } catch {
        this.notifications.push({ raw: raw.slice(0, 500) });
        return;
      }
      if (msg.id != null && this.pending.has(msg.id)) {
        const { resolve } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        resolve(msg);
      } else {
        this.notifications.push(msg);
        if (this.notifications.length > 200) this.notifications.shift();
        const remaining = [];
        for (const waiter of this.waiters) {
          try {
            if (waiter.predicate(msg)) {
              clearTimeout(waiter.timer);
              waiter.resolve(msg);
            } else {
              remaining.push(waiter);
            }
          } catch (err) {
            clearTimeout(waiter.timer);
            waiter.reject(err);
          }
        }
        this.waiters = remaining;
      }
    };
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("codex app-server websocket open timed out")), 10000);
      this.ws.onopen = () => {
        clearTimeout(timer);
        resolve();
      };
      this.ws.onerror = (event) => {
        clearTimeout(timer);
        reject(new Error(`codex app-server websocket error: ${event.message || "unknown"}`));
      };
    });
    return this;
  }

  async call(method, params, timeoutMs = 20000) {
    const id = this.nextId++;
    this.ws.send(JSON.stringify({ method, id, params }));
    return await new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`codex app-server request timed out: ${method}`));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
      });
    });
  }

  waitFor(predicate, timeoutMs = 120000) {
    return new Promise((resolve, reject) => {
      const existing = this.notifications.find((msg) => predicate(msg));
      if (existing) return resolve(existing);
      const timer = setTimeout(() => {
        this.waiters = this.waiters.filter((item) => item.timer !== timer);
        reject(new Error("codex app-server notification wait timed out"));
      }, timeoutMs);
      this.waiters.push({ predicate, resolve, reject, timer });
    });
  }

  close() {
    if (this.ws) this.ws.close();
  }
}

class CodexAppBridge {
  constructor(options = {}) {
    this.codexBin = options.codexBin || process.env.CODEX_BRIDGE_CODEX_BIN || "codex.exe";
    this.port = Number(options.port || process.env.CODEX_APP_BRIDGE_PORT || 17909);
    this.url = options.url || `ws://127.0.0.1:${this.port}`;
    this.codexHome = options.codexHome || defaultCodexHome();
    this.allowedRoots =
      options.allowedRoots ||
      splitPathList(process.env.CODEX_BRIDGE_ALLOWED_ROOTS).concat(DEFAULT_ALLOWED_ROOTS);
    this.appServerProcess = null;
    this.logsDir = path.resolve(options.logsDir || path.join(__dirname, "..", "runtime", "codex-app"));
  }

  status() {
    return redact({
      url: this.url,
      codexHome: this.codexHome,
      appServerPid: this.appServerProcess?.pid || null,
      processes: listCodexProcesses(),
      sessions: readSessionIndex(this.codexHome, 20).items,
    });
  }

  async ensureServer() {
    try {
      const res = await fetch(`http://127.0.0.1:${this.port}/readyz`);
      if (res.ok) return { alreadyRunning: true, url: this.url };
    } catch {}
    fs.mkdirSync(this.logsDir, { recursive: true });
    const out = fs.openSync(path.join(this.logsDir, "app-server.out.log"), "a");
    const err = fs.openSync(path.join(this.logsDir, "app-server.err.log"), "a");
    try {
      this.appServerProcess = spawn(this.codexBin, ["app-server", "--listen", this.url], {
        windowsHide: true,
        stdio: ["ignore", out, err],
        detached: false,
        env: { ...process.env, CODEX_HOME: this.codexHome },
      });
    } finally {
      fs.closeSync(out);
      fs.closeSync(err);
    }
    const startedAt = Date.now();
    while (Date.now() - startedAt < 15000) {
      try {
        const res = await fetch(`http://127.0.0.1:${this.port}/readyz`);
        if (res.ok) return { alreadyRunning: false, url: this.url, pid: this.appServerProcess.pid };
      } catch {}
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error("codex app-server did not become ready");
  }

  async withClient(fn) {
    await this.ensureServer();
    const client = await new JsonRpcWsClient(this.url).connect();
    try {
      await client.call("initialize", {
        clientInfo: { name: "carsinos-codex-bridge", version: "0.1.0" },
        capabilities: { suppressNotifications: [] },
      });
      return await fn(client);
    } finally {
      client.close();
    }
  }

  async listThreads(limit = 10) {
    const response = await this.withClient((client) =>
      client.call("thread/list", { limit: Math.max(1, Math.min(Number(limit) || 10, 50)), archived: false }, 20000)
    );
    return compactThreadListResponse(response);
  }

  async readThread(threadId, includeTurns = true) {
    const response = await this.withClient((client) =>
      client.call("thread/read", { threadId: String(threadId), includeTurns: Boolean(includeTurns) }, 20000)
    );
    return compactThreadReadResponse(response);
  }

  resolveCwd(cwd) {
    const resolved = assertAllowedRoot(cwd || process.cwd(), this.allowedRoots);
    if (!fs.existsSync(resolved) || !fs.statSync(resolved).isDirectory()) {
      throw new Error(`cwd does not exist or is not a directory: ${resolved}`);
    }
    return resolved;
  }

  async startThread(input = {}) {
    const cwd = this.resolveCwd(input.cwd);
    const response = await this.withClient((client) =>
      client.call("thread/start", {
        model: input.model || process.env.CODEX_BRIDGE_MODEL || "gpt-5.4-mini",
        modelProvider: input.modelProvider || null,
        cwd,
        approvalPolicy: input.approvalPolicy || "never",
        sandbox: input.sandbox || "read-only",
        baseInstructions: input.baseInstructions || null,
        developerInstructions: input.developerInstructions || null,
        ephemeral: Boolean(input.ephemeral),
        experimentalRawEvents: false,
        persistExtendedHistory: false,
      }, 30000)
    );
    return redact(response);
  }

  async runTurn(input = {}) {
    const threadId = String(input.threadId || "").trim();
    const text = String(input.text || "").trim();
    if (!threadId) throw new Error("threadId is required");
    if (!text) throw new Error("text is required");
    if (text.length > 20000) throw new Error("text is too long");
    return await this.withClient((client) => this.runTurnOnClient(client, { ...input, threadId, text }));
  }

  async startThreadAndRun(input = {}) {
    const text = String(input.text || input.prompt || "").trim();
    if (!text) throw new Error("text is required");
    return await this.withClient(async (client) => {
      const cwd = this.resolveCwd(input.cwd);
      const thread = await client.call("thread/start", {
        model: input.model || process.env.CODEX_BRIDGE_MODEL || "gpt-5.4-mini",
        modelProvider: input.modelProvider || null,
        cwd,
        approvalPolicy: input.approvalPolicy || "never",
        sandbox: input.sandbox || "read-only",
        baseInstructions: input.baseInstructions || null,
        developerInstructions: input.developerInstructions || null,
        ephemeral: Boolean(input.ephemeral),
        experimentalRawEvents: false,
        persistExtendedHistory: false,
      }, 30000);
      const threadId = thread?.result?.thread?.id;
      if (!threadId) return redact({ thread });
      const turn = await this.runTurnOnClient(client, { ...input, threadId, text });
      return redact({ thread, turn });
    });
  }

  async runTurnOnClient(client, input) {
    const started = await client.call("turn/start", {
      threadId: input.threadId,
      input: [{ type: "text", text: input.text, text_elements: [] }],
      model: input.model || null,
      effort: input.effort || null,
    }, 30000);
    const turnId = started?.result?.turn?.id;
    if (started.error) return redact({ started });
    const completed = await client.waitFor((msg) =>
      msg.method === "turn/completed" &&
      msg.params?.threadId === input.threadId &&
      (!turnId || msg.params?.turn?.id === turnId),
      Math.max(30000, Math.min(Number(input.timeoutMs) || 180000, 600000))
    );
    const deltas = client.notifications
      .filter((msg) =>
        msg.method === "item/agentMessage/delta" &&
        msg.params?.threadId === input.threadId &&
        (!turnId || msg.params?.turnId === turnId)
      )
      .map((msg) => msg.params?.delta || "")
      .join("");
    let text = deltas;
    let threadRead = null;
    if (!text) {
      threadRead = await client.call("thread/read", { threadId: input.threadId, includeTurns: true }, 20000);
      text = extractLatestAgentText(threadRead?.result?.thread);
    }
    return redact({
      started,
      completed,
      text: truncateText(text, Number(input.maxChars) || 4000),
      thread: threadRead ? compactThreadReadResponse(threadRead) : undefined,
    });
  }
}

module.exports = {
  CodexAppBridge,
  JsonRpcWsClient,
  compactThread,
  compactThreadListResponse,
  compactThreadReadResponse,
  extractLatestAgentText,
  listCodexProcesses,
  parseProcessRows,
  readSessionIndex,
};
