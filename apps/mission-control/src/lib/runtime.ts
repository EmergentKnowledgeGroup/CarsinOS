import { invoke } from "@tauri-apps/api/core";
import type { RuntimeConnectionSettings } from "../types";
import { STORAGE_KEYS } from "../storageKeys";

const SETTINGS_KEY = STORAGE_KEYS.gatewaySettings;
const TOKEN_KEY_FALLBACK = STORAGE_KEYS.gatewayTokenFallback;

function normalizeGatewayUrlOrEmpty(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) {
    return "";
  }
  const normalized = trimmed.toLowerCase();
  const withScheme =
    normalized.startsWith("http://") || normalized.startsWith("https://")
      ? trimmed
      : `http://${trimmed}`;
  try {
    return `${new URL(withScheme).origin}/`;
  } catch {
    return "";
  }
}

function readEnvGatewayUrl(): string {
  return normalizeGatewayUrlOrEmpty(import.meta.env.VITE_CARSINOS_GATEWAY_URL ?? "");
}

function readEnvGatewayToken(): string | null {
  const value = (import.meta.env.VITE_CARSINOS_GATEWAY_TOKEN ?? "").trim();
  return value.length > 0 ? value : null;
}

function preferEnvGatewayToken(): boolean {
  const value = (import.meta.env.VITE_CARSINOS_PREFER_ENV_TOKEN ?? "").trim().toLowerCase();
  return value === "1" || value === "true" || value === "yes" || value === "on";
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function loadConnectionSettings(): RuntimeConnectionSettings {
  const envGatewayUrl = readEnvGatewayUrl();
  if (envGatewayUrl) {
    // One-click/dev launch should override stale persisted URLs (for example old busy ports).
    return { gateway_url: envGatewayUrl };
  }
  if (typeof window === "undefined") {
    return { gateway_url: "" };
  }
  const raw = window.localStorage.getItem(SETTINGS_KEY);
  if (!raw) {
    return { gateway_url: "" };
  }
  try {
    const parsed = JSON.parse(raw) as Partial<RuntimeConnectionSettings>;
    const configuredUrl = normalizeGatewayUrlOrEmpty(parsed.gateway_url ?? "");
    return {
      gateway_url: configuredUrl,
    };
  } catch {
    return { gateway_url: "" };
  }
}

export function persistConnectionSettings(settings: RuntimeConnectionSettings): void {
  if (typeof window === "undefined") {
    return;
  }
  const normalizedUrl = normalizeGatewayUrlOrEmpty(settings.gateway_url) || settings.gateway_url.trim();
  window.localStorage.setItem(
    SETTINGS_KEY,
    JSON.stringify({ gateway_url: normalizedUrl })
  );
}

export async function setGatewayToken(token: string): Promise<void> {
  const value = token.trim();
  if (!value) {
    throw new Error("token cannot be empty");
  }
  if (isTauriRuntime()) {
    await invoke("set_gateway_token", { token: value });
    return;
  }
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(TOKEN_KEY_FALLBACK, value);
}

export async function clearGatewayToken(): Promise<void> {
  if (isTauriRuntime()) {
    await invoke("clear_gateway_token");
    return;
  }
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.removeItem(TOKEN_KEY_FALLBACK);
}

export async function getGatewayToken(): Promise<string | null> {
  const envToken = readEnvGatewayToken();
  if (preferEnvGatewayToken() && envToken) {
    return envToken;
  }
  if (isTauriRuntime()) {
    const storedToken = await invoke<string | null>("get_gateway_token");
    if (storedToken && storedToken.trim().length > 0) {
      return storedToken.trim();
    }
    return envToken;
  }
  if (typeof window === "undefined") {
    return envToken;
  }
  const storedToken = window.localStorage.getItem(TOKEN_KEY_FALLBACK);
  if (storedToken && storedToken.trim().length > 0) {
    return storedToken.trim();
  }
  return envToken;
}

export async function isGatewayTokenConfigured(): Promise<boolean> {
  const envToken = readEnvGatewayToken();
  if (preferEnvGatewayToken() && envToken) {
    return true;
  }
  if (isTauriRuntime()) {
    const hasStoredToken = await invoke<boolean>("gateway_token_present");
    return hasStoredToken || Boolean(envToken);
  }
  if (typeof window === "undefined") {
    return Boolean(envToken);
  }
  const storedToken = window.localStorage.getItem(TOKEN_KEY_FALLBACK);
  return Boolean(storedToken && storedToken.trim().length > 0) || Boolean(envToken);
}

export interface SetupTokenLaunchResult {
  launched: boolean;
  command: string;
  detail: string;
}

export async function launchAnthropicSetupTokenFlow(): Promise<SetupTokenLaunchResult> {
  const command = "claude setup-token";
  if (isTauriRuntime()) {
    const detail = await invoke<string>("launch_anthropic_setup_token");
    return {
      launched: true,
      command,
      detail,
    };
  }
  return {
    launched: false,
    command,
    detail: "Desktop launcher unavailable in web mode. Run the command in a terminal, then paste the token.",
  };
}
