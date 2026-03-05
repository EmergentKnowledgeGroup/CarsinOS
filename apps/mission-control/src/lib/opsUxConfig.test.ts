import { describe, expect, it } from "vitest";
import {
  DEFAULT_OPSUX_RUNTIME_CONFIG,
  sanitizeOpsUxRuntimeConfig,
  withOpsUxControlPatch,
} from "./opsUxConfig";

describe("opsUxRuntimeConfig", () => {
  it("keeps approved fail-safe defaults", () => {
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.live_feed_drawer).toBe(false);
    expect(DEFAULT_OPSUX_RUNTIME_CONFIG.controls.incident_auto_trigger).toBe(false);
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
      },
      safety: {
        incident_high_burst_threshold: 8,
        incident_auto_cooldown_ms: -4,
      },
    });

    expect(sanitized.controls.live_feed_drawer).toBe(true);
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
    });

    expect(next.controls.global_kill_switch).toBe(true);
    expect(next.controls.live_feed_drawer).toBe(true);
    expect(next.safety).toBe(originalSafety);
  });
});
