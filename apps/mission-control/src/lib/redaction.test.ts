import { describe, expect, it } from "vitest";
import { redactSecrets } from "./redaction";

describe("redactSecrets", () => {
  it("redacts known sensitive keys recursively", () => {
    const input = {
      token: "abc",
      nested: {
        access_token: "def",
        client_secret: "ghi",
      },
      safe: "ok",
    };

    expect(redactSecrets(input)).toEqual({
      token: "[REDACTED]",
      nested: {
        access_token: "[REDACTED]",
        client_secret: "[REDACTED]",
      },
      safe: "ok",
    });
  });

  it("redacts bearer and sk-* string patterns", () => {
    expect(redactSecrets("Authorization: Bearer abc123")).toBe(
      "Authorization: Bearer [REDACTED]"
    );
    expect(redactSecrets("sk-ant-abcdefghijk")).toBe("[REDACTED]");
    expect(redactSecrets("x-api-key: top-secret")).toBe("x-api-key: [REDACTED]");
    expect(redactSecrets("https://api.example.com/callback?code=abc123&token=xyz")).toBe(
      "https://api.example.com/callback?code=[REDACTED]&token=[REDACTED]"
    );
    expect(
      redactSecrets(
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature"
      )
    ).toBe("[REDACTED]");
  });
});
