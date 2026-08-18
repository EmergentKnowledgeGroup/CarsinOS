import { describe, expect, it } from "vitest";

import { DEFAULT_GLASS_CONFIG, type GlassConfig } from "./config";
import {
  activeThemeName,
  deleteCustomTheme,
  duplicateTheme,
  exportThemeJson,
  importThemeJson,
  setDraftMode,
  setDraftToken,
  tokenInputKind,
  upsertCustomTheme,
} from "./themeEditor";
import { BUILT_IN_THEMES, type ThemeDef } from "./themes";

function porcelain(): ThemeDef {
  const theme = BUILT_IN_THEMES.find((t) => t.id === "porcelain-light");
  if (!theme) throw new Error("porcelain-light missing");
  return theme;
}

function carbon(): ThemeDef {
  const theme = BUILT_IN_THEMES.find((t) => t.id === "carbon-dark");
  if (!theme) throw new Error("carbon-dark missing");
  return theme;
}

describe("duplicateTheme", () => {
  it("copies tokens and derives a unique id and name", () => {
    const copy = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    expect(copy.id).toBe("porcelain-light-copy");
    expect(copy.name).toBe("Porcelain (Light) Copy");
    expect(copy.mode).toBe("light");
    expect(copy.tokens).toEqual(porcelain().tokens);
    expect(copy.tokens).not.toBe(porcelain().tokens);
  });

  it("keeps suffixing until the id is unique", () => {
    const first = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const second = duplicateTheme(porcelain(), [...BUILT_IN_THEMES, first]);
    expect(second.id).toBe("porcelain-light-copy-2");
    expect(second.name).toBe("Porcelain (Light) Copy 2");
  });
});

describe("setDraftToken", () => {
  it("updates an editable token", () => {
    const draft = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const next = setDraftToken(draft, "accent", "#123456");
    expect(next.tokens.accent).toBe("#123456");
    expect(draft.tokens.accent).toBe("#17766B");
  });

  it("refuses to change a protected token", () => {
    const draft = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const next = setDraftToken(draft, "claw", "#000000");
    expect(next).toBe(draft);
  });
});

describe("setDraftMode", () => {
  it("re-seeds protected tokens from the canonical theme of the new mode", () => {
    const draft = setDraftToken(
      duplicateTheme(porcelain(), BUILT_IN_THEMES),
      "ground",
      "#101010",
    );
    const dark = setDraftMode(draft, "dark");
    expect(dark.mode).toBe("dark");
    expect(dark.tokens.claw).toBe(carbon().tokens.claw);
    expect(dark.tokens.ok).toBe(carbon().tokens.ok);
    expect(dark.tokens.warn).toBe(carbon().tokens.warn);
    expect(dark.tokens["claw-soft"]).toBe(carbon().tokens["claw-soft"]);
    expect(dark.tokens.ground).toBe("#101010");
  });

  it("returns the draft unchanged when the mode already matches", () => {
    const draft = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    expect(setDraftMode(draft, "light")).toBe(draft);
  });
});

describe("export/import round trip", () => {
  it("round-trips a valid theme through JSON", () => {
    const theme = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const result = importThemeJson(exportThemeJson(theme));
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.theme).toEqual(theme);
  });

  it("reports unparseable JSON honestly", () => {
    const result = importThemeJson("{not json");
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors[0]).toMatch(/not valid JSON/i);
    }
  });

  it("surfaces validation errors for incomplete themes", () => {
    const raw = JSON.stringify({ id: "x", name: "X", mode: "light", tokens: {} });
    const result = importThemeJson(raw);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.some((e) => e.includes("missing token"))).toBe(true);
    }
  });

  it("refuses an id that collides with a built-in theme", () => {
    const clone: ThemeDef = { ...porcelain(), tokens: { ...porcelain().tokens } };
    const result = importThemeJson(JSON.stringify(clone));
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.some((e) => e.includes("built-in"))).toBe(true);
    }
  });
});

describe("upsertCustomTheme", () => {
  it("appends a new custom theme", () => {
    const theme = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const config = upsertCustomTheme({ ...DEFAULT_GLASS_CONFIG }, theme);
    expect(config.customThemes.map((t) => t.id)).toEqual([theme.id]);
  });

  it("replaces an existing custom theme with the same id", () => {
    const theme = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const first = upsertCustomTheme({ ...DEFAULT_GLASS_CONFIG }, theme);
    const edited = setDraftToken(theme, "accent", "#ABCDEF");
    const second = upsertCustomTheme(first, edited);
    expect(second.customThemes).toHaveLength(1);
    expect(second.customThemes[0]?.tokens.accent).toBe("#ABCDEF");
  });

  it("refuses to shadow a built-in theme id", () => {
    const clone: ThemeDef = { ...porcelain(), tokens: { ...porcelain().tokens } };
    const config = upsertCustomTheme({ ...DEFAULT_GLASS_CONFIG }, clone);
    expect(config.customThemes).toHaveLength(0);
  });
});

describe("deleteCustomTheme", () => {
  it("removes the theme and keeps an unrelated active selection", () => {
    const theme = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const config: GlassConfig = {
      ...upsertCustomTheme({ ...DEFAULT_GLASS_CONFIG }, theme),
      themeId: "carbon-dark",
    };
    const next = deleteCustomTheme(config, theme.id);
    expect(next.customThemes).toHaveLength(0);
    expect(next.themeId).toBe("carbon-dark");
  });

  it("falls back to auto when deleting the active theme", () => {
    const theme = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    const config: GlassConfig = {
      ...upsertCustomTheme({ ...DEFAULT_GLASS_CONFIG }, theme),
      themeId: theme.id,
    };
    expect(deleteCustomTheme(config, theme.id).themeId).toBe("auto");
  });
});

describe("activeThemeName", () => {
  it("names auto Follow system and resolves ids to display names", () => {
    expect(activeThemeName({ ...DEFAULT_GLASS_CONFIG })).toBe("Follow system");
    expect(
      activeThemeName({ ...DEFAULT_GLASS_CONFIG, themeId: "carbon-dark" }),
    ).toBe("Carbon (After Hours)");
    const custom = duplicateTheme(porcelain(), BUILT_IN_THEMES);
    expect(
      activeThemeName({
        ...upsertCustomTheme({ ...DEFAULT_GLASS_CONFIG }, custom),
        themeId: custom.id,
      }),
    ).toBe(custom.name);
    expect(
      activeThemeName({ ...DEFAULT_GLASS_CONFIG, themeId: "ghost" }),
    ).toBe("Follow system");
  });
});

describe("tokenInputKind", () => {
  it("uses color inputs for plain color tokens", () => {
    expect(tokenInputKind("accent")).toBe("color");
    expect(tokenInputKind("ground")).toBe("color");
  });

  it("uses text inputs for compound values", () => {
    expect(tokenInputKind("glass")).toBe("text");
    expect(tokenInputKind("shadow")).toBe("text");
    expect(tokenInputKind("shadow-pop")).toBe("text");
  });
});
