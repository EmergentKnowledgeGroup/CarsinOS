// @vitest-environment jsdom

import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { GLASS_CONFIG_STORAGE_KEY, loadGlassConfig } from "../../glass/config";
import { fixtureSummaryResponse } from "../../glass/execass/fixtures";
import { ExecassOfficePanel } from "./ExecassOfficePanel";
import type { ExecassOfficeController } from "./useExecassOfficeController";

let root: Root | null = null;
let container: HTMLDivElement;

function makeController(): ExecassOfficeController {
  return {
    summary: fixtureSummaryResponse(),
    summaryLoading: false,
    summaryError: null,
    briefing: null,
    stopAll: null,
    trayNotes: [],
    resolvingDecisionIds: [],
    intakeBusy: false,
    freezeBusy: false,
    conversationalReply: null,
    resolveAttention: vi.fn(),
    delegate: vi.fn(),
    clearConversationalReply: vi.fn(),
    freezeAll: vi.fn(),
    resumeAllWork: vi.fn(),
    loadReceipts: vi.fn(),
    dismissTrayNote: vi.fn(),
  } as unknown as ExecassOfficeController;
}

async function mount() {
  container = document.createElement("div");
  document.body.appendChild(container);
  await act(async () => {
    root = createRoot(container);
    root.render(
      <StrictMode>
        <ExecassOfficePanel controller={makeController()} onOpenRoom={() => {}} />
      </StrictMode>,
    );
  });
}

function blockIds(): string[] {
  return Array.from(
    container.querySelectorAll("[data-testid^='office-block-']"),
  ).map((el) => el.getAttribute("data-testid")!.replace("office-block-", ""));
}

function click(label: string) {
  const button = Array.from(container.querySelectorAll("button")).find(
    (b) => b.getAttribute("aria-label") === label || b.textContent === label,
  );
  expect(button, `button not found: ${label}`).toBeDefined();
  return act(async () => button!.click());
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

describe("Office block canvas", () => {
  it("renders blocks in the saved order and skips hidden blocks", async () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "next", size: "s", visible: true },
          { id: "done", size: "m", visible: true },
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: true },
        ],
      }),
    );
    await mount();
    expect(blockIds()).toEqual(["next", "done", "in-motion"]);
  });

  it("reorders a block in arrange mode and persists the layout", async () => {
    await mount();
    await click("Arrange office");
    await click("Move In motion earlier");
    expect(blockIds()[0]).toBe("in-motion");
    expect(loadGlassConfig().layout?.[0]?.id).toBe("in-motion");
  });

  it("cycles a block size in arrange mode and persists it", async () => {
    await mount();
    await click("Arrange office");
    await click("Resize Next");
    const saved = loadGlassConfig().layout?.find((p) => p.id === "next");
    expect(saved?.size).toBe("m");
    expect(
      container
        .querySelector("[data-testid='office-block-next']")
        ?.className.includes("mc-block-m"),
    ).toBe(true);
  });

  it("hops over hidden placements so one move always changes the visible order", async () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "next", size: "s", visible: true },
          { id: "needs-you", size: "l", visible: false },
          { id: "done", size: "m", visible: true },
          { id: "in-motion", size: "m", visible: true },
        ],
      }),
    );
    await mount();
    await click("Arrange office");
    await click("Move Done since you checked earlier");
    expect(blockIds()).toEqual(["done", "next", "in-motion"]);
  });

  it("hides a block, offers it in the library, and pins it back", async () => {
    await mount();
    await click("Arrange office");
    await click("Hide Done since you checked");
    expect(blockIds()).not.toContain("done");
    expect(
      loadGlassConfig().layout?.find((p) => p.id === "done")?.visible,
    ).toBe(false);

    await click("Pin Done since you checked to Office");
    expect(blockIds()).toContain("done");
    expect(
      loadGlassConfig().layout?.find((p) => p.id === "done")?.visible,
    ).toBe(true);
  });

  it("refuses a resize that would create rows beyond the fixed canvas", async () => {
    await mount();
    await click("Arrange office");
    await click("Resize Next");
    await click("Resize Next");
    await click("Resize In motion");

    expect(
      loadGlassConfig().layout?.find((p) => p.id === "in-motion")?.size,
    ).toBe("m");
    expect(container.querySelector("[role='alert']")?.textContent).toContain(
      "four-row Office canvas",
    );
  });

  it("reports storage failure and leaves the visible arrangement unchanged", async () => {
    await mount();
    await click("Arrange office");
    const original = Storage.prototype.setItem;
    const failure = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(function (this: Storage, key, value) {
        if (key === GLASS_CONFIG_STORAGE_KEY) throw new Error("disk full");
        return original.call(this, key, value);
      });

    await click("Move In motion earlier");
    expect(blockIds()[0]).toBe("needs-you");
    expect(container.querySelector("[role='alert']")?.textContent).toContain(
      "could not be saved",
    );
    failure.mockRestore();
  });

  it("persists and broadcasts each arrange action once under Strict Mode", async () => {
    await mount();
    await click("Arrange office");
    const writes = vi.spyOn(Storage.prototype, "setItem");
    const events = vi.fn();
    window.addEventListener("mc-glass-config-changed", events);

    await click("Move In motion earlier");
    expect(
      writes.mock.calls.filter(([key]) => key === GLASS_CONFIG_STORAGE_KEY),
    ).toHaveLength(1);
    expect(events).toHaveBeenCalledTimes(1);

    window.removeEventListener("mc-glass-config-changed", events);
    writes.mockRestore();
  });
});
