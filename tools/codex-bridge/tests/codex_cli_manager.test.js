const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { CodexCliManager, parseJsonlTail } = require("../relay/codex_cli_manager.js");
const { scrubLogText } = require("../relay/safe.js");

function tmpRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "codex-bridge-cli-"));
}

test("parseJsonlTail keeps recent JSON events and raw malformed lines", () => {
  const events = parseJsonlTail('{"a":1}\nnot-json\n{"b":2}\n', 2);
  assert.deepEqual(events, [{ raw: "not-json" }, { b: 2 }]);
});

test("scrubLogText redacts external challenge tokens and long blobs", () => {
  const raw = "https://chatgpt.com/backend-api/plugins/featured?__cf_chl_rt_tk=secret-123 cH: '"
    + "x".repeat(180)
    + "' <html><body>challenge</body></html>";
  const scrubbed = scrubLogText(raw);
  assert.match(scrubbed, /__cf_chl_rt_tk=\[REDACTED\]/);
  assert.match(scrubbed, /cH: '\[REDACTED\]'/);
  assert.match(scrubbed, /\[REDACTED_HTML\]/);
  assert.doesNotMatch(scrubbed, /secret-123/);
  assert.doesNotMatch(scrubbed, new RegExp(`x{160}`));
});

test("manager rejects path traversal session ids", () => {
  const root = tmpRoot();
  const manager = new CodexCliManager({ root, allowedRoots: [root] });
  assert.throws(() => manager.sessionDir("..\\bad"), /sessionId must match/);
});

test("manager captures fake codex exec output and final text", async () => {
  const root = tmpRoot();
  const workspace = path.join(root, "workspace");
  fs.mkdirSync(workspace, { recursive: true });
  const fake = path.join(root, "fake-codex.js");
  fs.writeFileSync(fake, "console.log(JSON.stringify({ event: 'started' }));\n", "utf8");
  const manager = new CodexCliManager({
    root,
    codexBin: process.execPath,
    codexArgsPrefix: [fake],
    allowedRoots: [root],
  });
  const started = manager.startExec({ sessionId: "test-run", cwd: workspace, prompt: "hi" });
  assert.equal(started.status, "running");
  await new Promise((resolve) => setTimeout(resolve, 500));
  const read = manager.readSession("test-run");
  assert.ok(["succeeded", "running", "failed"].includes(read.status));
  assert.equal(read.stdoutEvents.at(-1).event, "started");
});

test("manager marks exec sessions failed when spawn emits error", async () => {
  const root = tmpRoot();
  const workspace = path.join(root, "workspace");
  fs.mkdirSync(workspace, { recursive: true });
  const missingBin = path.join(root, "missing-codex-bin");
  const manager = new CodexCliManager({
    root,
    codexBin: missingBin,
    allowedRoots: [root],
  });

  const started = manager.startExec({ sessionId: "spawn-error", cwd: workspace, prompt: "hi" });
  assert.equal(started.status, "running");

  await new Promise((resolve) => setTimeout(resolve, 250));
  const read = manager.readSession("spawn-error");
  assert.equal(read.status, "failed");
  assert.match(read.error, /ENOENT|not found|spawn/i);
});
