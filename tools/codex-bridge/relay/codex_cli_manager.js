const fs = require("node:fs");
const path = require("node:path");
const { spawn, spawnSync } = require("node:child_process");
const {
  assertAllowedRoot,
  ensureDir,
  redact,
  safeId,
  safeJoin,
  scrubLogText,
  splitPathList,
  tailFile,
} = require("./safe.js");

const DEFAULT_MODEL = process.env.CODEX_BRIDGE_MODEL || "gpt-5.4-mini";
const DEFAULT_ALLOWED_ROOTS = [
  "Z:\\carsinos-codex-work",
  "\\\\192.168.1.83\\Documents\\openclaw replacement\\carsinos",
];

function nowIso() {
  return new Date().toISOString();
}

function parseJsonlTail(text, limit = 20) {
  return text
    .split(/\r?\n/)
    .filter(Boolean)
    .slice(-limit)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return { raw: line };
      }
    });
}

function isPidRunning(pid) {
  const value = Number(pid);
  if (!Number.isFinite(value) || value <= 0) return false;
  try {
    process.kill(value, 0);
    return true;
  } catch {
    return false;
  }
}

class CodexCliManager {
  constructor(options = {}) {
    this.root = path.resolve(options.root || path.join(__dirname, "..", "runtime"));
    this.sessionsRoot = ensureDir(path.join(this.root, "codex-cli"));
    this.registryPath = path.join(this.sessionsRoot, "sessions.json");
    this.codexBin = options.codexBin || process.env.CODEX_BRIDGE_CODEX_BIN || "codex.exe";
    this.codexArgsPrefix = options.codexArgsPrefix || [];
    this.defaultModel = options.defaultModel || DEFAULT_MODEL;
    this.allowedRoots =
      options.allowedRoots ||
      splitPathList(process.env.CODEX_BRIDGE_ALLOWED_ROOTS).concat(DEFAULT_ALLOWED_ROOTS);
    this.processes = new Map();
  }

  loadRegistry() {
    if (!fs.existsSync(this.registryPath)) return { sessions: [] };
    try {
      const parsed = JSON.parse(fs.readFileSync(this.registryPath, "utf8"));
      if (Array.isArray(parsed.sessions)) return parsed;
    } catch {}
    return { sessions: [] };
  }

  saveRegistry(registry) {
    ensureDir(path.dirname(this.registryPath));
    fs.writeFileSync(this.registryPath, JSON.stringify(registry, null, 2) + "\n", "utf8");
  }

  upsertSession(session) {
    const registry = this.loadRegistry();
    const index = registry.sessions.findIndex((item) => item.sessionId === session.sessionId);
    if (index >= 0) {
      registry.sessions[index] = { ...registry.sessions[index], ...session };
    } else {
      registry.sessions.push(session);
    }
    registry.sessions.sort((left, right) => String(right.updatedAt).localeCompare(String(left.updatedAt)));
    this.saveRegistry(registry);
    return session;
  }

  resolveCwd(cwd) {
    const raw = cwd || process.cwd();
    const resolved = assertAllowedRoot(raw, this.allowedRoots);
    if (!fs.existsSync(resolved) || !fs.statSync(resolved).isDirectory()) {
      throw new Error(`cwd does not exist or is not a directory: ${resolved}`);
    }
    return resolved;
  }

  sessionDir(sessionId) {
    return ensureDir(safeJoin(this.sessionsRoot, safeId(sessionId, "sessionId")));
  }

