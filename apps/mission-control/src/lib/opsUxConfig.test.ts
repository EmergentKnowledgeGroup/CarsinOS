import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_OPSUX_RUNTIME_CONFIG,
  loadOpsUxRuntimeConfig,
  saveOpsUxRuntimeConfig,
  subscribeOpsUxRuntimeConfig,
  sanitizeOpsUxRuntimeConfig,
  withOpsUxControlPatch,
} from "./opsUxConfig";
import { STORAGE_KEYS } from "../storageKeys";

afterEach(() => {
  window.localStorage.clear();
  vi.restoreAllMocks();
});

describe("opsUxRuntimeConfig", () => {
  it("keeps approved fail-safe defaults", () => {
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.live_feed_drawer).toBe(false);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.incident_auto_trigger).toBe(false);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.strategy_hub).toBe(false);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.runbook_hub).toBe(false);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.memory_hub).toBe(false);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.connectors_hub).toBe(false);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_high_burst_threshold).toBe(5);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_auto_cooldown_ms).toBe(10 * 60_000);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.safety.recovery_retention_window_ms).toBe(30 * 60_000);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.safety.recovery_log_max_bytes).toBe(50 * 1024 * 1024);
  });

  it("sanitizes malformed runtime config payloads", () => {
    const sanitized = sanitizeOpsUxRuntimeConfig({
      controls: {
        live_feed_drawer: true,
        incident_auto_trigger: "yes",
        strategy_hub: true,
        runbook_hub: true,
        memory_hub: true,
        connectors_hub: true,
      },
      safety: {
        incident_high_burst_threshold: 8,
        incident_auto_cooldown_ms: -4,
      },
    });

    expect(sanitized.controls.live_feed_drawer).toBe(true);
    expect(sanitized.controls.strategy_hub).toBe(true);
    expect(sanitized.controls.runbook_hub).toBe(true);
    expect(sanitized.controls.memory_hub).toBe(true);
    expect(sanitized.controls.connectors_hub).toBe(true);
    expect(sanitized.controls.incident_auto_trigger).toBe(
      DEFAULT_OPSUX_RUNTIME_CONFIG.controls.incident_auto_trigger
    );
    expect(sanitized.safety.incident_high_burst_threshold).toBe(8);
    expect(sanitized.safety.incident_auto_cooldown_ms).toBe(
      DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_auto_cooldown_ms
    );
  });

  it("patches controls without mutating safety profile", () => {
    const originalSafety = DEFAULT_OPSUX_RUNTIME_CONFIG.safety;
    const next = withOpsUxControlPatch(DEFAULT_OPSUX_RUNTIME_CONFIG, {
      global_kill_switch: true,
      live_feed_drawer: true,
      strategy_hub: true,
      runbook_hub: true,
      memory_hub: true,
      connectors_hub: true,
    });

    expect(next.controls.global_kill_switch).toBe(true);
    expect(next.controls.live_feed_drawer).toBe(true);
    expect(next.controls.strategy_hub).toBe(true);
    expect(next.controls.runbook_hub).toBe(true);
    expect(next.controls.memory_hub).toBe(true);
    expect(next.controls.connectors_hub).toBe(true);
    expect(next.safety).toBe(originalSafety);
  });

  it("notifies same-tab subscribers after a save", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeOpsUxRuntimeConfig(listener);
    const next = withOpsUxControlPatch(DEFAULT_OPSUX_RUNTIME_CONFIG, {
      strategy_hub: true,
    });

    const result = saveOpsUxRuntimeConfig(next);

    expect(result).toEqual({ ok: true, error: null });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith({
      config: next,
      degraded: false,
      error: null,
    });
    unsubscribe();
  });

  it("does not fail persistence when one listener throws", () => {
    const badListener = vi.fn(() => {
      throw new Error("listener broke");
    });
    const goodListener = vi.fn();
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const unsubscribeBad = subscribeOpsUxRuntimeConfig(badListener);
    const unsubscribeGood = subscribeOpsUxRuntimeConfig(goodListener);
    const next = withOpsUxControlPatch(DEFAULT_OPSUX_RUNTIME_CONFIG, {
      runbook_hub: true,
    });

    const result = saveOpsUxRuntimeConfig(next);

    expect(result).toEqual({ ok: true, error: null });
    expect(badListener).toHaveBeenCalledTimes(1);
    expect(goodListener).toHaveBeenCalledTimes(1);
    expect(consoleError).toHaveBeenCalledTimes(1);
    unsubscribeBad();
    unsubscribeGood();
  });

  it("reloads listeners when storage changes outside the current save path", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeOpsUxRuntimeConfig(listener);
    const next = withOpsUxControlPatch(DEFAULT_OPSUX_RUNTIME_CONFIG, {
      runbook_hub: true,
    });

    window.localStorage.setItem(STORAGE_KEYS.opsUxRuntimeConfigV1, JSON.stringify(next));
    window.dispatchEvent(
      new StorageEvent("storage", {
        key: STORAGE_KEYS.opsUxRuntimeConfigV1,
        newValue: JSON.stringify(next),
      })
    );

    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith(loadOpsUxRuntimeConfig());
    unsubscribe();
  });

  it("returns a stable snapshot object when storage state has not changed", () => {
    const first = loadOpsUxRuntimeConfig();
    const second = loadOpsUxRuntimeConfig();

    expect(second).toBe(first);

    const next = withOpsUxControlPatch(DEFAULT_OPSUX_RUNTIME_CONFIG, {
      connectors_hub: true,
    });
    window.localStorage.setItem(STORAGE_KEYS.opsUxRuntimeConfigV1, JSON.stringify(next));

    const changed = loadOpsUxRuntimeConfig();
    const changedAgain = loadOpsUxRuntimeConfig();

    expect(changed).not.toBe(first);
    expect(changedAgain).toBe(changed);
    expect(changed.config.controls.connectors_hub).toBe(true);
  });
});
