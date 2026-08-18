import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { GuidedTourOverlay, type GuidedTourStep } from "./GuidedTourOverlay";

const STEPS: GuidedTourStep[] = [
  {
    id: "floor-office",
    targetId: "floor-office",
    title: "4F · The Office",
    body: "Start your day here.",
  },
  {
    id: "help",
    targetId: "nav-help-shortcut",
    title: "Help & Docs",
    body: "Plain-language docs.",
  },
];

describe("GuidedTourOverlay", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    // @ts-expect-error test-only global
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});
    // jsdom reports empty client rects for everything; the overlay treats
    // that as "hidden", so give every element a visible rect.
    Object.defineProperty(HTMLElement.prototype, "getClientRects", {
      configurable: true,
      value: () => [{ width: 10, height: 10 }],
    });
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
  });

  function render(props: Partial<Parameters<typeof GuidedTourOverlay>[0]> = {}) {
    act(() => {
      root.render(
        <GuidedTourOverlay
          open
          steps={STEPS}
          stepIndex={0}
          onClose={() => {}}
          onPrev={() => {}}
          onNext={() => {}}
          {...props}
        />,
      );
    });
  }

  function attachVisibleAnchor(tourId: string) {
    const anchor = document.createElement("div");
    anchor.setAttribute("data-tour-id", tourId);
    anchor.getBoundingClientRect = () =>
      ({ top: 40, left: 8, width: 120, height: 32 }) as DOMRect;
    document.body.appendChild(anchor);
    return anchor;
  }

  it("shows honest recovery copy when the current stop's target is missing", () => {
    render();
    const note = document.querySelector(".mc-tour-missing");
    expect(note?.textContent ?? "").toMatch(/isn't visible right now/i);
  });

  it("shows honest recovery copy when the target exists but is hidden (zero rect)", () => {
    const anchor = document.createElement("div");
    anchor.setAttribute("data-tour-id", "floor-office");
    document.body.appendChild(anchor);
    render();
    const note = document.querySelector(".mc-tour-missing");
    expect(note?.textContent ?? "").toMatch(/isn't visible right now/i);
    expect(document.querySelector(".mc-tour-highlight")).toBeNull();
  });

  it("shows no recovery copy while the target is visibly on screen", () => {
    attachVisibleAnchor("floor-office");
    render();
    expect(document.querySelector(".mc-tour-missing")).toBeNull();
    expect(document.querySelector(".mc-tour-highlight")).not.toBeNull();
  });

  it("cycles Tab focus inside the tour dialog instead of escaping it", () => {
    const outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);
    render();
    const buttons = Array.from(
      document.querySelectorAll<HTMLButtonElement>(".mc-tour-bubble button"),
    ).filter((button) => !button.disabled);
    expect(buttons.length).toBeGreaterThan(1);
    const last = buttons[buttons.length - 1];
    act(() => {
      last.focus();
    });
    act(() => {
      last.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true }),
      );
    });
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("falls back to an equivalent tour launcher when the invoker was unmounted mid-tour", () => {
    const invoker = document.createElement("button");
    invoker.textContent = "banner tour";
    document.body.appendChild(invoker);
    const equivalent = document.createElement("button");
    equivalent.className = "mc-tab-help-btn-tour";
    equivalent.textContent = "tour";
    document.body.appendChild(equivalent);
    act(() => {
      invoker.focus();
    });
    render();
    // The tour walked to another room and the invoking banner unmounted.
    invoker.remove();
    render({ open: false });
    expect(document.activeElement).toBe(equivalent);
  });

  it("restores focus to the launcher when the tour closes", () => {
    const launcher = document.createElement("button");
    launcher.textContent = "launch tour";
    document.body.appendChild(launcher);
    act(() => {
      launcher.focus();
    });
    render();
    expect(document.activeElement).not.toBe(launcher);
    render({ open: false });
    expect(document.activeElement).toBe(launcher);
  });
});
