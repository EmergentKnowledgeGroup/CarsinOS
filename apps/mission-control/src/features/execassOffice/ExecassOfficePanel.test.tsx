// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { fixtureSummaryResponse } from "../../glass/execass/fixtures";
import { ExecassOfficePanel } from "./ExecassOfficePanel";
import {
  GLASS_CONFIG_EVENT,
  loadGlassConfig,
  saveGlassConfig,
} from "../../glass/config";
import { pinRoomBlocksToOffice } from "./pinToOffice";
import type { ExecassOfficeController } from "./useExecassOfficeController";

let root: Root | null = null;
let container: HTMLDivElement;

afterEach(async () => {
  await act(async () => root?.unmount());
  container.remove();
  root = null;
  localStorage.clear();
});

function fixtureController(): ExecassOfficeController {
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
    resolveAttention: vi.fn().mockResolvedValue(undefined),
    delegate: vi.fn(),
    clearConversationalReply: vi.fn(),
    freezeAll: vi.fn(),
    resumeAllWork: vi.fn(),
    loadReceipts: vi.fn(),
    dismissTrayNote: vi.fn(),
  } as unknown as ExecassOfficeController;
}

describe("ExecassOfficePanel room shortcuts", () => {
  it("keeps unpinned room shortcuts off the canvas", async () => {
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel
          controller={fixtureController()}
          onOpenRoom={() => true}
        />,
      );
    });
    expect(
      container.querySelector('[data-testid="office-block-boards"]'),
    ).toBeNull();
  });

  it("renders a pinned Boards shortcut that opens the room by stable id", async () => {
    pinRoomBlocksToOffice("boards");
    const onOpenRoom = vi.fn(() => true);
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel
          controller={fixtureController()}
          onOpenRoom={onOpenRoom}
        />,
      );
    });

    const block = container.querySelector(
      '[data-testid="office-block-boards"]',
    );
    expect(block).toBeTruthy();
    expect(block?.textContent).toContain("The Trenches");

    const open = Array.from(block?.querySelectorAll("button") ?? []).find(
      (button) => button.textContent?.includes("Open Boards"),
    );
    expect(open).toBeTruthy();
    await act(async () => open!.click());
    expect(onOpenRoom).toHaveBeenCalledWith("boards");
  });

  it("explains when a pinned shortcut targets a disabled room", async () => {
    pinRoomBlocksToOffice("calendar");
    const onOpenRoom = vi.fn(() => false);
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel
          controller={fixtureController()}
          onOpenRoom={onOpenRoom}
        />,
      );
    });

    const block = container.querySelector(
      '[data-testid="office-block-calendar"]',
    );
    const open = Array.from(block?.querySelectorAll("button") ?? []).find(
      (button) => button.textContent?.includes("Open Calendar"),
    );
    expect(open).toBeTruthy();
    await act(async () => open!.click());
    expect(onOpenRoom).toHaveBeenCalledWith("calendar");
    expect(block?.querySelector('[role="status"]')?.textContent).toContain(
      "Unavailable — turn on in Config",
    );
    await act(async () => {
      window.dispatchEvent(new Event(GLASS_CONFIG_EVENT));
    });
    expect(block?.querySelector('[role="status"]')).toBeNull();
    expect(open?.textContent).toContain("Open Calendar");
  });

  it("clears a stale refusal when the resolved room registry changes", async () => {
    pinRoomBlocksToOffice("history");
    const controller = fixtureController();
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel controller={controller} onOpenRoom={() => false} />,
      );
    });

    const block = container.querySelector(
      '[data-testid="office-block-history"]',
    );
    const open = Array.from(block?.querySelectorAll("button") ?? []).find(
      (button) => button.textContent?.includes("Open History & Receipts"),
    );
    expect(open).toBeTruthy();
    await act(async () => open!.click());
    expect(block?.querySelector('[role="status"]')?.textContent).toContain(
      "Unavailable — turn on in Config",
    );

    // Re-enabling the destination hands the panel a new resolved room-select
    // callback; the door may not stay labeled unavailable.
    await act(async () => {
      root!.render(
        <ExecassOfficePanel controller={controller} onOpenRoom={() => true} />,
      );
    });
    expect(block?.querySelector('[role="status"]')).toBeNull();
    expect(open?.textContent).toContain("Open History & Receipts");
  });

  it("keeps a refusal across rerenders when the resolved room registry is unchanged", async () => {
    pinRoomBlocksToOffice("history");
    const controller = fixtureController();
    const onOpenRoom = vi.fn(() => false);
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel controller={controller} onOpenRoom={onOpenRoom} />,
      );
    });

    const block = container.querySelector(
      '[data-testid="office-block-history"]',
    );
    const open = Array.from(block?.querySelectorAll("button") ?? []).find(
      (button) => button.textContent?.includes("Open History & Receipts"),
    );
    expect(open).toBeTruthy();
    await act(async () => open!.click());
    expect(block?.querySelector('[role="status"]')?.textContent).toContain(
      "Unavailable — turn on in Config",
    );

    await act(async () => {
      root!.render(
        <ExecassOfficePanel controller={controller} onOpenRoom={onOpenRoom} />,
      );
    });
    expect(block?.querySelector('[role="status"]')?.textContent).toContain(
      "Unavailable — turn on in Config",
    );
  });
});

