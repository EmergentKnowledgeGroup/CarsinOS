/**
 * Themes are data: a theme is a bag of named color/style tokens applied as
 * CSS custom properties. Components consume tokens only — never hardcoded
 * colors. Users can build custom themes from any base; locked tokens
 * (Claw Orange and friends) survive every override.
 */

export type ThemeMode = "light" | "dark";

export interface ThemeDef {
  id: string;
  name: string;
  mode: ThemeMode;
  tokens: Record<string, string>;
}

/** Tokens a custom theme may never override. Claw Orange is constitutional. */
export const LOCKED_TOKENS: readonly string[] = [
  "claw",
  "claw-soft",
  "ok",
  "warn",
];

export const REQUIRED_TOKEN_KEYS: readonly string[] = [
  "ground",
  "surface",
  "glass",
  "ink",
  "ink-soft",
  "ink-faint",
  "line",
  "line-soft",
  "accent",
  "accent-soft",
  "accent-ink",
  "claw",
  "claw-soft",
  "gold",
  "gold-soft",
  "gold-line",
  "ok",
  "warn",
  "shadow",
  "shadow-pop",
];

export const BUILT_IN_THEMES: ThemeDef[] = [
  {
    id: "porcelain-light",
    name: "Porcelain (Light)",
    mode: "light",
    tokens: {
      ground: "#F6F5F1",
      surface: "#FFFFFF",
      glass: "rgba(255,255,255,0.74)",
      ink: "#21282B",
      "ink-soft": "#596467",
      "ink-faint": "#8C9598",
      line: "#E5E1D8",
      "line-soft": "#EEEBE3",
      accent: "#17766B",
      "accent-soft": "#E2EFEB",
      "accent-ink": "#0E5A50",
      claw: "#E8541B",
      "claw-soft": "#FDE8DD",
      gold: "#97782A",
      "gold-soft": "#F6EED9",
      "gold-line": "#E2D4AB",
      ok: "#3E7A4E",
      warn: "#A8402F",
      shadow: "0 1px 2px rgba(33,40,43,.05), 0 8px 26px rgba(33,40,43,.07)",
      "shadow-pop":
        "0 4px 12px rgba(33,40,43,.12), 0 24px 60px rgba(33,40,43,.2)",
    },
  },
  {
    id: "carbon-dark",
    name: "Carbon (After Hours)",
    mode: "dark",
    tokens: {
      ground: "#0C0C0F",
      surface: "#17171B",
      glass: "rgba(23,23,27,0.82)",
      ink: "#EAE8E2",
      "ink-soft": "#A8A9AC",
      "ink-faint": "#6F7074",
      line: "#29292F",
      "line-soft": "#1F1F24",
      accent: "#4FB3A5",
      "accent-soft": "#1A2F2C",
      "accent-ink": "#7CC9BE",
      claw: "#FF5A1F",
      "claw-soft": "#351D10",
      gold: "#CFAE5E",
      "gold-soft": "#2A2516",
      "gold-line": "#493E21",
      ok: "#6FAE7E",
      warn: "#D3705F",
      shadow: "0 1px 2px rgba(0,0,0,.3), 0 8px 26px rgba(0,0,0,.35)",
      "shadow-pop": "0 4px 12px rgba(0,0,0,.4), 0 24px 60px rgba(0,0,0,.55)",
    },
  },
];

export interface CustomThemePatch {
  id: string;
  name: string;
  mode?: ThemeMode;
  tokens?: Record<string, string>;
}

export function createCustomTheme(
  base: ThemeDef,
  patch: CustomThemePatch,
): ThemeDef {
  const tokens = { ...base.tokens };
  for (const [key, value] of Object.entries(patch.tokens ?? {})) {
    if (LOCKED_TOKENS.includes(key)) continue;
    tokens[key] = value;
  }
  return {
    id: patch.id,
    name: patch.name,
    mode: patch.mode ?? base.mode,
    tokens,
  };
}

export type ThemeValidation =
  | { ok: true; theme: ThemeDef }
  | { ok: false; errors: string[] };

export function validateTheme(input: unknown): ThemeValidation {
  const errors: string[] = [];
  if (typeof input !== "object" || input === null || Array.isArray(input)) {
    return { ok: false, errors: ["theme must be an object"] };
  }
  const candidate = input as Partial<ThemeDef> & {
    tokens?: unknown;
  };
  if (typeof candidate.id !== "string" || candidate.id.length === 0) {
    errors.push("id is required");
  }
  if (typeof candidate.name !== "string" || candidate.name.length === 0) {
    errors.push("name is required");
  }
  if (candidate.mode !== "light" && candidate.mode !== "dark") {
    errors.push("mode must be 'light' or 'dark'");
  }
  const tokens = candidate.tokens;
  if (typeof tokens !== "object" || tokens === null || Array.isArray(tokens)) {
    errors.push("tokens must be an object");
  } else {
    for (const key of REQUIRED_TOKEN_KEYS) {
      const value = (tokens as Record<string, unknown>)[key];
      if (typeof value !== "string" || value.length === 0) {
        errors.push(`missing token: ${key}`);
      }
    }
    if (candidate.mode === "light" || candidate.mode === "dark") {
      const canonical = BUILT_IN_THEMES.find(
        (theme) => theme.mode === candidate.mode,
      );
      for (const key of LOCKED_TOKENS) {
        if (canonical && (tokens as Record<string, unknown>)[key] !== canonical.tokens[key]) {
          errors.push(`protected token changed: ${key}`);
        }
      }
    }
  }
  if (errors.length > 0) return { ok: false, errors };
  return { ok: true, theme: candidate as ThemeDef };
}

export function applyTheme(root: HTMLElement, theme: ThemeDef): void {
  for (const [key, value] of Object.entries(theme.tokens)) {
    root.style.setProperty(`--${key}`, value);
  }
  root.dataset.theme = theme.mode;
  root.dataset.themeId = theme.id;
}
