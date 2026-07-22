// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RuntimeCloseDialog } from "./App";

describe("RuntimeCloseDialog", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean })
      .IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("describes the consequence, keeps focus inside, and cancels once on Escape", async () => {
    const onCancel = vi.fn();
    await act(async () => {
      root.render(
        <RuntimeCloseDialog
          confirmation={{
            consequence: "Active app-bound work will be paused before the window closes.",
            binding: {
              challenge: "challenge-1",
              original_request_id: "request-1",
              original_nonce: "nonce-1",
              original_issued_at_ms: 1_000,
            },
          }}
          confirming={false}
          onConfirm={vi.fn()}
          onCancel={onCancel}
        />,
      );
    });

    const dialog = container.querySelector<HTMLElement>('[role="dialog"]');
    const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"));
    expect(dialog?.getAttribute("aria-describedby")).toBe(
      "runtime-close-consequence runtime-close-cancel-note",
    );
    expect(container.querySelector("#runtime-close-consequence")?.textContent).toContain(
      "will be paused",
    );
    expect(document.activeElement).toBe(buttons[0]);

    buttons[1].focus();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    expect(document.activeElement).toBe(buttons[0]);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
