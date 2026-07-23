import { beforeEach, describe, expect, test } from "vitest";

import {
  DEFAULT_GLASS_CONFIG,
  loadGlassConfig,
  resolveActiveTheme,
  saveGlassConfig,
  type GlassConfig,
} from "./config";
import { BUILT_IN_THEMES, createCustomTheme } from "./themes";

const porcelain = BUILT_IN_THEMES.find((t) => t.id === "porcelain-light")!;

describe("loadGlassConfig", () => {
  beforeEach(() => localStorage.clear());

  test("returns defaults when nothing is stored", () => {
    const config = loadGlassConfig();
    expect(config).toEqual(DEFAULT_GLASS_CONFIG);
    expect(config.themeId).toBe("auto");
    expect(config.customThemes).toEqual([]);
  });

  test("returns defaults when stored value is corrupt json", () => {
    localStorage.setItem("mc-glass-config-v1", "{not json");
    expect(loadGlassConfig()).toEqual(DEFAULT_GLASS_CONFIG);
  });

  test("round-trips a saved config", () => {
    const custom = createCustomTheme(porcelain, {
      id: "my-cave",
      name: "My Cave",
      tokens: { ground: "#101010" },
    });
    const config: GlassConfig = {
      themeId: "my-cave",
      customThemes: [custom],
      layout: [{ id: "next", size: "l", visible: true }],
      floorOverrides: { window: { hidden: true } },
    };
    const saved = saveGlassConfig(config);
    expect(saved.ok).toBe(true);
    expect(loadGlassConfig()).toEqual(config);
  });

  test("drops invalid custom themes on load instead of failing", () => {
    localStorage.setItem(
      "mc-glass-config-v1",
      JSON.stringify({
        themeId: "auto",
        customThemes: [{ id: "broken", name: "Broken", mode: "light", tokens: {} }],
      }),
    );
    const config = loadGlassConfig();
    expect(config.customThemes).toEqual([]);
  });

  test("keeps floor overrides data-only and drops undeclared shell fields", () => {
    localStorage.setItem(
      "mc-glass-config-v1",
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        floorOverrides: {
          window: {
            hidden: true,
            label: "  Observatory  ",
            order: 2,
            rooms: [{ route: "help" }],
            requiresCapabilities: [],
          },
        },
      }),
    );
    expect(loadGlassConfig().floorOverrides).toEqual({
      window: { hidden: true, label: "Observatory", order: 2 },
    });
  });

  test("drops malformed saved layout entries before a layout consumer can read them", () => {
    localStorage.setItem(
      "mc-glass-config-v1",
      JSON.stringify({
        layout: [
          { id: "next", size: "l", visible: true },
          null,
          42,
          { id: "bad-size", size: "xl", visible: true },
          { id: "missing-visible", size: "s" },
        ],
      }),
    );

    expect(loadGlassConfig().layout).toEqual([
      { id: "next", size: "l", visible: true },
    ]);
  });

  test("rejects persisted custom themes that replace protected tokens", () => {
    const tampered = JSON.parse(JSON.stringify(porcelain)) as {
      id: string;
      name: string;
      tokens: Record<string, string>;
    };
    tampered.id = "tampered";
    tampered.name = "Tampered";
    tampered.tokens.claw = "#00FF00";
    localStorage.setItem(
      "mc-glass-config-v1",
      JSON.stringify({ themeId: tampered.id, customThemes: [tampered] }),
    );

    expect(loadGlassConfig().customThemes).toEqual([]);
  });
});

describe("resolveActiveTheme", () => {
  test("auto follows the system preference", () => {
    const config: GlassConfig = { ...DEFAULT_GLASS_CONFIG, themeId: "auto" };
    expect(resolveActiveTheme(config, { prefersDark: true }).id).toBe(
      "carbon-dark",
    );
    expect(resolveActiveTheme(config, { prefersDark: false }).id).toBe(
      "porcelain-light",
    );
  });

  test("resolves a built-in theme by id", () => {
    const config: GlassConfig = {
      ...DEFAULT_GLASS_CONFIG,
      themeId: "carbon-dark",
    };
    expect(resolveActiveTheme(config, { prefersDark: false }).id).toBe(
      "carbon-dark",
    );
  });

  test("resolves a custom theme by id", () => {
    const custom = createCustomTheme(porcelain, {
      id: "my-cave",
      name: "My Cave",
      tokens: { ground: "#101010" },
    });
    const config: GlassConfig = {
      ...DEFAULT_GLASS_CONFIG,
      themeId: "my-cave",
      customThemes: [custom],
    };
    expect(resolveActiveTheme(config, { prefersDark: true }).id).toBe("my-cave");
  });

  test("falls back to auto behavior for an unknown theme id", () => {
    const config: GlassConfig = { ...DEFAULT_GLASS_CONFIG, themeId: "ghost" };
    expect(resolveActiveTheme(config, { prefersDark: true }).id).toBe(
      "carbon-dark",
    );
  });
});
