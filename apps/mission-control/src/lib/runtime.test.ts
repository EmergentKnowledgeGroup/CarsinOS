import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearGatewayToken,
  getGatewayToken,
  isGatewayTokenConfigured,
  loadConnectionSettings,
  persistConnectionSettings,
  setGatewayToken,
} from "./runtime";
import { STORAGE_KEYS } from "../storageKeys";

const SETTINGS_KEY = STORAGE_KEYS.gatewaySettings;
const TOKEN_KEY = STORAGE_KEYS.gatewayTokenFallback;

describe("runtime connection + token helpers", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.unstubAllEnvs();
  });

  afterEach(() => {
    window.localStorage.clear();
    vi.unstubAllEnvs();
  });

  it("prefers env gateway URL over persisted settings", () => {
    window.localStorage.setItem(SETTINGS_KEY, JSON.stringify({ gateway_url: "http://stale:9000" }));
    vi.stubEnv("VITE_CARSINOS_GATEWAY_URL", "http://127.0.0.1:18789");

    expect(loadConnectionSettings()).toEqual({ gateway_url: "http://127.0.0.1:18789/" });
  });

  it("persists normalized gateway URL", () => {
    persistConnectionSettings({ gateway_url: "127.0.0.1:18888" });
    const raw = window.localStorage.getItem(SETTINGS_KEY);
    expect(raw).not.toBeNull();
    expect(JSON.parse(raw ?? "{}")).toEqual({ gateway_url: "http://127.0.0.1:18888/" });
  });

  it("stores and retrieves web token from localStorage", async () => {
    await setGatewayToken("  token-abc  ");
    expect(window.localStorage.getItem(TOKEN_KEY)).toBe("token-abc");
    await expect(getGatewayToken()).resolves.toBe("token-abc");
    await expect(isGatewayTokenConfigured()).resolves.toBe(true);
  });

  it("clears token state", async () => {
    window.localStorage.setItem(TOKEN_KEY, "present");
    await clearGatewayToken();
    expect(window.localStorage.getItem(TOKEN_KEY)).toBeNull();
    await expect(isGatewayTokenConfigured()).resolves.toBe(false);
  });

  it("honors env token precedence when configured", async () => {
    window.localStorage.setItem(TOKEN_KEY, "stored-token");
    vi.stubEnv("VITE_CARSINOS_GATEWAY_TOKEN", "env-token");
    vi.stubEnv("VITE_CARSINOS_PREFER_ENV_TOKEN", "true");

    await expect(getGatewayToken()).resolves.toBe("env-token");
    await expect(isGatewayTokenConfigured()).resolves.toBe(true);
  });
});
