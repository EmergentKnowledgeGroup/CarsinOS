import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearGatewayToken,
  getGatewayToken,
  getDesktopBootstrap,
  isGatewayTokenConfigured,
  loadConnectionSettings,
  persistConnectionSettings,
  setGatewayToken,
} from "./runtime";
import { STORAGE_KEYS } from "../storageKeys";

const SETTINGS_KEY = STORAGE_KEYS.gatewaySettings;
const TOKEN_KEY = STORAGE_KEYS.gatewayTokenFallback;

describe("runtime connection + token helpers", () => {
  beforeEach(async () => {
    vi.unstubAllEnvs();
    window.history.replaceState({}, "", "/");
    window.localStorage.clear();
    window.sessionStorage.clear();
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    await clearGatewayToken();
  });

  afterEach(async () => {
    vi.unstubAllEnvs();
    window.history.replaceState({}, "", "/");
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    await clearGatewayToken();
    window.localStorage.clear();
    window.sessionStorage.clear();
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

  it("uses the managed loopback gateway in packaged desktop builds", () => {
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    window.localStorage.setItem(SETTINGS_KEY, JSON.stringify({ gateway_url: "http://stale:9000" }));

    expect(loadConnectionSettings()).toEqual({ gateway_url: "http://127.0.0.1:18789/" });
  });

  it("does not invent a desktop bootstrap in the browser", async () => {
    await expect(getDesktopBootstrap()).resolves.toBeNull();
  });

  it("keeps web tokens in memory outside the E2E session-storage harness", async () => {
    await setGatewayToken("  token-abc  ");
    expect(window.localStorage.getItem(TOKEN_KEY)).toBeNull();
    expect(window.sessionStorage.getItem(TOKEN_KEY)).toBeNull();
    await expect(getGatewayToken()).resolves.toBe("token-abc");
    await expect(isGatewayTokenConfigured()).resolves.toBe(true);
  });

  it("uses sessionStorage only in the explicit E2E browser harness", async () => {
    window.history.replaceState({}, "", "/?e2e=1");

    await setGatewayToken("  e2e-token  ");
    expect(window.localStorage.getItem(TOKEN_KEY)).toBeNull();
    expect(window.sessionStorage.getItem(TOKEN_KEY)).toBe("e2e-token");
    await expect(getGatewayToken()).resolves.toBe("e2e-token");
    await expect(isGatewayTokenConfigured()).resolves.toBe(true);
  });

  it("clears token state", async () => {
    await setGatewayToken("present");
    window.localStorage.setItem(TOKEN_KEY, "legacy-present");
    window.sessionStorage.setItem(TOKEN_KEY, "session-present");
    await clearGatewayToken();
    expect(window.localStorage.getItem(TOKEN_KEY)).toBeNull();
    expect(window.sessionStorage.getItem(TOKEN_KEY)).toBeNull();
    await expect(isGatewayTokenConfigured()).resolves.toBe(false);
  });

  it("purges and ignores the legacy localStorage token fallback", async () => {
    window.localStorage.setItem(TOKEN_KEY, "legacy-token");

    await expect(getGatewayToken()).resolves.toBeNull();
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
