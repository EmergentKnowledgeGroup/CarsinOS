/**
 * Glass Office user configuration: active theme, custom themes, office
 * block layout, and floor overrides. Persisted locally; a server-synced
 * prefs blob can replace the storage layer later without touching callers.
 */

import type { BlockPlacement } from "./blocks";
import type { FloorOverride } from "./floors";
import {
  BUILT_IN_THEMES,
  validateTheme,
  type ThemeDef,
} from "./themes";

export const GLASS_CONFIG_STORAGE_KEY = "mc-glass-config-v1";

/** Fired on window after any Glass config save so live consumers re-read. */
export const GLASS_CONFIG_EVENT = "mc-glass-config-changed";

export function notifyGlassConfigChanged(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new Event(GLASS_CONFIG_EVENT));
}

export interface GlassConfig {
  /** "auto" follows the system preference; otherwise a theme id. */
  themeId: string;
  customThemes: ThemeDef[];
  layout?: BlockPlacement[];
  floorOverrides?: Partial<Record<string, FloorOverride>>;
}

export const DEFAULT_GLASS_CONFIG: GlassConfig = {
  themeId: "auto",
  customThemes: [],
};

export function loadGlassConfig(storage: Storage = localStorage): GlassConfig {
  let raw: string | null = null;
  try {
    raw = storage.getItem(GLASS_CONFIG_STORAGE_KEY);
  } catch {
    return { ...DEFAULT_GLASS_CONFIG };
  }
  if (!raw) return { ...DEFAULT_GLASS_CONFIG };
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { ...DEFAULT_GLASS_CONFIG };
  }
  if (typeof parsed !== "object" || parsed === null) {
    return { ...DEFAULT_GLASS_CONFIG };
  }
  const candidate = parsed as Partial<GlassConfig>;
  const builtInIds = new Set(BUILT_IN_THEMES.map((theme) => theme.id));
  const seenCustomIds = new Set<string>();
  const customThemes = Array.isArray(candidate.customThemes)
    ? candidate.customThemes
        .map((theme) => validateTheme(theme))
        .filter((result): result is { ok: true; theme: ThemeDef } => result.ok)
        .map((result) => result.theme)
        .filter((theme) => {
          if (builtInIds.has(theme.id) || seenCustomIds.has(theme.id)) {
            return false;
          }
          seenCustomIds.add(theme.id);
          return true;
        })
    : [];
  const requestedThemeId =
    typeof candidate.themeId === "string" && candidate.themeId.length > 0
      ? candidate.themeId
      : "auto";
  const knownThemeIds = new Set([
    "auto",
    ...BUILT_IN_THEMES.map((theme) => theme.id),
    ...customThemes.map((theme) => theme.id),
  ]);
  const config: GlassConfig = {
    themeId: knownThemeIds.has(requestedThemeId) ? requestedThemeId : "auto",
    customThemes,
  };
  if (Array.isArray(candidate.layout)) {
    config.layout = candidate.layout.flatMap((entry) => {
      if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
        return [];
      }
      const placement = entry as unknown as Record<string, unknown>;
      if (
        typeof placement.id !== "string" ||
        (placement.size !== "s" && placement.size !== "m" && placement.size !== "l") ||
        typeof placement.visible !== "boolean"
      ) {
        return [];
      }
      return [{ id: placement.id, size: placement.size, visible: placement.visible }];
    });
  }
  if (
    typeof candidate.floorOverrides === "object" &&
    candidate.floorOverrides !== null
  ) {
    const floorOverrides: NonNullable<GlassConfig["floorOverrides"]> = {};
    for (const [floorId, rawOverride] of Object.entries(
      candidate.floorOverrides,
    )) {
      if (typeof rawOverride !== "object" || rawOverride === null) continue;
      const value = rawOverride as Record<string, unknown>;
      const override: FloorOverride = {};
      if (typeof value.hidden === "boolean") override.hidden = value.hidden;
      if (typeof value.label === "string" && value.label.trim()) {
        override.label = value.label.trim();
      }
      if (typeof value.order === "number" && Number.isFinite(value.order)) {
        override.order = value.order;
      }
      if (Object.keys(override).length > 0) floorOverrides[floorId] = override;
    }
    if (Object.keys(floorOverrides).length > 0) {
      config.floorOverrides = floorOverrides;
    }
  }
  return config;
}

export function saveGlassConfig(
  config: GlassConfig,
  storage: Storage = localStorage,
): { ok: boolean; error?: string } {
  try {
    storage.setItem(GLASS_CONFIG_STORAGE_KEY, JSON.stringify(config));
    return { ok: true };
  } catch (error: unknown) {
    return { ok: false, error: String(error) };
  }
}

export interface ThemeResolutionContext {
  prefersDark: boolean;
}

export function resolveActiveTheme(
  config: GlassConfig,
  ctx: ThemeResolutionContext,
): ThemeDef {
  const autoTheme = () => {
    const wanted = ctx.prefersDark ? "carbon-dark" : "porcelain-light";
    const theme = BUILT_IN_THEMES.find((t) => t.id === wanted);
    if (!theme) throw new Error(`built-in theme missing: ${wanted}`);
    return theme;
  };
  if (config.themeId === "auto") return autoTheme();
  const all = [...BUILT_IN_THEMES, ...config.customThemes];
  return all.find((t) => t.id === config.themeId) ?? autoTheme();
}
