const http = require("node:http");
const path = require("node:path");
const { CodexCliManager } = require("./codex_cli_manager.js");
const { CodexAppBridge } = require("./codex_app_observer.js");
const { ClaudeCodeManager } = require("./claude_code_manager.js");

const PORT = Number(process.env.CODEX_BRIDGE_PORT || 17889);
const ROOT = path.resolve(__dirname, "..");
const RUNTIME_ROOT = path.resolve(process.env.CODEX_BRIDGE_RUNTIME_ROOT || path.join(ROOT, "runtime"));

const cli = new CodexCliManager({ root: RUNTIME_ROOT });
const app = new CodexAppBridge({ logsDir: path.join(RUNTIME_ROOT, "codex-app") });
const claude = new ClaudeCodeManager({ root: RUNTIME_ROOT });

function json(res, status, body) {
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "access-control-allow-origin": "*",
    "access-control-allow-methods": "GET,POST,OPTIONS",
    "access-control-allow-headers": "content-type",
    "cache-control": "no-store",
  });
  res.end(JSON.stringify(body, null, 2));
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let data = "";
    req.setEncoding("utf8");
    req.on("data", (chunk) => {
      data += chunk;
      if (data.length > 1024 * 1024) {
        reject(new Error("request too large"));
        req.destroy();
      }
    });
    req.on("end", () => {
      if (!data.trim()) return resolve({});
      try {
        resolve(JSON.parse(data));
      } catch (err) {
        reject(err);
      }
    });
    req.on("error", reject);
  });
}

async function route(req, res) {
  if (req.method === "OPTIONS") return json(res, 204, {});
  const url = new URL(req.url, `http://${req.headers.host || "127.0.0.1"}`);
  try {
    if (req.method === "GET" && url.pathname === "/status") {
      return json(res, 200, {
        ok: true,
        service: "carsinos-codex-bridge",
        root: ROOT,
        cliSessions: cli.listSessions(),
        claudeCodeSessions: claude.listSessions(),
        codexApp: app.status(),
      });
    }
    if (req.method === "GET" && url.pathname === "/codex-cli/sessions") {
      return json(res, 200, { ok: true, items: cli.listSessions() });
    }
    if (req.method === "GET" && url.pathname.startsWith("/codex-cli/sessions/")) {
      const sessionId = decodeURIComponent(url.pathname.split("/").pop());
      return json(res, 200, { ok: true, session: cli.readSession(sessionId, Number(url.searchParams.get("maxBytes")) || 65536) });
    }
    if (req.method === "POST" && url.pathname === "/codex-cli/exec") {
      const body = await readBody(req);
      return json(res, 202, { ok: true, session: cli.startExec(body) });
    }
    if (req.method === "POST" && url.pathname === "/codex-cli/window") {
      const body = await readBody(req);
      return json(res, 202, { ok: true, session: cli.startInteractiveWindow(body) });
    }
    if (req.method === "GET" && url.pathname === "/claude-code/sessions") {
      return json(res, 200, { ok: true, items: claude.listSessions() });
    }
    if (req.method === "GET" && url.pathname.startsWith("/claude-code/sessions/")) {
      const sessionId = decodeURIComponent(url.pathname.split("/").pop());
      return json(res, 200, { ok: true, session: claude.readSession(sessionId, Number(url.searchParams.get("maxBytes")) || 65536) });
    }
    if (req.method === "POST" && url.pathname === "/claude-code/exec") {
      const body = await readBody(req);
      return json(res, 202, { ok: true, session: claude.startExec(body) });
    }
    if (req.method === "POST" && url.pathname === "/claude-code/window") {
      const body = await readBody(req);
      return json(res, 202, { ok: true, session: claude.startInteractiveWindow(body) });
    }
    if (req.method === "GET" && url.pathname === "/codex-app/status") {
      return json(res, 200, { ok: true, status: app.status() });
    }
    if (req.method === "GET" && url.pathname === "/codex-app/threads") {
      return json(res, 200, { ok: true, response: await app.listThreads(Number(url.searchParams.get("limit")) || 10) });
    }
    if (req.method === "GET" && url.pathname.startsWith("/codex-app/threads/")) {
      const threadId = decodeURIComponent(url.pathname.split("/").pop());
      return json(res, 200, { ok: true, response: await app.readThread(threadId, url.searchParams.get("includeTurns") !== "false") });
    }
    if (req.method === "POST" && url.pathname === "/codex-app/thread") {
      const body = await readBody(req);
      return json(res, 202, { ok: true, response: await app.startThread(body) });
    }
    if (req.method === "POST" && url.pathname === "/codex-app/turn") {
      const body = await readBody(req);
      return json(res, 202, { ok: true, response: await app.runTurn(body) });
    }
    if (req.method === "POST" && url.pathname === "/codex-app/run") {
      const body = await readBody(req);
      return json(res, 202, { ok: true, response: await app.startThreadAndRun(body) });
    }
    return json(res, 404, { ok: false, error: "not found" });
  } catch (err) {
    return json(res, 500, { ok: false, error: err.message || String(err) });
  }
}

if (require.main === module) {
  http.createServer(route).listen(PORT, "127.0.0.1", () => {
    console.log(`carsinos codex bridge listening on http://127.0.0.1:${PORT}`);
  });
}

module.exports = { route };
