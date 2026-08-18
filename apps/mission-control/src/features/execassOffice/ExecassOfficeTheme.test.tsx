// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  GLASS_CONFIG_STORAGE_KEY,
  notifyGlassConfigChanged,
} from "../../glass/config";
import { fixtureSummaryResponse } from "../../glass/execass/fixtures";
import { ExecassOfficePanel } from "./ExecassOfficePanel";
import type { ExecassOfficeController } from "./useExecassOfficeController";

let root: Root | null = null;
let container: HTMLDivElement;

async function mount() {
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
    resolveAttention: vi.fn(),
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
    root.render(<ExecassOfficePanel controller={controller} onOpenRoom={() => {}} />);
  });
}

function officeEl(): HTMLElement {
  return container.querySelector(".mc-execass-office") as HTMLElement;
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

describe("Office glass theme", () => {
  it("applies the active glass theme tokens to the office surface", async () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({ themeId: "carbon-dark", customThemes: [] }),
    );
    await mount();
    expect(officeEl().style.getPropertyValue("--claw")).toBe("#FF5A1F");
    expect(officeEl().style.getPropertyValue("--surface")).toBe("#17171B");
  });

  it("re-applies tokens when the glass config changes", async () => {
    await mount();
    // jsdom has no matchMedia, so auto resolves to the light theme.
    expect(officeEl().style.getPropertyValue("--claw")).toBe("#E8541B");
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({ themeId: "carbon-dark", customThemes: [] }),
    );
    await act(async () => notifyGlassConfigChanged());
    expect(officeEl().style.getPropertyValue("--claw")).toBe("#FF5A1F");
  });
});
