#!/usr/bin/env node

import { spawn } from "node:child_process";
import { appendFileSync, createWriteStream, mkdirSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

const suite = process.argv[2] ?? "unknown";
if (suite !== "tauri-smoke") {
  console.error(`[mission-control] unsupported suite '${suite}'`);
  process.exit(2);
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");
const appDir = path.join(repoRoot, "apps", "mission-control");

const gatewayPort = 19_789;
const appPort = 1_420;
const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
const appUrl = `http://127.0.0.1:${appPort}/?e2e=1`;
const token = "stub-token-001";
const modelId = "qwen3.5-9b-instruct";

const stamp = new Date().toISOString().replace(/[-:.]/g, "").replace("T", "T").slice(0, 15) + "Z";
const artifactRoot = process.env.MISSION_CONTROL_TAURI_ARTIFACT_ROOT
  ? path.resolve(process.env.MISSION_CONTROL_TAURI_ARTIFACT_ROOT)
  : path.join(
      repoRoot,
      "runtime",
      "quality-gate",
      "artifacts",
      "tauri-smoke"
    );
const artifactDir = path.join(artifactRoot, stamp);
const screenshotDir = path.join(artifactDir, "screenshots");
const videoDir = path.join(artifactDir, "video");
const tracePath = path.join(artifactDir, "trace.zip");
const manifestPath = path.join(artifactDir, "manifest.json");
const harnessLogPath = path.join(artifactDir, "harness.log");
const mockLogPath = path.join(artifactDir, "mock-gateway.log");
const tauriLogPath = path.join(artifactDir, "tauri-dev.log");
const isWindows = process.platform === "win32";

process.stdout.on("error", () => {
  // Desktop smoke can be launched from wrappers with fragile inherited handles.
});
process.stderr.on("error", () => {
  // Keep harness logging best-effort; artifact files carry the durable record.
});

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function waitFor(conditionFn, timeoutMs, errorMessage) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (await conditionFn()) {
      return;
    }
    await sleep(300);
  }
  throw new Error(errorMessage);
}

async function isHttpReady(url, headers = {}) {
  try {
    const response = await fetch(url, { headers });
    return response.ok;
  } catch {
    return false;
  }
}

async function recordHarnessEvent(message) {
  try {
    mkdirSync(artifactDir, { recursive: true });
    appendFileSync(harnessLogPath, `${new Date().toISOString()} ${message}\n`, "utf8");
  } catch {
    // Artifact logging is diagnostic only.
  }
}

function createLineScanner(onText) {
  let remainder = "";
  return (chunk) => {
    const text = `${remainder}${String(chunk)}`;
    const lines = text.split(/\r?\n/);
    remainder = lines.pop() ?? "";
    for (const line of lines) {
      onText(line);
    }
  };
}

function commandQuote(value) {
  const text = String(value);
  if (/^[A-Za-z0-9_./:=+-]+$/.test(text)) {
    return text;
  }
  return `"${text.replace(/"/g, '\\"')}"`;
}

function cmdQuote(value) {
  return `"${String(value).replace(/"/g, '""')}"`;
}

function safeLauncherCwd() {
  return process.env.TMP || process.env.TEMP || os.tmpdir();
}

function prepareSpawnCommand(cmd, args, cwd) {
  if (!isWindows) {
    return { cmd, args, cwd };
  }

  if (!cwd.startsWith("\\\\")) {
    if (cmd === "npm") {
      return {
        cmd: process.env.ComSpec || "cmd.exe",
        args: ["/d", "/c", [cmd, ...args].join(" ")],
        cwd,
      };
    }
    const winCmd = cmd === "node" ? process.execPath : cmd;
    return { cmd: winCmd, args, cwd };
  }

  const commandLine = [cmd, ...args].map(cmdQuote).join(" ");
  const cwdCommand = `pushd ${cmdQuote(cwd)}`;
  return {
    cmd: process.env.ComSpec || "cmd.exe",
    args: ["/d", "/c", `${cwdCommand} && ${commandLine}`],
    cwd: safeLauncherCwd(),
  };
}

