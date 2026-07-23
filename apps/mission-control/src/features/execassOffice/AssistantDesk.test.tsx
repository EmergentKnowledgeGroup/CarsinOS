// @vitest-environment jsdom

import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  fixtureDelegationDetailResponse,
  fixtureSummaryResponse,
} from "../../glass/execass/fixtures";
import { ExecassOfficePanel } from "./ExecassOfficePanel";
import type { ExecassOfficeController } from "./useExecassOfficeController";

let root: Root | null = null;
let container: HTMLDivElement;

function makeController(
  overrides: Partial<ExecassOfficeController> = {},
): ExecassOfficeController {
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
    converse: vi
      .fn()
      .mockResolvedValue({ kind: "conversational", text: "All quiet." }),
    loadDelegationDetail: vi
      .fn()
      .mockResolvedValue(fixtureDelegationDetailResponse().detail),
    clearConversationalReply: vi.fn(),
    freezeAll: vi.fn(),
    resumeAllWork: vi.fn(),
    loadReceipts: vi.fn(),
    dismissTrayNote: vi.fn(),
    ...overrides,
  } as unknown as ExecassOfficeController;
}

async function mount(controller: ExecassOfficeController) {
  container = document.createElement("div");
  document.body.appendChild(container);
  await act(async () => {
    root = createRoot(container);
    root.render(
      <StrictMode>
        <ExecassOfficePanel controller={controller} />
      </StrictMode>,
    );
  });
}

async function rerender(controller: ExecassOfficeController) {
  await act(async () => {
    root!.render(
      <StrictMode>
        <ExecassOfficePanel controller={controller} />
      </StrictMode>,
    );
  });
}

function desk(): HTMLElement | null {
  return document.querySelector("[data-testid='assistant-desk']");
}

function click(label: string) {
  const button = Array.from(document.querySelectorAll("button")).find(
    (b) => b.getAttribute("aria-label") === label || b.textContent === label,
  );
  expect(button, `button not found: ${label}`).toBeDefined();
  return act(async () => button!.click());
}

async function typeInto(selector: string, value: string) {
  const input = document.querySelector(selector) as HTMLInputElement | null;
  expect(input, `input not found: ${selector}`).not.toBeNull();
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(input),
    "value",
  )?.set;
  setter?.call(input, value);
  await act(async () => {
    input!.dispatchEvent(new Event("input", { bubbles: true }));
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

describe("Assistant's Desk slide-over", () => {
  it("opens from the ExecAss persona and logs a conversational exchange", async () => {
    const controller = makeController();
    await mount(controller);
    expect(desk()).toBeNull();

    await click("Walk to the Assistant's Desk");
    expect(desk()).not.toBeNull();
    expect(desk()?.getAttribute("role")).toBe("dialog");

    await typeInto(
      "[data-testid='desk-composer'] input",
      "anything need me today?",
    );
    await click("Send to ExecAss");
    expect(controller.converse).toHaveBeenCalledWith(
      "anything need me today?",
      null,
    );
    expect(desk()?.textContent).toContain("anything need me today?");
    expect(desk()?.textContent).toContain("All quiet.");
  });

  it("escalates a decision to the desk with live context and sends a revision", async () => {
    const controller = makeController();
    await mount(controller);
    await click("Let's talk it through");

    expect(desk()).not.toBeNull();
    const item = controller.summary!.needs_you[0]!;
    expect(desk()?.textContent).toContain(item.reason);
    expect(desk()?.textContent).toContain(item.recommendation);

    await typeInto(
      "[data-testid='desk-revision'] input",
      "export the list first, then close it",
    );
    await click("Send revision");
    expect(controller.resolveAttention).toHaveBeenCalledWith(
      item,
      "revise",
      "export the list first, then close it",
    );
    expect(desk()?.textContent).toContain(
      "export the list first, then close it",
    );
  });

  it("attaches desk asks to the focused delegation", async () => {
    const controller = makeController();
    await mount(controller);
    await click("Let's talk it through");
    await typeInto("[data-testid='desk-composer'] input", "why this vendor?");
    await click("Send to ExecAss");
    const item = controller.summary!.needs_you[0]!;
    const delegationId =
      item.subject.scope_kind === "delegation"
        ? item.subject.delegation_id
        : null;
    expect(controller.converse).toHaveBeenCalledWith(
      "why this vendor?",
      delegationId,
    );
  });

  it("shows the plan over the shoulder and an honest load failure", async () => {
    const controller = makeController();
    await mount(controller);
    await click("Let's talk it through");
    expect(desk()?.textContent).toContain(
      "Pick a venue, draft the agenda, confirm the caterer, send invites.",
    );

    const failing = makeController({
      loadDelegationDetail: vi.fn().mockRejectedValue(new Error("down")),
    } as Partial<ExecassOfficeController>);
    await act(async () => root?.unmount());
    container.remove();
    await mount(failing);
    await click("Let's talk it through");
    expect(desk()?.textContent).toContain(
      "The plan could not be fetched right now.",
    );
  });

  it("tells the truth when the focused decision leaves the summary", async () => {
    const controller = makeController();
    await mount(controller);
    await click("Let's talk it through");
    const drained = makeController({
      summary: { ...fixtureSummaryResponse(), needs_you: [] },
    } as Partial<ExecassOfficeController>);
    await rerender(drained);
    expect(desk()?.textContent).toContain("moved on while you were walking over");
    expect(desk()?.querySelector("[data-testid='desk-revision']")).toBeNull();
  });

  it("closes on Escape and keeps the sitting's conversation", async () => {
    const controller = makeController();
    await mount(controller);
    await click("Walk to the Assistant's Desk");
    await typeInto("[data-testid='desk-composer'] input", "remember this");
    await click("Send to ExecAss");

    await act(async () => {
      desk()!.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(desk()).toBeNull();

    await click("Walk to the Assistant's Desk");
    expect(desk()?.textContent).toContain("remember this");
  });

  it("logs delegation-created outcomes as durable work", async () => {
    const controller = makeController({
      converse: vi.fn().mockResolvedValue({
        kind: "delegation",
        delegation: {
          delegation_id: "dlg-new",
          intent_summary: "Book the venue",
        },
      }),
    } as Partial<ExecassOfficeController>);
    await mount(controller);
    await click("Walk to the Assistant's Desk");
    await typeInto("[data-testid='desk-composer'] input", "book the venue");
    await click("Send to ExecAss");
    expect(desk()?.textContent).toContain("Book the venue");
    expect(desk()?.textContent).toContain("delegation");
  });
});
