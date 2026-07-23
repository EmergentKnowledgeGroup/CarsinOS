// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  GLASS_CONFIG_STORAGE_KEY,
  loadGlassConfig,
} from "../../glass/config";
import { BUILT_IN_THEMES } from "../../glass/themes";
import { ThemeStudio } from "./ThemeStudio";

let root: Root | null = null;
let container: HTMLDivElement;

async function mount() {
  container = document.createElement("div");
  document.body.appendChild(container);
  await act(async () => {
    root = createRoot(container);
    root.render(<ThemeStudio />);
  });
}

function findButton(label: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find(
    (b) => b.getAttribute("aria-label") === label || b.textContent === label,
  );
  expect(button, `button not found: ${label}`).toBeDefined();
  return button as HTMLButtonElement;
}

function click(label: string) {
  return act(async () => findButton(label).click());
}

function setInput(element: Element | null, value: string) {
  expect(element).not.toBeNull();
  const input = element as HTMLInputElement | HTMLTextAreaElement;
  const proto = Object.getPrototypeOf(input) as object;
  const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
  setter?.call(input, value);
  return act(async () => {
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(() => {
  localStorage.clear();
});

afterEach(async () => {
  await act(async () => root?.unmount());
  container.remove();
  root = null;
  localStorage.clear();
});

describe("ThemeStudio", () => {
  it("lists built-in themes with Follow system active by default", async () => {
    await mount();
    for (const theme of BUILT_IN_THEMES) {
      expect(container.textContent).toContain(theme.name);
    }
    const active = container.querySelector("[data-testid='theme-active']");
    expect(active?.textContent).toContain("Follow system");
  });

  it("activates a theme and persists the selection", async () => {
    await mount();
    await click("Use Carbon (After Hours)");
    expect(loadGlassConfig().themeId).toBe("carbon-dark");
    const active = container.querySelector("[data-testid='theme-active']");
    expect(active?.textContent).toContain("Carbon (After Hours)");
  });

  it("duplicates a built-in into a persisted custom theme and opens the editor", async () => {
    await mount();
    await click("Duplicate Porcelain (Light)");
    const saved = loadGlassConfig().customThemes;
    expect(saved).toHaveLength(1);
    expect(saved[0]?.id).toBe("porcelain-light-copy");
    expect(
      container.querySelector("[data-testid='theme-editor']"),
    ).not.toBeNull();
  });

  it("disables protected tokens and persists edits to editable ones", async () => {
    await mount();
    await click("Duplicate Porcelain (Light)");
    const clawInput = container.querySelector("[data-token='claw'] input");
    expect((clawInput as HTMLInputElement).disabled).toBe(true);

    const accentInput = container.querySelector("[data-token='accent'] input");
    await setInput(accentInput, "#123456");
    await click("Save theme");
    const saved = loadGlassConfig().customThemes[0];
    expect(saved?.tokens.accent).toBe("#123456");
    expect(saved?.tokens.claw).toBe("#E8541B");
  });

  it("applies draft tokens to the live preview", async () => {
    await mount();
    await click("Duplicate Porcelain (Light)");
    const accentInput = container.querySelector("[data-token='accent'] input");
    await setInput(accentInput, "#123456");
    const preview = container.querySelector(
      "[data-testid='theme-preview']",
    ) as HTMLElement;
    expect(preview.style.getPropertyValue("--accent")).toBe("#123456");
    expect(preview.style.getPropertyValue("--claw")).toBe("#E8541B");
  });

  it("imports a valid theme JSON and reports broken JSON honestly", async () => {
    await mount();
    const textarea = container.querySelector(
      "[data-testid='theme-transfer'] textarea",
    );
    await setInput(textarea, "{broken");
    await click("Import theme");
    expect(container.textContent).toContain("not valid JSON");

    const valid = {
      ...BUILT_IN_THEMES[0],
      id: "my-cave",
      name: "My Cave",
      tokens: { ...BUILT_IN_THEMES[0]!.tokens },
    };
    await setInput(textarea, JSON.stringify(valid));
    await click("Import theme");
    expect(loadGlassConfig().customThemes.map((t) => t.id)).toContain(
      "my-cave",
    );
  });

  it("deleting the active custom theme falls back to Follow system", async () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "my-cave",
        customThemes: [
          {
            ...BUILT_IN_THEMES[0],
            id: "my-cave",
            name: "My Cave",
            tokens: { ...BUILT_IN_THEMES[0]!.tokens },
          },
        ],
      }),
    );
    await mount();
    await click("Delete My Cave");
    expect(loadGlassConfig().customThemes).toHaveLength(0);
    expect(loadGlassConfig().themeId).toBe("auto");
    const active = container.querySelector("[data-testid='theme-active']");
    expect(active?.textContent).toContain("Follow system");
  });
});
