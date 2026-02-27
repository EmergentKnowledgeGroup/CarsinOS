import { invoke } from "@tauri-apps/api/core";
import type { RuntimeConnectionSettings } from "../types";

const SETTINGS_KEY = "mission_control.runtime.connection.v1";
const TOKEN_KEY_FALLBACK = "mission_control.runtime.token.v1";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function loadConnectionSettings(): RuntimeConnectionSettings {
  if (typeof window === "undefined") {
    return { gateway_url: "" };
  }
  const raw = window.localStorage.getItem(SETTINGS_KEY);
  if (!raw) {
    return { gateway_url: "" };
  }
  try {
    const parsed = JSON.parse(raw) as Partial<RuntimeConnectionSettings>;
    return {
      gateway_url: (parsed.gateway_url ?? "").trim(),
    };
  } catch {
    return { gateway_url: "" };
  }
}

export function persistConnectionSettings(settings: RuntimeConnectionSettings): void {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(
    SETTINGS_KEY,
    JSON.stringify({ gateway_url: settings.gateway_url.trim() })
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
  if (isTauriRuntime()) {
    return invoke<string | null>("get_gateway_token");
  }
  if (typeof window === "undefined") {
    return null;
  }
  return window.localStorage.getItem(TOKEN_KEY_FALLBACK);
}

export async function isGatewayTokenConfigured(): Promise<boolean> {
  if (isTauriRuntime()) {
    return invoke<boolean>("gateway_token_present");
  }
  if (typeof window === "undefined") {
    return false;
  }
  return Boolean(window.localStorage.getItem(TOKEN_KEY_FALLBACK));
}