  startExec(input = {}) {
    const sessionId = safeId(input.sessionId || `codex-${Date.now()}`, "sessionId");
    const prompt = String(input.prompt || "").trim();
    if (!prompt) throw new Error("prompt is required");
    if (prompt.length > 20000) throw new Error("prompt is too long");
    const cwd = this.resolveCwd(input.cwd);
    const model = String(input.model || this.defaultModel).trim();
    const dir = this.sessionDir(sessionId);
    const stdoutPath = path.join(dir, "stdout.jsonl");
    const stderrPath = path.join(dir, "stderr.log");
    const finalPath = path.join(dir, "final.md");
    const metaPath = path.join(dir, "meta.json");
    const args = [
      "exec",
      "--json",
      "--color",
      "never",
      "--skip-git-repo-check",
      "--sandbox",
      input.sandbox || "workspace-write",
      "--model",
      model,
      "--cd",
      cwd,
      "--output-last-message",
      finalPath,
      "-",
    ];
    const env = { ...process.env };
    if (input.codexHome) env.CODEX_HOME = path.resolve(String(input.codexHome));
    const stdout = fs.createWriteStream(stdoutPath, { flags: "a" });
    const stderr = fs.createWriteStream(stderrPath, { flags: "a" });
    const child = spawn(this.codexBin, [...this.codexArgsPrefix, ...args], {
      cwd,
      env,
      windowsHide: true,
      stdio: ["pipe", "pipe", "pipe"],
    });
    const startedAt = nowIso();
    const base = {
      sessionId,
      kind: "exec",
      status: "running",
      pid: child.pid,
      cwd,
      model,
      startedAt,
      updatedAt: startedAt,
      stdoutPath,
      stderrPath,
      finalPath,
      metaPath,
    };
    let settled = false;
    const finish = (patch) => {
      if (settled) return;
      settled = true;
      const endedAt = nowIso();
      const next = {
        sessionId,
        endedAt,
        updatedAt: endedAt,
        ...patch,
      };
      this.upsertSession(next);
      let currentMeta = base;
      if (fs.existsSync(metaPath)) {
        try {
          currentMeta = JSON.parse(fs.readFileSync(metaPath, "utf8"));
        } catch (error) {
          console.warn(`codex bridge ignored unreadable metadata for ${sessionId}: ${error.message}`);
        }
      }
      fs.writeFileSync(
        metaPath,
        JSON.stringify(redact({ ...currentMeta, ...next }), null, 2) + "\n",
        "utf8"
      );
      this.processes.delete(sessionId);
      if (!stdout.writableEnded) stdout.end();
      if (!stderr.writableEnded) stderr.end();
    };
    child.on("error", (error) => {
      finish({
        status: "failed",
        exitCode: null,
        signal: null,
        error: String(error?.message || error),
      });
    });
    child.stdin.on("error", () => {});
    child.stdout.pipe(stdout);
    child.stderr.pipe(stderr);
    try {
      child.stdin.end(prompt);
    } catch (error) {
      finish({
        status: "failed",
        exitCode: null,
        signal: null,
        error: String(error?.message || error),
      });
    }
    fs.writeFileSync(metaPath, JSON.stringify(redact({ ...base, args }), null, 2) + "\n", "utf8");
    this.upsertSession(base);
    this.processes.set(sessionId, child);
    child.on("exit", (code, signal) => {
      finish({
        status: code === 0 ? "succeeded" : "failed",
        exitCode: code,
        signal,
      });
    });
    return this.readSession(sessionId, 12000);
  }

