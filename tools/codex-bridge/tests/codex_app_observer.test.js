const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const {
  compactThread,
  JsonRpcWsClient,
  extractLatestAgentText,
  parseProcessRows,
  readSessionIndex,
} = require("../relay/codex_app_observer.js");

test("session index parser exposes safe thread summaries only", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "codex-bridge-app-"));
  fs.writeFileSync(
    path.join(root, "session_index.jsonl"),
    [
      JSON.stringify({ id: "thread-a", thread_name: "Alpha", updated_at: "2026-05-17T00:00:00Z", secret: "nope" }),
      JSON.stringify({ id: "thread-b", thread_name: "Beta", updated_at: "2026-05-17T00:01:00Z" }),
    ].join("\n"),
    "utf8"
  );
  const result = readSessionIndex(root, 1);
  assert.equal(result.items.length, 1);
  assert.deepEqual(result.items[0], {
    id: "thread-b",
    threadName: "Beta",
    updatedAt: "2026-05-17T00:01:00Z",
  });
});

test("process parser filters codex app-server rows and redacts token-looking values", () => {
  const rows = parseProcessRows(JSON.stringify([
    { ProcessId: 1, ParentProcessId: 0, Name: "notepad.exe", CommandLine: "notepad" },
    { ProcessId: 2, ParentProcessId: 1, Name: "codex.exe", CommandLine: "codex.exe app-server --token abc123" },
  ]));
  assert.equal(rows.length, 1);
  assert.equal(rows[0].pid, 2);
  assert.match(rows[0].commandLine, /token \[REDACTED\]/);
});

test("thread compaction keeps glanceable metadata and truncates large previews", () => {
  const compact = compactThread({
    id: "thread-a",
    name: "Thread A",
    preview: "x".repeat(700),
    cwd: "Z:\\work",
    ignored: "not returned",
  }, 40);
  assert.equal(compact.id, "thread-a");
  assert.equal(compact.cwd, "Z:\\work");
  assert.equal(compact.ignored, undefined);
  assert.match(compact.preview, /truncated/);
  assert.ok(compact.preview.length < 90);
});

test("JsonRpcWsClient waitFor resolves existing notifications", async () => {
  const client = new JsonRpcWsClient("ws://example.invalid");
  client.notifications.push({ method: "turn/completed", params: { threadId: "t1" } });
  const matched = await client.waitFor((msg) => msg.method === "turn/completed", 100);
  assert.equal(matched.params.threadId, "t1");
});

test("extractLatestAgentText reads the most recent assistant item", () => {
  const text = extractLatestAgentText({
    turns: [
      { items: [{ type: "agentMessage", text: "older" }] },
      { items: [{ type: "userMessage", content: [] }, { type: "agentMessage", text: "newer" }] },
    ],
  });
  assert.equal(text, "newer");
});
