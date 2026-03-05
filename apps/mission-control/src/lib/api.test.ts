import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  getGatewayHealth,
  getMissionControlUsage,
  removeAgent,
  revokeAuthProfile,
  validateAnthropicSetupToken,
  websocketUrlFromGateway,
} from "./api";
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

  it("hits remove-agent and auth validation endpoints with POST", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify({ removed: true, valid: true, profile: {} }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await removeAgent({ gateway_url: "http://127.0.0.1:18888" }, "assistant-main");
    await validateAnthropicSetupToken(
      { gateway_url: "http://127.0.0.1:18888" },
      { setup_token: "token-1", api_base_url: "https://api.anthropic.com" }
    );
    await revokeAuthProfile(
      { gateway_url: "http://127.0.0.1:18888" },
      "profile-1",
      { reason: "reauth", remove_secret: true }
    );

    expect(fetchMock).toHaveBeenCalledTimes(3);
    const [removeUrl, removeInit] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(removeUrl).toContain("/api/v1/agents/assistant-main/remove");
    expect(removeInit.method).toBe("POST");

    const [validateUrl, validateInit] = fetchMock.mock.calls[1] as [string, RequestInit];
    expect(validateUrl).toContain("/api/v1/auth/anthropic/setup-token/validate");
    expect(validateInit.method).toBe("POST");

    const [revokeUrl, revokeInit] = fetchMock.mock.calls[2] as [string, RequestInit];
    expect(revokeUrl).toContain("/api/v1/security/auth-profiles/profile-1/revoke");
    expect(revokeInit.method).toBe("POST");
  });
});
