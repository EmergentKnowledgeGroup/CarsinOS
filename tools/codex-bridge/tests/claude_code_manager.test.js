const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { ClaudeCodeManager, permissionMode } = require("../relay/claude_code_manager.js");

function tempRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "carsinos-claude-code-test-"));
}

test("rejects unsafe Claude permission modes", () => {
  assert.equal(permissionMode({ permission_mode: "plan" }), "plan");
  assert.throws(
    () => permissionMode({ permission_mode: "bypassPermissions" }),
    /unsupported Claude permission_mode/
  );
});

test("starts a Claude Code exec session through a bounded command", async () => {
  const root = tempRoot();
  const cwd = path.join(root, "workspace");
  fs.mkdirSync(cwd, { recursive: true });
  const fakeCli = path.join(root, "fake-claude.js");
  fs.writeFileSync(
    fakeCli,
    [
      "console.log('CLAUDE_CODE_BRIDGE_OK');",
      "console.error(`args:${process.argv.slice(2).join(' ')}`);",
    ].join("\n"),
    "utf8"
  );
  const manager = new ClaudeCodeManager({
    root,
    claudeBin: process.execPath,
    claudeArgsPrefix: [fakeCli],
    allowedRoots: [root],
    defaultModel: "sonnet",
  });

  const session = manager.startExec({
    sessionId: "claude-test",
    cwd,
    prompt: "Say ok.",
    permission_mode: "plan",
  });
  assert.equal(session.sessionId, "claude-test");
  assert.equal(session.status, "running");

  await new Promise((resolve) => setTimeout(resolve, 250));
  const read = manager.readSession("claude-test");
  assert.match(read.finalText, /CLAUDE_CODE_BRIDGE_OK/);
  assert.match(read.stderrTail, /--permission-mode plan/);
});

test("marks Claude Code exec sessions failed when spawn emits error", async () => {
  const root = tempRoot();
  const cwd = path.join(root, "workspace");
  fs.mkdirSync(cwd, { recursive: true });
  const missingBin = path.join(root, "missing-claude-bin");
  const manager = new ClaudeCodeManager({
    root,
    claudeBin: missingBin,
    allowedRoots: [root],
    defaultModel: "sonnet",
  });

  const session = manager.startExec({
    sessionId: "claude-spawn-error",
    cwd,
    prompt: "Say ok.",
    permission_mode: "plan",
  });
  assert.equal(session.status, "running");

  await new Promise((resolve) => setTimeout(resolve, 250));
  const read = manager.readSession("claude-spawn-error");
  assert.equal(read.status, "failed");
  assert.match(read.error, /ENOENT|not found|spawn/i);
});
