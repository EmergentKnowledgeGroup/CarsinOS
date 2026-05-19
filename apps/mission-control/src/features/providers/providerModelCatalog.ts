import { providerLabel } from "../../lib/providerCatalog";
import type { AuthProfileResponse, ProviderCapabilityResponse } from "../../types";

export interface ProviderOption {
  value: string;
  label: string;
}

export const FALLBACK_PROVIDER_OPTIONS: ProviderOption[] = [
  { value: "anthropic", label: providerLabel("anthropic") },
  { value: "openai", label: providerLabel("openai") },
  { value: "ollama", label: providerLabel("ollama") },
  { value: "lmstudio", label: providerLabel("lmstudio") },
  { value: "vllm", label: providerLabel("vllm") },
  { value: "mock", label: providerLabel("mock") },
];

export function buildProviderOptions(
  providerCapabilities: ProviderCapabilityResponse[],
  providerCapabilitiesError: string | null,
  currentProvider: string
): ProviderOption[] {
  const mapped = providerCapabilities
    .filter((item) => item.provider !== "unconfigured")
    .map((item) => ({
      value: item.provider,
      label: providerLabel(item.provider),
    }))
    .sort((left, right) => left.label.localeCompare(right.label));
  const base =
    providerCapabilitiesError || mapped.length === 0
      ? [...FALLBACK_PROVIDER_OPTIONS]
      : mapped;
  const trimmedCurrent = currentProvider.trim();
  if (trimmedCurrent && !base.some((item) => item.value === trimmedCurrent)) {
    base.push({
      value: trimmedCurrent,
      label: providerLabel(trimmedCurrent),
    });
  }
  return base;
}

export function mergeCatalogModelOptions(currentModelId: string, modelIds: string[]): string[] {
  const trimmedCurrent = currentModelId.trim();
  if (trimmedCurrent && !modelIds.includes(trimmedCurrent)) {
    return [trimmedCurrent, ...modelIds];
  }
  return modelIds;
}

export function pickCatalogModel(currentModelId: string, modelIds: string[]): string {
  const trimmedCurrent = currentModelId.trim();
  if (trimmedCurrent && modelIds.includes(trimmedCurrent)) {
    return trimmedCurrent;
  }
  return modelIds[0] ?? "";
}

export function enabledAuthProfilesForProvider(
  authProfiles: AuthProfileResponse[],
  provider: string
): AuthProfileResponse[] {
  const normalizedProvider = provider.trim().toLowerCase();
  return authProfiles
    .filter(
      (profile) =>
        profile.enabled && profileSupportsSelection(profile, normalizedProvider)
    )
    .sort((left, right) => left.display_name.localeCompare(right.display_name));
}

export function profileSupportsSelection(
  profile: AuthProfileResponse,
  provider: string
): boolean {
  const normalizedProvider = provider.trim().toLowerCase();
  if (profile.provider.trim().toLowerCase() !== normalizedProvider) {
    return false;
  }
  if (normalizedProvider !== "anthropic") {
    return true;
  }
  const authMode = profile.auth_mode.trim().toLowerCase();
  return authMode === "api_key";
}

export function formatModelDiscoveryNote(provider: string, modelIds: string[]): string {
  const label = providerLabel(provider);
  if (modelIds.length === 0) {
    return `No models were reported by ${label} yet.`;
  }
  if (provider.trim().toLowerCase() === "anthropic") {
    return `Anthropic API key ready. carsinOS loaded ${modelIds.length} model choice${
      modelIds.length === 1 ? "" : "s"
    } for you.`;
  }
  return `Found ${modelIds.length} model${modelIds.length === 1 ? "" : "s"} from ${label}.`;
}
