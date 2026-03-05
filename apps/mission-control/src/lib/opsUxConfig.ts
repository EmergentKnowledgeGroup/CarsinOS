import { STORAGE_KEYS } from "../storageKeys";

export interface OpsUxFeatureControls {
  global_kill_switch: boolean;
  live_feed_drawer: boolean;
  incident_auto_trigger: boolean;
  usage_charts: boolean;
}

export interface OpsUxSafetyProfile {
  fail_safe_on_config_error: boolean;
  incident_high_burst_threshold: number;
  incident_high_burst_window_ms: number;
  incident_auto_cooldown_ms: number;
  incident_health_degraded_trigger_ms: number;
  incident_healthy_exit_ms: number;
  recovery_retention_window_ms: number;
  recovery_log_max_bytes: number;
  mark_read_undo_window_ms: number;
}

export interface OpsUxRuntimeConfig {
  schema_version: "mc-opsux-runtime-v1";
  controls: OpsUxFeatureControls;
  safety: OpsUxSafetyProfile;
}

export interface LoadedOpsUxRuntimeConfig {
  config: OpsUxRuntimeConfig;
  degraded: boolean;
  error: string | null;
}

const MB = 1024 * 1024;

export const DEFAULT_OPSUX_RUNTIME_CONFIG: OpsUxRuntimeConfig = {
  schema_version: "mc-opsux-runtime-v1",
  controls: {
    // Approved defaults: optional modules start off and are enabled deliberately.
    global_kill_switch: false,
    live_feed_drawer: false,
    incident_auto_trigger: false,
    usage_charts: false,
  },
  safety: {
    // Approved defaults from operator guidance.
    fail_safe_on_config_error: true,
    incident_high_burst_threshold: 5,
    incident_high_burst_window_ms: 60_000,
    incident_auto_cooldown_ms: 10 * 60_000,
    incident_health_degraded_trigger_ms: 30_000,
    incident_healthy_exit_ms: 5 * 60_000,
    recovery_retention_window_ms: 30 * 60_000,
    recovery_log_max_bytes: 50 * MB,
    mark_read_undo_window_ms: 5 * 60_000,
  },
};

function coerceBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function coercePositiveInt(value: unknown, fallback: number, min = 1): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  const rounded = Math.floor(value);
  return rounded >= min ? rounded : fallback;
}

function readStorage(): Storage | null {
  try {
    if (typeof window === "undefined") {
      return null;
    }
    return window.localStorage;
  } catch {
    return null;
  }
}

export function sanitizeOpsUxRuntimeConfig(raw: unknown): OpsUxRuntimeConfig {
  const source = typeof raw === "object" && raw !== null ? (raw as Record<string, unknown>) : {};
  const controlsRaw =
    typeof source.controls === "object" && source.controls !== null
      ? (source.controls as Record<string, unknown>)
      : {};
  const safetyRaw =
    typeof source.safety === "object" && source.safety !== null
      ? (source.safety as Record<string, unknown>)
      : {};

  return {
    schema_version: "mc-opsux-runtime-v1",
    controls: {
      global_kill_switch: coerceBoolean(
        controlsRaw.global_kill_switch,
        DEFAULT_OPSUX_RUNTIME_CONFIG.controls.global_kill_switch
      ),
      live_feed_drawer: coerceBoolean(
        controlsRaw.live_feed_drawer,
        DEFAULT_OPSUX_RUNTIME_CONFIG.controls.live_feed_drawer
      ),
      incident_auto_trigger: coerceBoolean(
        controlsRaw.incident_auto_trigger,
        DEFAULT_OPSUX_RUNTIME_CONFIG.controls.incident_auto_trigger
      ),
      usage_charts: coerceBoolean(
        controlsRaw.usage_charts,
        DEFAULT_OPSUX_RUNTIME_CONFIG.controls.usage_charts
      ),
    },
    safety: {
      fail_safe_on_config_error: coerceBoolean(
        safetyRaw.fail_safe_on_config_error,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.fail_safe_on_config_error
      ),
      incident_high_burst_threshold: coercePositiveInt(
        safetyRaw.incident_high_burst_threshold,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_high_burst_threshold
      ),
      incident_high_burst_window_ms: coercePositiveInt(
        safetyRaw.incident_high_burst_window_ms,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_high_burst_window_ms
      ),
      incident_auto_cooldown_ms: coercePositiveInt(
        safetyRaw.incident_auto_cooldown_ms,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_auto_cooldown_ms
      ),
      incident_health_degraded_trigger_ms: coercePositiveInt(
        safetyRaw.incident_health_degraded_trigger_ms,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_health_degraded_trigger_ms
      ),
      incident_healthy_exit_ms: coercePositiveInt(
        safetyRaw.incident_healthy_exit_ms,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.incident_healthy_exit_ms
      ),
      recovery_retention_window_ms: coercePositiveInt(
        safetyRaw.recovery_retention_window_ms,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.recovery_retention_window_ms
      ),
      recovery_log_max_bytes: coercePositiveInt(
        safetyRaw.recovery_log_max_bytes,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.recovery_log_max_bytes,
        1024
      ),
      mark_read_undo_window_ms: coercePositiveInt(
        safetyRaw.mark_read_undo_window_ms,
        DEFAULT_OPSUX_RUNTIME_CONFIG.safety.mark_read_undo_window_ms
      ),
    },
  };
}

export function loadOpsUxRuntimeConfig(): LoadedOpsUxRuntimeConfig {
  const storage = readStorage();
  if (!storage) {
    return {
      config: DEFAULT_OPSUX_RUNTIME_CONFIG,
      degraded: true,
      error: "Local storage unavailable; running in fail-safe defaults.",
    };
  }

  const raw = storage.getItem(STORAGE_KEYS.opsUxRuntimeConfigV1);
  if (!raw) {
    return {
      config: DEFAULT_OPSUX_RUNTIME_CONFIG,
      degraded: false,
      error: null,
    };
  }

  try {
    const parsed = JSON.parse(raw) as unknown;
    return {
      config: sanitizeOpsUxRuntimeConfig(parsed),
      degraded: false,
      error: null,
    };
  } catch {
    return {
      config: DEFAULT_OPSUX_RUNTIME_CONFIG,
      degraded: true,
      error: "Runtime config was invalid; fail-safe defaults were applied.",
    };
  }
}

export function saveOpsUxRuntimeConfig(config: OpsUxRuntimeConfig): { ok: boolean; error: string | null } {
  const storage = readStorage();
  if (!storage) {
    return { ok: false, error: "Local storage unavailable; config not persisted." };
  }

  try {
    storage.setItem(STORAGE_KEYS.opsUxRuntimeConfigV1, JSON.stringify(config));
    return { ok: true, error: null };
  } catch {
    return { ok: false, error: "Failed to persist runtime config to local storage." };
  }
}

export function withOpsUxControlPatch(
  config: OpsUxRuntimeConfig,
  patch: Partial<OpsUxFeatureControls>
): OpsUxRuntimeConfig {
  return {
    ...config,
    controls: {
      ...config.controls,
      ...patch,
    },
  };
}
