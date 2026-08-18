// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastStack } from "./Toast";

describe("ToastStack", () => {
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

  it("announces informational messages politely and failures assertively", async () => {
    await act(async () => {
      root.render(
        <ToastStack
          toasts={[
            { id: "saved", message: "Saved", tone: "info" },
            { id: "failed", message: "Approval failed", tone: "error" },
          ]}
          onDismiss={vi.fn()}
        />
      );
    });

    const info = container.querySelector('[role="status"]');
    const error = container.querySelector('[role="alert"]');
    expect(info?.getAttribute("aria-live")).toBe("polite");
    expect(error?.getAttribute("aria-live")).toBe("assertive");
    expect(container.querySelectorAll('[aria-label="Dismiss notification"]')).toHaveLength(2);
  });
});
