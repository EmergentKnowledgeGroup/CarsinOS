// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { fixtureSummaryResponse } from "../../glass/execass/fixtures";
import { ExecassOfficePanel } from "./ExecassOfficePanel";
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
          onOpenRoom={() => {}}
        />,
      );
    });
    expect(
      container.querySelector('[data-testid="office-block-boards"]'),
    ).toBeNull();
  });

  it("renders a pinned Boards shortcut that opens the room by stable id", async () => {
    pinRoomBlocksToOffice("boards");
    const onOpenRoom = vi.fn();
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
        <ExecassOfficePanel controller={controller} onOpenRoom={() => {}} />,
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
