import type { ProviderCapabilityResponse } from "../types";

const LOCAL_PROVIDER_SET = new Set(["ollama", "vllm", "mock"]);

export function providerLabel(provider: string): string {
  switch (provider.trim().toLowerCase()) {
    case "openai":
      return "OpenAI";
    case "anthropic":
      return "Anthropic";
    case "openrouter":
      return "OpenRouter";
    case "ollama":
      return "Ollama";
    case "vllm":
      return "vLLM";
    case "mock":
      return "Mock";
    default:
      return provider.trim();
  }
}

export function isLocalProvider(provider: string): boolean {
  return LOCAL_PROVIDER_SET.has(provider.trim().toLowerCase());
}

export function localProviderCapabilities(
  items: ProviderCapabilityResponse[]
): ProviderCapabilityResponse[] {
  return items.filter((item) => isLocalProvider(item.provider));
}
