const assert = require("node:assert/strict");
const http = require("node:http");
const test = require("node:test");
const { route } = require("../relay/server.js");

async function withServer(fn) {
  const server = http.createServer(route);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    const address = server.address();
    await fn(`http://127.0.0.1:${address.port}`);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

test("server rejects mutating requests from non-loopback browser origins", async () => {
  await withServer(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/codex-cli/exec`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        origin: "https://evil.example",
      },
      body: JSON.stringify({ prompt: "hi" }),
    });

    assert.equal(response.status, 403);
    assert.notEqual(response.headers.get("access-control-allow-origin"), "*");
    assert.equal((await response.json()).error, "forbidden origin");
  });
});

test("server allows mutating requests without a browser origin header", async () => {
  await withServer(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/codex-cli/exec`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ prompt: "hi", cwd: process.cwd() }),
    });

    assert.notEqual(response.status, 403);
    assert.notEqual(response.headers.get("access-control-allow-origin"), "*");
  });
});
