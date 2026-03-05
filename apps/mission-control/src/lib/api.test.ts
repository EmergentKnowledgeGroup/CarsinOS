import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getGatewayHealth, getMissionControlUsage, websocketUrlFromGateway } from "./api";
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

  it("builds mission-control usage query with window + timezone metadata", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(
        JSON.stringify({
          contract_version: "mc-usage-v1",
          available: false,
          window: "today",
          timezone: "UTC",
          currency: "USD",
          window_start_utc: null,
          window_end_utc: null,
          estimated_cost_total: null,
          token_input_total: null,
          token_output_total: null,
          by_agent: null,
          by_model: null,
          by_provider: null,
          by_time: null,
          by_job: null,
          by_card: null,
          budget_thresholds: null,
          updated_at_utc: null,
          reason_code: "USAGE_UNAVAILABLE",
          detail: "stub",
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }
      )
    );
    vi.stubGlobal("fetch", fetchMock);

    await getMissionControlUsage(
      { gateway_url: "http://127.0.0.1:19999" },
      {
        window: "today",
        timezone: "America/Chicago",
        tz_offset_minutes: -360,
      }
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [calledUrl] = fetchMock.mock.calls[0] as [string];
    expect(calledUrl).toContain("/api/v1/mission-control/usage?");
    expect(calledUrl).toContain("window=today");
    expect(calledUrl).toContain("timezone=America%2FChicago");
    expect(calledUrl).toContain("tz_offset_minutes=-360");
  });
});
