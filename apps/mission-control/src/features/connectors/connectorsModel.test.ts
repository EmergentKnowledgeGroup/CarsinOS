import { describe, expect, it } from "vitest";
import { GatewayApiError } from "../../lib/api";
import { isConnectorUnsupportedError } from "./connectorsModel";

describe("connectorsModel", () => {
  it("treats registry-surface 404s as unsupported", () => {
    const error = new GatewayApiError("404 Not Found", {
      kind: "http",
      status: 404,
      statusText: "Not Found",
      path: "/api/v1/connectors/catalog",
    });

    expect(isConnectorUnsupportedError(error)).toBe(true);
  });

  it("does not treat connector-detail 404s as unsupported", () => {
    const error = new GatewayApiError("404 Not Found", {
      kind: "http",
      status: 404,
      statusText: "Not Found",
      path: "/api/v1/connectors/connector-123",
    });

    expect(isConnectorUnsupportedError(error)).toBe(false);
  });
});
