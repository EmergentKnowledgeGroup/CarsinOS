import { describe, expect, it } from "vitest";
import {
  ANTHROPIC_SETUP_TOKEN_MIN_LENGTH,
  ANTHROPIC_SETUP_TOKEN_PREFIX,
  buildProviderOptions,
  enabledAuthProfilesForProvider,
  formatModelDiscoveryNote,
  looksLikeAnthropicSetupToken,
  mergeCatalogModelOptions,
  normalizeAnthropicSetupToken,
  pickCatalogModel,
  profileSupportsSelection,
  validateAnthropicSetupTokenFormat,
} from "./providerModelCatalog";

describe("providerModelCatalog", () => {
  it("keeps the current provider in the options list even when capabilities omit it", () => {
    const options = buildProviderOptions(
      [
        {
          provider: "openai",
          supports_streaming: true,
          supports_tools: true,
          supports_json_mode: true,
          supports_vision: true,
          max_context_tokens: 128000,
          error_classes: [],
          retryable_error_classes: [],
        },
      ],
      null,
      "anthropic"
    );

    expect(options.map((item) => item.value)).toContain("openai");
    expect(options.map((item) => item.value)).toContain("anthropic");
  });

  it("preserves the current model at the front when the live catalog omits it", () => {
    expect(mergeCatalogModelOptions("claude-opus-custom", ["claude-sonnet-4-5"])).toEqual([
      "claude-opus-custom",
      "claude-sonnet-4-5",
    ]);
  });

  it("prefers the current model when it is still present, otherwise uses the first live model", () => {
    expect(
      pickCatalogModel("claude-sonnet-4-5", [
        "claude-opus-4-5",
        "claude-sonnet-4-5",
      ])
    ).toBe("claude-sonnet-4-5");
    expect(
      pickCatalogModel("claude-old", ["claude-opus-4-5", "claude-sonnet-4-5"])
    ).toBe("claude-opus-4-5");
  });

  it("filters enabled auth profiles to the requested provider", () => {
    const profiles = enabledAuthProfilesForProvider(
      [
        {
          auth_profile_id: "a",
          provider: "anthropic",
          display_name: "Claude Primary",
          auth_mode: "api_key",
          risk_level: "low",
          enabled: true,
          kill_switch_scope: "profile",
          api_base_url: null,
          created_at: 1,
          updated_at: 1,
        },
        {
          auth_profile_id: "b",
          provider: "anthropic",
          display_name: "Claude Disabled",
          auth_mode: "api_key",
          risk_level: "high",
          enabled: false,
          kill_switch_scope: "profile",
          api_base_url: null,
          created_at: 1,
          updated_at: 1,
        },
        {
          auth_profile_id: "c",
          provider: "openai",
          display_name: "OpenAI Primary",
          auth_mode: "oauth",
          risk_level: "high",
          enabled: true,
          kill_switch_scope: "profile",
          api_base_url: null,
          created_at: 1,
          updated_at: 1,
        },
      ],
      "anthropic"
    );

    expect(profiles.map((item) => item.auth_profile_id)).toEqual(["a"]);
  });

  it("rejects unsupported Anthropic auth profiles from selection surfaces", () => {
    expect(
      profileSupportsSelection(
        {
          auth_profile_id: "legacy",
          provider: "anthropic",
          display_name: "Legacy OAuth",
          auth_mode: "oauth",
          risk_level: "high",
          enabled: true,
          kill_switch_scope: "profile",
          api_base_url: null,
          created_at: 1,
          updated_at: 1,
        },
        "anthropic"
      )
    ).toBe(false);
  });

  it("formats caveman-friendly discovery notes", () => {
    expect(formatModelDiscoveryNote("anthropic", ["claude-sonnet-4-5"])).toBe(
      "Claude login ready. carsinOS loaded 1 model choice for you."
    );
    expect(formatModelDiscoveryNote("openai", [])).toBe(
      "No models were reported by OpenAI yet."
    );
  });

  it("recognizes real-looking Claude setup tokens", () => {
    const token = `${ANTHROPIC_SETUP_TOKEN_PREFIX}${"a".repeat(
      ANTHROPIC_SETUP_TOKEN_MIN_LENGTH
    )}`;
    expect(looksLikeAnthropicSetupToken(token)).toBe(true);
    expect(validateAnthropicSetupTokenFormat(token)).toBeNull();
  });

  it("rejects the wrong Claude token shape early", () => {
    expect(validateAnthropicSetupTokenFormat("oauth-access-token")).toContain(
      ANTHROPIC_SETUP_TOKEN_PREFIX
    );
    expect(validateAnthropicSetupTokenFormat(`${ANTHROPIC_SETUP_TOKEN_PREFIX}short`)).toContain(
      "too short"
    );
  });

  it("removes wrapped whitespace from Claude setup tokens before validation", () => {
    const token = `${ANTHROPIC_SETUP_TOKEN_PREFIX}${"a".repeat(
      ANTHROPIC_SETUP_TOKEN_MIN_LENGTH
    )}`;
    const wrapped = `${token.slice(0, 40)} \n ${token.slice(40)}`;
    expect(normalizeAnthropicSetupToken(wrapped)).toBe(token);
    expect(looksLikeAnthropicSetupToken(wrapped)).toBe(true);
    expect(validateAnthropicSetupTokenFormat(wrapped)).toBeNull();
  });
});
