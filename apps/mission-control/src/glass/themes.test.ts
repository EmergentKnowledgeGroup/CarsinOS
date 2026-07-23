import { describe, expect, test } from "vitest";

import {
  BUILT_IN_THEMES,
  LOCKED_TOKENS,
  REQUIRED_TOKEN_KEYS,
  applyTheme,
  createCustomTheme,
  validateTheme,
} from "./themes";

const porcelain = () => {
  const theme = BUILT_IN_THEMES.find((t) => t.id === "porcelain-light");
  if (!theme) throw new Error("porcelain-light missing");
  return theme;
};

describe("built-in themes", () => {
  test("ships porcelain-light and carbon-dark", () => {
    const ids = BUILT_IN_THEMES.map((t) => t.id);
    expect(ids).toContain("porcelain-light");
    expect(ids).toContain("carbon-dark");
    expect(BUILT_IN_THEMES.find((t) => t.id === "carbon-dark")?.mode).toBe(
      "dark",
    );
    expect(BUILT_IN_THEMES.find((t) => t.id === "porcelain-light")?.mode).toBe(
      "light",
    );
  });

  test("every built-in defines every required token key", () => {
    for (const theme of BUILT_IN_THEMES) {
      for (const key of REQUIRED_TOKEN_KEYS) {
        expect(theme.tokens[key], `${theme.id} missing ${key}`).toBeTruthy();
      }
    }
  });

  test("claw orange is a locked token present in every built-in", () => {
    expect(LOCKED_TOKENS).toContain("claw");
    for (const theme of BUILT_IN_THEMES) {
      expect(theme.tokens.claw).toBeTruthy();
    }
  });
});

describe("createCustomTheme", () => {
  test("applies user token overrides on top of the base theme", () => {
    const custom = createCustomTheme(porcelain(), {
      id: "my-cave",
      name: "My Cave",
      tokens: { ground: "#123456" },
    });
    expect(custom.id).toBe("my-cave");
    expect(custom.tokens.ground).toBe("#123456");
    expect(custom.tokens.surface).toBe(porcelain().tokens.surface);
  });

  test("silently refuses to override locked tokens (claw orange is constitutional)", () => {
    const custom = createCustomTheme(porcelain(), {
      id: "no-crab",
      name: "No Crab",
      tokens: { claw: "#00FF00" },
    });
    expect(custom.tokens.claw).toBe(porcelain().tokens.claw);
  });

  test("refuses to override any protected semantic token", () => {
    const custom = createCustomTheme(porcelain(), {
      id: "safe-statuses",
      name: "Safe statuses",
      tokens: { "claw-soft": "#000", ok: "#000", warn: "#000" },
    });
    for (const token of LOCKED_TOKENS) {
      expect(custom.tokens[token]).toBe(porcelain().tokens[token]);
    }
  });
});

describe("validateTheme", () => {
  test("accepts a round-tripped built-in theme", () => {
    const parsed = validateTheme(JSON.parse(JSON.stringify(porcelain())));
    expect(parsed.ok).toBe(true);
  });

  test("rejects a theme missing required keys, naming the missing key", () => {
    const broken = JSON.parse(JSON.stringify(porcelain())) as {
      tokens: Record<string, string>;
    };
    delete broken.tokens.ground;
    const parsed = validateTheme(broken);
    expect(parsed.ok).toBe(false);
    if (!parsed.ok) {
      expect(parsed.errors.join(" ")).toContain("ground");
    }
  });

  test("rejects a persisted theme that changes protected tokens", () => {
    const tampered = JSON.parse(JSON.stringify(porcelain())) as {
      tokens: Record<string, string>;
    };
    tampered.tokens.warn = "#00FF00";
    const parsed = validateTheme(tampered);
    expect(parsed.ok).toBe(false);
    if (!parsed.ok) {
      expect(parsed.errors).toContain("protected token changed: warn");
    }
  });

  test("rejects non-object input", () => {
    expect(validateTheme("nope").ok).toBe(false);
    expect(validateTheme(null).ok).toBe(false);
  });
});

describe("applyTheme", () => {
  test("sets css custom properties, mode, and theme id on the root element", () => {
    const root = document.createElement("div");
    applyTheme(root, porcelain());
    expect(root.style.getPropertyValue("--ground")).toBe(
      porcelain().tokens.ground,
    );
    expect(root.style.getPropertyValue("--claw")).toBe(porcelain().tokens.claw);
    expect(root.dataset.theme).toBe("light");
    expect(root.dataset.themeId).toBe("porcelain-light");
  });

  test("switching themes replaces previous token values", () => {
    const root = document.createElement("div");
    const carbon = BUILT_IN_THEMES.find((t) => t.id === "carbon-dark");
    if (!carbon) throw new Error("carbon-dark missing");
    applyTheme(root, porcelain());
    applyTheme(root, carbon);
    expect(root.style.getPropertyValue("--ground")).toBe(carbon.tokens.ground);
    expect(root.dataset.theme).toBe("dark");
  });
});
