/**
 * Theme editor session logic: drafts are plain ThemeDefs edited copy-on-write.
 * Protected tokens (Claw Orange and the semantic safety colors) can never be
 * written, and switching a draft's mode re-seeds them from the canonical
 * built-in of that mode so the result always validates.
 */

import type { GlassConfig } from "./config";
import {
  BUILT_IN_THEMES,
  LOCKED_TOKENS,
  validateTheme,
  type ThemeDef,
  type ThemeMode,
} from "./themes";

function canonicalFor(mode: ThemeMode): ThemeDef {
  const theme = BUILT_IN_THEMES.find((t) => t.mode === mode);
  if (!theme) throw new Error(`no built-in theme for mode: ${mode}`);
  return theme;
}

function uniqueSuffix(
  base: string,
  taken: ReadonlySet<string>,
  join: (base: string, n: number) => string,
): string {
  if (!taken.has(join(base, 1))) return join(base, 1);
  let n = 2;
  while (taken.has(join(base, n))) n += 1;
  return join(base, n);
}

/** Copy a theme into a new editable custom theme with a unique id/name. */
export function duplicateTheme(
  source: ThemeDef,
  existing: readonly ThemeDef[],
): ThemeDef {
  const ids = new Set(existing.map((t) => t.id));
  const names = new Set(existing.map((t) => t.name));
  return {
    id: uniqueSuffix(source.id, ids, (base, n) =>
      n === 1 ? `${base}-copy` : `${base}-copy-${n}`,
    ),
    name: uniqueSuffix(source.name, names, (base, n) =>
      n === 1 ? `${base} Copy` : `${base} Copy ${n}`,
    ),
    mode: source.mode,
    tokens: { ...source.tokens },
  };
}

/** Set one token on a draft. Protected tokens are refused unchanged. */
export function setDraftToken(
  draft: ThemeDef,
  key: string,
  value: string,
): ThemeDef {
  if (LOCKED_TOKENS.includes(key)) return draft;
  return { ...draft, tokens: { ...draft.tokens, [key]: value } };
}

/**
 * Switch a draft between light and dark. Protected tokens are re-seeded from
 * the canonical built-in of the target mode; everything else is kept.
 */
export function setDraftMode(draft: ThemeDef, mode: ThemeMode): ThemeDef {
  if (draft.mode === mode) return draft;
  const canonical = canonicalFor(mode);
  const tokens = { ...draft.tokens };
  for (const key of LOCKED_TOKENS) {
    const value = canonical.tokens[key];
    if (value !== undefined) tokens[key] = value;
  }
  return { ...draft, mode, tokens };
}

export function exportThemeJson(theme: ThemeDef): string {
  return JSON.stringify(theme, null, 2);
}

export type ThemeImportResult =
  | { ok: true; theme: ThemeDef }
  | { ok: false; errors: string[] };

export function importThemeJson(raw: string): ThemeImportResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { ok: false, errors: ["the file is not valid JSON"] };
  }
  const result = validateTheme(parsed);
  if (!result.ok) return result;
  if (BUILT_IN_THEMES.some((t) => t.id === result.theme.id)) {
    return {
      ok: false,
      errors: [`id "${result.theme.id}" belongs to a built-in theme`],
    };
  }
  return result;
}

/**
 * Add or replace a custom theme in the config. Invalid themes and themes that
 * would shadow a built-in id are refused; the config is returned unchanged.
 */
export function upsertCustomTheme(
  config: GlassConfig,
  theme: ThemeDef,
): GlassConfig {
  if (!validateTheme(theme).ok) return config;
  if (BUILT_IN_THEMES.some((t) => t.id === theme.id)) return config;
  const customThemes = config.customThemes.some((t) => t.id === theme.id)
    ? config.customThemes.map((t) => (t.id === theme.id ? theme : t))
    : [...config.customThemes, theme];
  return { ...config, customThemes };
}

/** Remove a custom theme; the active selection falls back to auto if needed. */
export function deleteCustomTheme(config: GlassConfig, id: string): GlassConfig {
  return {
    ...config,
    customThemes: config.customThemes.filter((t) => t.id !== id),
    themeId: config.themeId === id ? "auto" : config.themeId,
  };
}

/** Display name for the active selection; unknown ids read as Follow system. */
export function activeThemeName(config: GlassConfig): string {
  if (config.themeId === "auto") return "Follow system";
  const theme = [...BUILT_IN_THEMES, ...config.customThemes].find(
    (t) => t.id === config.themeId,
  );
  return theme?.name ?? "Follow system";
}

/** Compound values (rgba glass, shadow stacks) need free text, not a picker. */
export function tokenInputKind(key: string): "color" | "text" {
  return key === "glass" || key.startsWith("shadow") ? "text" : "color";
}
