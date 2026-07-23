// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { fixtureSummaryResponse } from "../../glass/execass/fixtures";
import { ExecassOfficePanel } from "./ExecassOfficePanel";
import type { ExecassOfficeController } from "./useExecassOfficeController";

let root: Root | null = null;
let container: HTMLDivElement;

afterEach(async () => {
  await act(async () => root?.unmount());
  container.remove();
  root = null;
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
      root.render(<ExecassOfficePanel controller={controller} />);
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
