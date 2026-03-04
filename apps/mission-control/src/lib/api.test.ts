import { describe, expect, it } from "vitest";
import { websocketUrlFromGateway } from "./api";

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
