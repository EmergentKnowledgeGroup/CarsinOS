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

const DEFAULT_MODEL = process.env.CODEX_BRIDGE_CLAUDE_MODEL || "sonnet";
const DEFAULT_ALLOWED_ROOTS = [
  "Z:\\carsinos-codex-work",
  "\\\\192.168.1.83\\Documents\\openclaw replacement\\carsinos",
];
const ALLOWED_PERMISSION_MODES = new Set(["default", "acceptEdits", "dontAsk", "plan"]);

function nowIso() {
  return new Date().toISOString();
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

function permissionMode(input) {
  const mode = String(input.permission_mode || input.permissionMode || "default").trim();
  if (!ALLOWED_PERMISSION_MODES.has(mode)) {
    throw new Error("unsupported Claude permission_mode");
  }
  return mode;
}

class ClaudeCodeManager {
  constructor(options = {}) {
    this.root = path.resolve(options.root || path.join(__dirname, "..", "runtime"));
    this.sessionsRoot = ensureDir(path.join(this.root, "claude-code"));
    this.registryPath = path.join(this.sessionsRoot, "sessions.json");
    this.claudeBin =
      options.claudeBin ||
      process.env.CODEX_BRIDGE_CLAUDE_BIN ||
      "C:\\Users\\UltariumV3\\.local\\bin\\claude.exe";
    this.claudeArgsPrefix = options.claudeArgsPrefix || [];
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
    const sessionId = safeId(input.sessionId || `claude-${Date.now()}`, "sessionId");
    const prompt = String(input.prompt || "").trim();
    if (!prompt) throw new Error("prompt is required");
    if (prompt.length > 20000) throw new Error("prompt is too long");
    const cwd = this.resolveCwd(input.cwd);
    const model = String(input.model || this.defaultModel).trim();
    const mode = permissionMode(input);
    const dir = this.sessionDir(sessionId);
    const stdoutPath = path.join(dir, "stdout.txt");
    const stderrPath = path.join(dir, "stderr.log");
    const finalPath = path.join(dir, "final.md");
    const metaPath = path.join(dir, "meta.json");
    const args = [
      "--print",
      "--output-format",
      "text",
      "--model",
      model,
      "--permission-mode",
      mode,
      prompt,
    ];
    const child = spawn(this.claudeBin, [...this.claudeArgsPrefix, ...args], {
      cwd,
      env: { ...process.env },
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout = fs.createWriteStream(stdoutPath, { flags: "a" });
    const stderr = fs.createWriteStream(stderrPath, { flags: "a" });
    child.stdout.pipe(stdout);
    child.stderr.pipe(stderr);
    child.stdout.on("data", (chunk) => fs.appendFileSync(finalPath, chunk));

    const startedAt = nowIso();
    const base = {
      sessionId,
      kind: "exec",
      status: "running",
      pid: child.pid,
      cwd,
      model,
      permissionMode: mode,
      startedAt,
      updatedAt: startedAt,
      stdoutPath,
      stderrPath,
      finalPath,
      metaPath,
    };
    fs.writeFileSync(metaPath, JSON.stringify(redact({ ...base, args: ["--print", "--output-format", "text", "--model", model, "--permission-mode", mode, "[prompt]"] }), null, 2) + "\n", "utf8");
    this.upsertSession(base);
    this.processes.set(sessionId, child);
    child.on("exit", (code, signal) => {
      const endedAt = nowIso();
      this.upsertSession({
        sessionId,
        status: code === 0 ? "succeeded" : "failed",
        exitCode: code,
        signal,
        endedAt,
        updatedAt: endedAt,
      });
      this.processes.delete(sessionId);
      stdout.end();
      stderr.end();
    });
    return this.readSession(sessionId, 12000);
  }

  startInteractiveWindow(input = {}) {
    const sessionId = safeId(input.sessionId || `claude-window-${Date.now()}`, "sessionId");
    const cwd = this.resolveCwd(input.cwd);
    const model = String(input.model || this.defaultModel).trim();
    const prompt = String(input.prompt || "").trim();
    const mode = permissionMode(input);
    const dir = this.sessionDir(sessionId);
    const transcriptPath = path.join(dir, "transcript.txt");
    const launcherPath = path.join(dir, "launch.ps1");
    const pidPath = path.join(dir, "window.pid");
    const promptPath = path.join(dir, "prompt.txt");
    if (prompt) fs.writeFileSync(promptPath, prompt, "utf8");
    const escapedCwd = cwd.replace(/'/g, "''");
    const escapedModel = model.replace(/'/g, "''");
    const escapedTranscript = transcriptPath.replace(/'/g, "''");
    const escapedPrompt = promptPath.replace(/'/g, "''");
    const escapedBin = this.claudeBin.replace(/'/g, "''");
    const escapedMode = mode.replace(/'/g, "''");
    const body = prompt
      ? [
          `$prompt = Get-Content -Raw -LiteralPath '${escapedPrompt}'`,
          `& '${escapedBin}' --print --output-format text --model '${escapedModel}' --permission-mode '${escapedMode}' $prompt`,
          "Write-Host ''",
          "Write-Host 'CarsinOS Claude Code window run finished.'",
        ]
      : [
          `& '${escapedBin}' --model '${escapedModel}' --permission-mode '${escapedMode}'`,
        ];
    const script = [
      "$ErrorActionPreference = 'Continue'",
      `[Console]::Title = 'CarsinOS Claude Code - ${sessionId}'`,
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
      throw new Error(`failed to launch Claude Code window: ${start.error?.message || start.stderr || start.status}`);
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
      permissionMode: mode,
      startedAt,
      updatedAt: startedAt,
      transcriptPath,
      launcherPath,
      pidPath,
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
    return redact({
      ...session,
      status,
      stdoutTail: scrubLogText(session.stdoutPath ? tailFile(session.stdoutPath, maxBytes) : ""),
      stderrTail: scrubLogText(session.stderrPath ? tailFile(session.stderrPath, Math.min(maxBytes, 24000)) : ""),
      finalText: session.finalPath ? tailFile(session.finalPath, maxBytes) : "",
      transcriptTail: scrubLogText(session.transcriptPath ? tailFile(session.transcriptPath, maxBytes) : ""),
    });
  }
}

module.exports = { ClaudeCodeManager, permissionMode };
