// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  loadGlassConfig,
  notifyGlassConfigChanged,
  saveGlassConfig,
} from "../../glass/config";
import { PinRoomToOffice } from "./PinRoomToOffice";

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
  localStorage.clear();
});

async function render(roomId: string) {
  await act(async () => {
    root = createRoot(container);
    root.render(<PinRoomToOffice roomId={roomId} />);
  });
}

function pinButton(): HTMLButtonElement | undefined {
  return Array.from(container.querySelectorAll("button")).find((button) =>
    button.textContent?.includes("Pin to Office"),
  );
}

describe("PinRoomToOffice", () => {
  it("pins the room's registered blocks and confirms only after the save succeeded", async () => {
    await render("boards");
    const button = pinButton();
    expect(button).toBeTruthy();
    expect(button?.getAttribute("aria-label")).toBe("Pin Boards to Office");

    await act(async () => button!.click());

    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "boards")?.visible).toBe(true);
    const status = container.querySelector('[role="status"]');
    expect(status?.textContent).toContain("On the Office canvas");
  });

  it("reports an already-pinned room honestly instead of pretending a change", async () => {
    await render("boards");
    await act(async () => pinButton()!.click());
    await act(async () => pinButton()!.click());
    const status = container.querySelector('[role="status"]');
    expect(status?.textContent).toContain("Already on the Office canvas");
  });

  it("renders nothing for rooms that cannot be pinned yet", async () => {
    await render("staff");
    expect(container.querySelector("button")).toBeNull();
  });

  it("clears stale success copy after the shortcut is hidden elsewhere", async () => {
    await render("boards");
    await act(async () => pinButton()!.click());
    expect(container.querySelector('[role="status"]')).not.toBeNull();

    const config = loadGlassConfig();
    const layout = (config.layout ?? []).map((placement) =>
      placement.id === "boards"
        ? { ...placement, visible: false }
        : placement,
    );
    await act(async () => {
      expect(saveGlassConfig({ ...config, layout }).ok).toBe(true);
      notifyGlassConfigChanged();
    });
    expect(container.querySelector('[role="status"]')).toBeNull();
  });
});