function createSafeLogStream(filePath) {
  const stream = createWriteStream(filePath, { flags: "a" });
  const errors = [];
  stream.on("error", (error) => {
    errors.push(error);
  });
  return {
    errors,
    write(chunk) {
      if (stream.destroyed || errors.length > 0) {
        return;
      }
      try {
        stream.write(chunk);
      } catch (error) {
        errors.push(error);
      }
    },
    end() {
      return new Promise((resolve) => {
        if (stream.destroyed) {
          resolve();
          return;
        }
        try {
          stream.end(resolve);
        } catch {
          resolve();
        }
      });
    },
  };
}

function spawnLogged(name, cmd, args, cwd, extraEnv = {}) {
  const prepared = prepareSpawnCommand(cmd, args, cwd);
  const child = spawn(prepared.cmd, prepared.args, {
    cwd: prepared.cwd,
    env: {
      ...process.env,
      ...extraEnv,
    },
    stdio: ["ignore", "pipe", "pipe"],
    detached: !isWindows,
    windowsHide: true,
  });
  if (!isWindows) {
    child.unref();
  }
  const proc = {
    name,
    child,
    command: [cmd, ...args].map(commandQuote).join(" "),
    exitStatus: null,
  };
  child.on("exit", (code, signal) => {
    proc.exitStatus = { code, signal };
  });
  return proc;
}

async function terminate(proc) {
  if (!proc || !proc.child || proc.child.exitCode !== null) {
    return;
  }
  if (isWindows) {
    await new Promise((resolve) => {
      const killer = spawn("taskkill", ["/PID", String(proc.child.pid), "/T", "/F"], {
        cwd: safeLauncherCwd(),
        stdio: "ignore",
        windowsHide: true,
      });
      killer.on("error", resolve);
      killer.on("exit", resolve);
    });
    return;
  }
  try {
    process.kill(-proc.child.pid, "SIGTERM");
  } catch {
    return;
  }
  const started = Date.now();
  while (Date.now() - started < 5_000) {
    if (proc.child.exitCode !== null) {
      return;
    }
    await sleep(100);
  }
  try {
    process.kill(-proc.child.pid, "SIGKILL");
  } catch {
    // no-op
  }
}

