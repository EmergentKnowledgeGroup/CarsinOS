#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createWriteStream } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
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
const artifactDir = path.join(
  repoRoot,
  "runtime",
  "quality-gate",
  "artifacts",
  "tauri-smoke",
  stamp
);
const screenshotDir = path.join(artifactDir, "screenshots");
const videoDir = path.join(artifactDir, "video");
const tracePath = path.join(artifactDir, "trace.zip");
const manifestPath = path.join(artifactDir, "manifest.json");
const mockLogPath = path.join(artifactDir, "mock-gateway.log");
const tauriLogPath = path.join(artifactDir, "tauri-dev.log");

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

function spawnLogged(name, cmd, args, cwd, extraEnv = {}) {
  const child = spawn(cmd, args, {
    cwd,
    env: {
      ...process.env,
      ...extraEnv,
    },
    stdio: ["ignore", "pipe", "pipe"],
    detached: true,
  });
  child.unref();
  return { name, child };
}

async function terminate(proc) {
  if (!proc || !proc.child || proc.child.exitCode !== null) {
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
  await mkdir(screenshotDir, { recursive: true });
  await mkdir(videoDir, { recursive: true });

  const mockLog = createWriteStream(mockLogPath, { flags: "a" });
  const tauriLog = createWriteStream(tauriLogPath, { flags: "a" });

  const mockProc = spawnLogged(
    "mock-gateway",
    "node",
    ["./e2e/mockGateway.mjs", "--port", String(gatewayPort)],
    appDir
  );

  const tauriProc = spawnLogged(
    "tauri-dev",
    "npm",
    ["run", "tauri:dev", "--", "--no-watch"],
    appDir,
    {
      CI: "1",
    }
  );

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
    await Promise.all([
      new Promise((resolve) => mockLog.end(resolve)),
      new Promise((resolve) => tauriLog.end(resolve)),
    ]);
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
      () =>
        isHttpReady(`${gatewayUrl}/api/v1/health`, {
          Authorization: `Bearer ${token}`,
        }),
      30_000,
      "mock gateway did not become ready"
    );

    await waitFor(
      async () => {
        if (!tauriProc.child || tauriProc.child.exitCode !== null) {
          return false;
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
    await page.getByRole("radio", { name: "Local connector" }).check();
    await page.getByPlaceholder("Or paste assistant model ID manually").fill(modelId);
    await shot("wizard-step-4-agent-provider");

    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByText("Step 5 of 6").waitFor();
    await shot("wizard-step-5-review");
    await page.getByRole("button", { name: "Finalize" }).click();
    await page.getByText("Step 6 of 6").waitFor();
    await page.getByRole("button", { name: "Go to Boards" }).click();
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

run().catch(() => {
  process.exit(1);
});
