import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getGatewayHealth, websocketUrlFromGateway } from "./api";
import { getGatewayToken } from "./runtime";

vi.mock("./runtime", () => ({
  getGatewayToken: vi.fn(),
}));

describe("websocketUrlFromGateway", () => {
  it("normalizes an http gateway URL to ws with token query", () => {
    const wsUrl = websocketUrlFromGateway({ gateway_url: "127.0.0.1:18789" }, "token-123");
    expect(wsUrl).toBe("ws://127.0.0.1:18789/api/v1/ws?token=token-123");
  });

  it("upgrades https gateway URL to wss and encodes token", () => {
    const wsUrl = websocketUrlFromGateway(
      { gateway_url: "https://carsinos.local:443" },
      "token with spaces"
    );
    expect(wsUrl).toBe(
      "wss://carsinos.local/api/v1/ws?token=token+with+spaces"
    );
  });

  it("throws on invalid gateway URL", () => {
    expect(() => websocketUrlFromGateway({ gateway_url: "https://" }, "x")).toThrow(
      /Invalid Gateway URL/
    );
  });
});

describe("request URL resolution", () => {
  beforeEach(() => {
    vi.mocked(getGatewayToken).mockResolvedValue("token-123");
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("uses the latest gateway URL when runtime settings change mid-session", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await getGatewayHealth({ gateway_url: "http://127.0.0.1:19789" });
    await getGatewayHealth({ gateway_url: "http://127.0.0.1:19890" });

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://127.0.0.1:19789/api/v1/health",
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      })
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://127.0.0.1:19890/api/v1/health",
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      })
    );
  });
});