  startInteractiveWindow(input = {}) {
    const sessionId = safeId(input.sessionId || `codex-window-${Date.now()}`, "sessionId");
    const cwd = this.resolveCwd(input.cwd);
    const model = String(input.model || this.defaultModel).trim();
    const prompt = String(input.prompt || "").trim();
    const dir = this.sessionDir(sessionId);
    const transcriptPath = path.join(dir, "transcript.txt");
    const launcherPath = path.join(dir, "launch.ps1");
    const pidPath = path.join(dir, "window.pid");
    const promptPath = path.join(dir, "prompt.txt");
    const stdoutPath = path.join(dir, "stdout.jsonl");
    const stderrPath = path.join(dir, "stderr.log");
    const finalPath = path.join(dir, "final.md");
    if (prompt) fs.writeFileSync(promptPath, prompt, "utf8");
    const escapedCwd = cwd.replace(/'/g, "''");
    const escapedModel = model.replace(/'/g, "''");
    const escapedTranscript = transcriptPath.replace(/'/g, "''");
    const escapedPrompt = promptPath.replace(/'/g, "''");
    const escapedStdout = stdoutPath.replace(/'/g, "''");
    const escapedStderr = stderrPath.replace(/'/g, "''");
    const escapedFinal = finalPath.replace(/'/g, "''");
    const body = prompt
      ? [
          `$prompt = Get-Content -Raw -LiteralPath '${escapedPrompt}'`,
          `$prompt | codex exec --json --color never --skip-git-repo-check --sandbox '${String(input.sandbox || "workspace-write").replace(/'/g, "''")}' --model '${escapedModel}' --cd '${escapedCwd}' --output-last-message '${escapedFinal}' - 2> '${escapedStderr}' | Tee-Object -FilePath '${escapedStdout}' -Append`,
          "Write-Host ''",
          "Write-Host 'CarsinOS Codex CLI window run finished. Final response:'",
          `if (Test-Path -LiteralPath '${escapedFinal}') { Get-Content -LiteralPath '${escapedFinal}' }`,
        ]
      : [
          `codex --no-alt-screen --model '${escapedModel}' --cd '${escapedCwd}'`,
        ];
    const script = [
      "$ErrorActionPreference = 'Continue'",
      `[Console]::Title = 'CarsinOS Codex CLI - ${sessionId}'`,
      `Set-Location -LiteralPath '${escapedCwd}'`,
      `Start-Transcript -Path '${escapedTranscript}' -Force | Out-Null`,
      ...body,
      "Stop-Transcript | Out-Null",
    ].join("\r\n");
    fs.writeFileSync(launcherPath, script, "utf8");
    const startScript = [
      `$p = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoExit','-ExecutionPolicy','Bypass','-File','${launcherPath.replace(/'/g, "''")}') -WorkingDirectory '${escapedCwd}' -PassThru`,
      `$p.Id | Set-Content -LiteralPath '${pidPath.replace(/'/g, "''")}'`,
    ].join("; ");
    const start = spawnSync("powershell.exe", [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-Command",
      startScript,
    ], {
      cwd,
      windowsHide: true,
      encoding: "utf8",
    });
    if (start.error || start.status !== 0) {
      throw new Error(`failed to launch Codex CLI window: ${start.error?.message || start.stderr || start.status}`);
    }
    const childPid = fs.existsSync(pidPath) ? Number(fs.readFileSync(pidPath, "utf8").trim()) || null : null;
    const startedAt = nowIso();
    const session = {
      sessionId,
      kind: "interactive_window",
      status: "running",
      pid: childPid,
      cwd,
      model,
      startedAt,
      updatedAt: startedAt,
      transcriptPath,
      launcherPath,
      pidPath,
      stdoutPath,
      stderrPath,
      finalPath,
    };
    this.upsertSession(session);
    return this.readSession(sessionId, 12000);
  }

  listSessions() {
    const registry = this.loadRegistry();
    return registry.sessions.map((session) => this.readSession(session.sessionId, 4000));
  }

  readSession(sessionId, maxBytes = 65536) {
    const id = safeId(sessionId, "sessionId");
    const registry = this.loadRegistry();
    const session = registry.sessions.find((item) => item.sessionId === id);
    if (!session) throw new Error("session not found");
    const child = this.processes.get(id);
    const status = child && !child.killed
      ? "running"
      : session.kind === "interactive_window"
        ? isPidRunning(session.pid) ? "running" : "stopped"
        : session.status;
    const stdoutTail = session.stdoutPath ? tailFile(session.stdoutPath, maxBytes) : "";
    const stderrTail = session.stderrPath ? tailFile(session.stderrPath, Math.min(maxBytes, 24000)) : "";
    const finalText = session.finalPath ? tailFile(session.finalPath, maxBytes) : "";
    const transcriptTail = session.transcriptPath ? tailFile(session.transcriptPath, maxBytes) : "";
    return redact({
      ...session,
      status,
      stdoutEvents: parseJsonlTail(stdoutTail),
      stderrTail: scrubLogText(stderrTail),
      finalText,
      transcriptTail: scrubLogText(transcriptTail),
    });
  }
}

module.exports = { CodexCliManager, parseJsonlTail };
