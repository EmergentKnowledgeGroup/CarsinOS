import { describe, expect, it } from "vitest";
import { redactSecrets } from "./redaction";

describe("redactSecrets", () => {
  it("redacts known sensitive keys recursively", () => {
    const input = {
      token: "abc",
      nested: {
        access_token: "def",
        client_secret: "ghi",
        apiKey: "jkl",
      },
      safe: "ok",
    };

    expect(redactSecrets(input)).toEqual({
      token: "[REDACTED]",
      nested: {
        access_token: "[REDACTED]",
        client_secret: "[REDACTED]",
        apiKey: "[REDACTED]",
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
    expect(redactSecrets("oauth_code=abc123 auth_code=xyz987")).toBe(
      "oauth_code=[REDACTED] auth_code=[REDACTED]"
    );
    expect(redactSecrets("https://x.test/cb?oauth_code=abc&auth_code=def")).toBe(
      "https://x.test/cb?oauth_code=[REDACTED]&auth_code=[REDACTED]"
    );
    expect(redactSecrets('"access_token": "value"')).toBe('"access_token": "[REDACTED]"');
    expect(redactSecrets("'api_key':'secret'")).toBe("'api_key':'[REDACTED]'");
    expect(
      redactSecrets(
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature"
      )
    ).toBe("[REDACTED]");
  });
});
