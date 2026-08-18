import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { useRef } from "react";
import { useDialogFocus } from "./useDialogFocus";

function Harness(props: { open: boolean }) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  useDialogFocus(props.open, dialogRef);
  if (!props.open) {
    return null;
  }
  return (
    <div ref={dialogRef} role="dialog" aria-modal="true" tabIndex={-1}>
      <button type="button">first</button>
      <button type="button">second</button>
      <button type="button">last</button>
    </div>
  );
}

function ClosedDetailsHarness(props: { open: boolean }) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  useDialogFocus(props.open, dialogRef);
  if (!props.open) {
    return null;
  }
  return (
    <div ref={dialogRef} role="dialog" aria-modal="true" tabIndex={-1}>
      <button type="button">first</button>
      <details>
        <summary>last visible</summary>
        <button type="button">hidden disclosure control</button>
      </details>
    </div>
  );
}

describe("useDialogFocus", () => {
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
    const visibleRect = new DOMRect(0, 0, 10, 10);
    vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue({
      0: visibleRect,
      length: 1,
      item: (index: number) => (index === 0 ? visibleRect : null),
      [Symbol.iterator]: function* () {
        yield visibleRect;
      },
    } as DOMRectList);
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    document.body.innerHTML = "";
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  function render(open: boolean) {
    act(() => {
      root.render(<Harness open={open} />);
    });
  }

  function dialogButtons(): HTMLButtonElement[] {
    return Array.from(
      document.querySelectorAll<HTMLButtonElement>("[role='dialog'] button"),
    );
  }

  it("moves focus into the dialog when it opens", () => {
    render(true);
    const dialog = document.querySelector("[role='dialog']");
    expect(dialog?.contains(document.activeElement)).toBe(true);
  });

  it("cycles Tab from the last control back to the first", () => {
    render(true);
    const buttons = dialogButtons();
    const last = buttons[buttons.length - 1];
    act(() => {
      last.focus();
    });
    act(() => {
      last.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Tab",
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("cycles Shift+Tab from the first control to the last", () => {
    render(true);
    const buttons = dialogButtons();
    const first = buttons[0];
    act(() => {
      first.focus();
    });
    act(() => {
      first.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Tab",
          shiftKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    expect(document.activeElement).toBe(buttons[buttons.length - 1]);
  });

  it("excludes controls inside closed disclosures from the focus ring", () => {
    act(() => {
      root.render(<ClosedDetailsHarness open />);
    });
    const first = document.querySelector<HTMLButtonElement>(
      "[role='dialog'] > button",
    );
    const summary = document.querySelector<HTMLElement>(
      "[role='dialog'] summary",
    );
    const hidden = document.querySelector<HTMLButtonElement>(
      "[role='dialog'] details button",
    );
    expect(first).toBeTruthy();
    expect(summary).toBeTruthy();
    expect(hidden).toBeTruthy();
    act(() => {
      first!.focus();
      first!.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Tab",
          shiftKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    expect(document.activeElement).toBe(summary);
    expect(document.activeElement).not.toBe(hidden);
  });

  it("restores focus to the exact invoker when the dialog closes", () => {
    const invoker = document.createElement("button");
    invoker.textContent = "open dialog";
    document.body.appendChild(invoker);
    act(() => {
      invoker.focus();
    });
    render(true);
    expect(document.activeElement).not.toBe(invoker);
    render(false);
    expect(document.activeElement).toBe(invoker);
  });
});
