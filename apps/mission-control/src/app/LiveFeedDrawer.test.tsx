// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LiveFeedDrawer } from "./LiveFeedDrawer";

type TestGlobal = typeof globalThis & {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
};

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: () => ({
    getTotalSize: () => 0,
    getVirtualItems: () => [],
  }),
}));

describe("LiveFeedDrawer", () => {
  let container: HTMLDivElement | null;
  let root: Root | null;
  let previousActEnvironment: boolean | undefined;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = null;
    previousActEnvironment = (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT;
    (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    if (root) {
      act(() => {
        root?.unmount();
      });
      root = null;
    }
    container?.remove();
    container = null;
    document.body.innerHTML = "";
    if (previousActEnvironment === undefined) {
      delete (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT;
    } else {
      (globalThis as TestGlobal).IS_REACT_ACT_ENVIRONMENT = previousActEnvironment;
    }
  });

  it("removes the drawer controls from the accessibility tree while closed", async () => {
    if (!container) {
      throw new Error("test container was not initialized");
    }
    root = createRoot(container);
    const onToggleOpen = vi.fn();

    await act(async () => {
      root?.render(
        <LiveFeedDrawer
          enabled
          open={false}
          paused={false}
          unreadCount={33}
          domainFilter="all"
          severityFilter="all"
          events={[]}
          storageMode="durable"
          storageError={null}
          recoveryAvailableCount={0}
          markAllUndoAvailable={false}
          clearUndoAvailable={false}
          approvalsCount={0}
          openBreakersCount={0}
          mailUnreadCount={0}
          onToggleOpen={onToggleOpen}
          onTogglePause={() => {}}
          onDomainFilterChange={() => {}}
          onSeverityFilterChange={() => {}}
          onMarkAllRead={() => {}}
          onUndoMarkAllRead={() => {}}
          onClearSoft={() => {}}
          onRestoreClear={() => {}}
          onRestoreRecovery={() => {}}
        />
      );
    });

    const drawer = container.querySelector('[data-testid="live-feed-drawer"]');
    expect(drawer?.getAttribute("aria-hidden")).toBe("true");
    expect(drawer?.hasAttribute("inert")).toBe(true);
    expect(onToggleOpen).not.toHaveBeenCalled();
  });
});
