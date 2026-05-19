import type { Agent, AuthProfileResponse, RuntimeConnectionSettings } from "../../types";
import { STORAGE_KEYS } from "../../storageKeys";
import { isLocalProvider } from "../../lib/providerCatalog";
import { profileSupportsSelection } from "../providers/providerModelCatalog";

export const ONBOARDING_DISMISSED_KEY = STORAGE_KEYS.onboardingDismissedAtMs;
export const ONBOARDING_DISMISS_WINDOW_MS = 24 * 60 * 60 * 1000;

export type OnboardingMode = "quickstart" | "manual";
export type OnboardingProviderPath = "anthropic" | "openai" | "local";
export type OnboardingAnthropicAuthMode = "api_key";

export type OnboardingStepId =
  | "mode"
  | "preflight"
  | "connect"
  | "provider"
  | "review"
  | "done";

export interface OnboardingCompletenessInput {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  agents: Agent[];
  authProfiles: AuthProfileResponse[];
}

export function hasEnabledCloudProfile(profiles: AuthProfileResponse[]): boolean {
  return profiles.some(
    (profile) =>
      profile.enabled &&
      (profileSupportsSelection(profile, "anthropic") ||
        profile.provider.toLowerCase() === "openai")
  );
}

export function hasConfiguredLocalAgent(agents: Agent[]): boolean {
  return agents.some((agent) => isLocalProvider(agent.model_provider));
}

export function isOnboardingComplete(input: OnboardingCompletenessInput): boolean {
  const hasGatewayUrl = input.settings.gateway_url.trim().length > 0;
  const hasToken = input.tokenConfigured;
  const hasAgent = input.agents.length > 0;
  const hasProviderPath =
    hasEnabledCloudProfile(input.authProfiles) || hasConfiguredLocalAgent(input.agents);
  return hasGatewayUrl && hasToken && hasAgent && hasProviderPath;
}

export function loadDismissedAt(): number | null {
  if (typeof window === "undefined") {
    return null;
  }
  let raw: string | null;
  try {
    raw = window.localStorage.getItem(ONBOARDING_DISMISSED_KEY);
  } catch (error) {
    console.debug("onboarding dismissal read failed", error);
    return null;
  }
  if (!raw) {
    return null;
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return null;
  }
  return parsed;
}

export function setDismissedNow(): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(ONBOARDING_DISMISSED_KEY, String(Date.now()));
  } catch (error) {
    console.debug("onboarding dismissal write failed", error);
  }
}

export function shouldAutoOpenWizard(
  input: OnboardingCompletenessInput,
  options: {
    dismissedAtMs: number | null;
    nowMs?: number;
    bootstrapSettled?: boolean;
  }
): boolean {
  const bootstrapSettled = options.bootstrapSettled ?? true;
  const hasGatewayUrl = input.settings.gateway_url.trim().length > 0;
  if (hasGatewayUrl && !bootstrapSettled) {
    return false;
  }
  if (isOnboardingComplete(input)) {
    return false;
  }
  const nowMs = options.nowMs ?? Date.now();
  const dismissedAtMs = options.dismissedAtMs;
  if (dismissedAtMs === null) {
    return true;
  }
  const elapsed = nowMs - dismissedAtMs;
  if (elapsed < 0) {
    return true;
  }
  return elapsed >= ONBOARDING_DISMISS_WINDOW_MS;
}

export function reorderProfileFirst(existingIds: string[], targetProfileId: string): string[] {
  const cleanedTarget = targetProfileId.trim();
  if (!cleanedTarget) {
    return [...existingIds];
  }
  const deduped = existingIds.filter((id) => id !== cleanedTarget);
  return [cleanedTarget, ...deduped];
}