describe("ExecassOfficePanel decision controls", () => {
  it("always offers an explicit Stop action and forwards the stop result", async () => {
    const resolveAttention = vi.fn().mockResolvedValue(undefined);
    const controller = {
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
      resolveAttention,
      delegate: vi.fn(),
      clearConversationalReply: vi.fn(),
      freezeAll: vi.fn(),
      resumeAllWork: vi.fn(),
      loadReceipts: vi.fn(),
      dismissTrayNote: vi.fn(),
    } as unknown as ExecassOfficeController;

    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel controller={controller} onOpenRoom={() => true} />,
      );
    });

    const stop = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Stop",
    );
    expect(stop).toBeDefined();
    await act(async () => stop!.click());
    expect(resolveAttention).toHaveBeenCalledWith(
      controller.summary!.needs_you[0],
      "stop",
      undefined,
    );
  });
});

describe("ExecassOfficePanel mobile stacked feed", () => {
  function stubViewportWidth(narrowMatches: boolean) {
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: narrowMatches,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  }

  function seedNeedsYouLast() {
    saveGlassConfig({
      ...loadGlassConfig(),
      layout: [
        { id: "in-motion", size: "m", visible: true },
        { id: "done", size: "m", visible: true },
        { id: "needs-you", size: "l", visible: true },
        { id: "next", size: "s", visible: true },
      ],
    });
  }

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("puts Needs You first in the narrow stacked feed even when arranged later", async () => {
    stubViewportWidth(true);
    seedNeedsYouLast();
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel
          controller={fixtureController()}
          onOpenRoom={() => true}
        />,
      );
    });
    const blocks = Array.from(
      container.querySelectorAll('[data-testid^="office-block-"]'),
    );
    expect(blocks.length).toBeGreaterThan(1);
    expect(blocks[0]?.getAttribute("data-testid")).toBe(
      "office-block-needs-you",
    );
  });

  it("shows the owner's true stored order while arranging, even when narrow", async () => {
    stubViewportWidth(true);
    seedNeedsYouLast();
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel
          controller={fixtureController()}
          onOpenRoom={() => true}
        />,
      );
    });
    const arrange = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("aria-label") === "Arrange office",
    );
    expect(arrange).toBeTruthy();
    await act(async () => arrange!.click());
    const blocks = Array.from(
      container.querySelectorAll('[data-testid^="office-block-"]'),
    );
    expect(blocks[0]?.getAttribute("data-testid")).toBe(
      "office-block-in-motion",
    );
  });

  it("keeps the owner's arranged order untouched on desktop widths", async () => {
    stubViewportWidth(false);
    seedNeedsYouLast();
    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(
        <ExecassOfficePanel
          controller={fixtureController()}
          onOpenRoom={() => true}
        />,
      );
    });
    const blocks = Array.from(
      container.querySelectorAll('[data-testid^="office-block-"]'),
    );
    expect(blocks[0]?.getAttribute("data-testid")).toBe(
      "office-block-in-motion",
    );
  });
});
