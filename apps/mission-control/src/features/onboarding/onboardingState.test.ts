import { describe, expect, it } from "vitest";
import type { Agent, AuthProfileResponse, RuntimeConnectionSettings } from "../../types";
import {
  ONBOARDING_DISMISS_WINDOW_MS,
  hasConfiguredLocalAgent,
  hasEnabledCloudProfile,
  isOnboardingComplete,
  reorderProfileFirst,
  shouldAutoOpenWizard,
} from "./onboardingState";

function makeAgent(partial: Partial<Agent>): Agent {
  return {
    agent_id: partial.agent_id ?? "agent-1",
    name: partial.name ?? "Agent",
    model_provider: partial.model_provider ?? "anthropic",
    model_id: partial.model_id ?? "claude-sonnet-4-5",
    workspace_root: partial.workspace_root ?? ".",
    tool_profile: partial.tool_profile ?? "default",
  };
}

function makeProfile(partial: Partial<AuthProfileResponse>): AuthProfileResponse {
  return {
    auth_profile_id: partial.auth_profile_id ?? "profile-1",
    provider: partial.provider ?? "anthropic",
    display_name: partial.display_name ?? "primary",
    auth_mode: partial.auth_mode ?? "api_key",
    risk_level: partial.risk_level ?? "standard",
    enabled: partial.enabled ?? true,
    kill_switch_scope: partial.kill_switch_scope ?? "none",
    api_base_url: partial.api_base_url ?? null,
    created_at: partial.created_at ?? 0,
    updated_at: partial.updated_at ?? 0,
  };
}

function makeSettings(partial: Partial<RuntimeConnectionSettings>): RuntimeConnectionSettings {
  return {
    gateway_url: partial.gateway_url ?? "http://127.0.0.1:18789",
  };
}

describe("onboardingState", () => {
  it("detects enabled cloud profiles", () => {
    expect(
      hasEnabledCloudProfile([
        makeProfile({ provider: "openai", enabled: true }),
        makeProfile({ provider: "local", enabled: true }),
      ])
    ).toBe(true);

    expect(
      hasEnabledCloudProfile([
        makeProfile({ provider: "anthropic", enabled: false }),
        makeProfile({ provider: "local", enabled: true }),
      ])
    ).toBe(false);

    expect(
      hasEnabledCloudProfile([
        makeProfile({
          provider: "anthropic",
          auth_mode: "oauth",
          enabled: true,
        }),
      ])
    ).toBe(false);
  });

  it("detects configured local agents", () => {
    expect(
      hasConfiguredLocalAgent([
        makeAgent({ model_provider: "lmstudio" }),
        makeAgent({ model_provider: "anthropic" }),
      ])
    ).toBe(true);

    expect(
      hasConfiguredLocalAgent([
        makeAgent({ model_provider: "anthropic" }),
        makeAgent({ model_provider: "openai" }),
      ])
    ).toBe(false);
  });

  it("requires gateway + token + agent + provider path for onboarding completeness", () => {
    const base = {
      settings: makeSettings({}),
      tokenConfigured: true,
      agents: [makeAgent({ model_provider: "lmstudio" })],
      authProfiles: [] as AuthProfileResponse[],
    };

    expect(isOnboardingComplete(base)).toBe(true);
    expect(isOnboardingComplete({ ...base, settings: makeSettings({ gateway_url: "" }) })).toBe(
      false
    );
    expect(isOnboardingComplete({ ...base, tokenConfigured: false })).toBe(false);
    expect(isOnboardingComplete({ ...base, agents: [] })).toBe(false);
    expect(
      isOnboardingComplete({
        ...base,
        agents: [makeAgent({ model_provider: "anthropic" })],
        authProfiles: [],
      })
    ).toBe(false);
  });

  it("auto-opens wizard unless dismissed within the 24h window", () => {
    const incomplete = {
      settings: makeSettings({ gateway_url: "" }),
      tokenConfigured: false,
      agents: [],
      authProfiles: [] as AuthProfileResponse[],
    };
    const now = 1_000_000;

    expect(shouldAutoOpenWizard(incomplete, { dismissedAtMs: null, nowMs: now })).toBe(true);
    expect(
      shouldAutoOpenWizard(incomplete, {
        dismissedAtMs: now - (ONBOARDING_DISMISS_WINDOW_MS - 1),
        nowMs: now,
      })
    ).toBe(false);
    expect(
      shouldAutoOpenWizard(incomplete, {
        dismissedAtMs: now - ONBOARDING_DISMISS_WINDOW_MS,
        nowMs: now,
      })
    ).toBe(true);
  });

  it("waits for the initial bootstrap to settle before auto-opening a connected install", () => {
    const pendingBootstrap = {
      settings: makeSettings({}),
      tokenConfigured: true,
      agents: [],
      authProfiles: [] as AuthProfileResponse[],
    };

    expect(
      shouldAutoOpenWizard(pendingBootstrap, {
        dismissedAtMs: null,
        bootstrapSettled: false,
      })
    ).toBe(false);

    expect(
      shouldAutoOpenWizard(pendingBootstrap, {
        dismissedAtMs: null,
        bootstrapSettled: true,
      })
    ).toBe(true);
  });

  it("reorders selected profile to the top without duplicates", () => {
    expect(reorderProfileFirst(["a", "b", "c"], "b")).toEqual(["b", "a", "c"]);
    expect(reorderProfileFirst(["a", "b", "c"], "")).toEqual(["a", "b", "c"]);
  });
});