async function run() {
  const startedAtUtc = new Date().toISOString();
  await recordHarnessEvent("starting tauri smoke harness");
  await mkdir(screenshotDir, { recursive: true });
  await mkdir(videoDir, { recursive: true });

  const mockLog = createSafeLogStream(mockLogPath);
  const tauriLog = createSafeLogStream(tauriLogPath);

  const mockProc = spawnLogged(
    "mock-gateway",
    "node",
    ["./e2e/mockGateway.mjs", "--port", String(gatewayPort)],
    appDir
  );

  mockProc.child.on("error", (error) => {
    mockLog.write(`[spawn-error] ${String(error)}\n`);
  });
  mockProc.child.on("exit", (code, signal) => {
    mockLog.write(`[exit] code=${code ?? "null"} signal=${signal ?? "null"}\n`);
  });

  const tauriProc = spawnLogged(
    "tauri-dev",
    "npm",
    ["run", "tauri:dev", "--", "--no-watch"],
    appDir,
    {
      CI: "1",
    }
  );
  tauriProc.child.on("error", (error) => {
    tauriLog.write(`[spawn-error] ${String(error)}\n`);
  });
  tauriProc.child.on("exit", (code, signal) => {
    tauriLog.write(`[exit] code=${code ?? "null"} signal=${signal ?? "null"}\n`);
  });
  await recordHarnessEvent(`spawned ${mockProc.name}: ${mockProc.command}`);
  await recordHarnessEvent(`spawned ${tauriProc.name}: ${tauriProc.command}`);

  let tauriRuntimeSignal = false;
  const tauriSignalPattern =
    /Running DevCommand|Finished `dev` profile|Running `target\/|CarsinOS Mission Control|beforeDevCommand/i;

  mockProc.child.stdout?.on("data", (chunk) => {
    mockLog.write(chunk);
  });
  mockProc.child.stderr?.on("data", (chunk) => {
    mockLog.write(chunk);
  });

  const tauriScanner = createLineScanner((line) => {
    tauriLog.write(`${line}\n`);
    if (!tauriRuntimeSignal && tauriSignalPattern.test(line)) {
      tauriRuntimeSignal = true;
    }
  });
  tauriProc.child.stdout?.on("data", tauriScanner);
  tauriProc.child.stderr?.on("data", tauriScanner);

  const cleanup = async () => {
    await terminate(tauriProc);
    await terminate(mockProc);
    await Promise.all([mockLog.end(), tauriLog.end()]);
  };

  let browser;
  let context;
  let page;
  const steps = [];

  try {
    const playwrightModule = await import(
      pathToFileURL(path.join(appDir, "node_modules", "@playwright", "test", "index.mjs")).href
    );
    const { chromium } = playwrightModule;

    await waitFor(
      async () => {
        if (mockProc.exitStatus) {
          throw new Error(
            `mock gateway exited before ready (code=${mockProc.exitStatus.code ?? "null"} signal=${
              mockProc.exitStatus.signal ?? "null"
            })`
          );
        }
        return isHttpReady(`${gatewayUrl}/api/v1/health`, {
          Authorization: `Bearer ${token}`,
        });
      },
      30_000,
      "mock gateway did not become ready"
    );

    await waitFor(
      async () => {
        if (tauriProc.exitStatus) {
          throw new Error(
            `tauri dev exited before ready (code=${tauriProc.exitStatus.code ?? "null"} signal=${
              tauriProc.exitStatus.signal ?? "null"
            })`
          );
        }
        const appReady = await isHttpReady(`http://127.0.0.1:${appPort}/`);
        return appReady && tauriRuntimeSignal;
      },
      240_000,
      "tauri dev runtime did not become ready"
    );

    browser = await chromium.launch({
      headless: true,
    });
    context = await browser.newContext({
      viewport: { width: 1680, height: 1024 },
      recordVideo: {
        dir: videoDir,
        size: { width: 1280, height: 720 },
      },
    });
    await context.tracing.start({
      screenshots: true,
      snapshots: true,
    });
    page = await context.newPage();
    page.on("console", (message) => {
      recordHarnessEvent(`browser console ${message.type()}: ${message.text()}`);
    });
    page.on("pageerror", (error) => {
      recordHarnessEvent(`browser pageerror: ${error?.stack ?? String(error)}`);
    });
    await page.addInitScript(() => {
      window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
    });

    let stepNo = 0;
    const shot = async (label) => {
      stepNo += 1;
      const safe = label
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/(^-|-$)/g, "");
      const file = `${String(stepNo).padStart(3, "0")}-${safe}.png`;
      const full = path.join(screenshotDir, file);
      await page.screenshot({ path: full, fullPage: true });
      steps.push({ index: stepNo, label, file: path.relative(artifactDir, full) });
    };

    await page.goto(appUrl);
    await page.getByRole("heading", { name: "Setup Wizard" }).waitFor();
    await shot("wizard-opened");

    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByText("Step 2 of 6").waitFor();
    await shot("wizard-step-2");

    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByText("Step 3 of 6").waitFor();
    await shot("wizard-step-3-connect");

    await page.getByLabel("Gateway URL").fill(gatewayUrl);
    await page.getByLabel("Gateway token").first().fill(token);
    await shot("connect-fields-filled");

    await page.getByRole("button", { name: /Save \+ Connect/ }).click();
    await page.getByText(/Connection status:\s*Connected/).waitFor();
    await shot("gateway-connected");

    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByText("Step 4 of 6").waitFor();
    await page.getByLabel("Agent ID").fill("assistant-main");
    await page.getByLabel("Agent name").fill("Assistant");
    const localConnectorRadio = page.getByRole("radio", { name: /Local connector/i });
    if ((await localConnectorRadio.count()) > 0) {
      await localConnectorRadio.first().check({ force: true });
    }
    const manualModelInput = page.getByPlaceholder("Or paste assistant model ID manually");
    if ((await manualModelInput.count()) > 0 && (await manualModelInput.first().isVisible())) {
      await manualModelInput.first().fill(modelId);
    } else {
      const assistantModelSelect = page.getByLabel(/Assistant model/i);
      if ((await assistantModelSelect.count()) > 0) {
        await assistantModelSelect.first().selectOption(modelId).catch(async () => {
          await assistantModelSelect.first().selectOption({ label: modelId });
        });
      }
    }
    await shot("wizard-step-4-agent-provider");

    await page.getByRole("button", { name: /^(Continue|Apply setup \+ Continue)$/ }).click();
    await page.getByText("Step 5 of 6").waitFor();
    await shot("wizard-step-5-review");
    await page.getByRole("button", { name: /^(Finalize|Finish setup)$/ }).click();
    const finalStep = page.getByText("Step 6 of 6");
    if (
      await finalStep
        .waitFor({ timeout: 5_000 })
        .then(() => true)
        .catch(() => false)
    ) {
      await page.getByRole("button", { name: /Go to Boards|Enter Mission Control/ }).click();
    }
    await page.getByText("Investigate gateway health").waitFor();
    await shot("wizard-complete-boards");

    const navTabs = [
      "boards",
      "calendar",
      "focus",
      "events",
      "mail",
      "chatrooms",
      "assistant",
      "team",
      "cockpit",
    ];
    for (const tab of navTabs) {
      const navId = `nav-${tab}`;
      await page.locator(`[data-tour-id="${navId}"]`).click();
      await page.waitForFunction(
        (id) => {
          const node = document.querySelector(`[data-tour-id="${id}"]`);
          return Boolean(node && node.className.includes("mc-nav-item-active"));
        },
        navId
      );
      await shot(`nav-${tab}`);
    }

    await page.locator('[data-tour-id="nav-config"]').click();
    await page.getByRole("heading", { name: "Settings" }).waitFor();
    await shot("settings-opened");

    const video = page.video();
    await context.tracing.stop({ path: tracePath });
    await context.close();
    await browser.close();

    const videoPath = video ? await video.path() : null;

    await writeFile(
      manifestPath,
      JSON.stringify(
        {
          suite: "tauri-smoke",
          started_at_utc: startedAtUtc,
          app_url: appUrl,
          gateway_url: gatewayUrl,
          screenshots_dir: path.relative(repoRoot, screenshotDir),
          trace: path.relative(repoRoot, tracePath),
          video: videoPath ? path.relative(repoRoot, videoPath) : null,
          mock_log: path.relative(repoRoot, mockLogPath),
          tauri_log: path.relative(repoRoot, tauriLogPath),
          steps,
        },
        null,
        2
      )
    );

    console.log(`[mission-control] tauri smoke visual PASS`);
    console.log(`[mission-control] artifacts: ${path.relative(repoRoot, artifactDir)}`);
  } catch (error) {
    if (page) {
      try {
        await page.screenshot({ path: path.join(artifactDir, "failure-page.png"), fullPage: true });
        await writeFile(path.join(artifactDir, "failure-page.html"), await page.content(), "utf8");
      } catch (captureError) {
        await recordHarnessEvent(`failure capture skipped: ${String(captureError)}`);
      }
    }
    await recordHarnessEvent(`FAIL ${error?.stack ?? String(error)}`);
    console.error(`[mission-control] tauri smoke visual FAIL: ${String(error)}`);
    throw error;
  } finally {
    if (context) {
      try {
        await context.close();
      } catch {
        // no-op
      }
    }
    if (browser) {
      try {
        await browser.close();
      } catch {
        // no-op
      }
    }
    await cleanup();
  }
}

run().catch((error) => {
  const details = error?.stack ?? String(error);
  try {
    appendFileSync(
      path.join(safeLauncherCwd(), "mission-control-tauri-smoke-last-error.log"),
      `${new Date().toISOString()} ${details}\n`,
      "utf8"
    );
  } catch {
    // no-op
  }
  try {
    process.stderr.write(`[mission-control] tauri smoke harness error: ${details}\n`);
  } catch {
    // no-op
  }
  process.exit(1);
});
