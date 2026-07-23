// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppShell } from "./AppShell";
import type { MissionControlTab } from "./useAppController";
import { DEFAULT_OPSUX_RUNTIME_CONFIG } from "../lib/opsUxConfig";

describe("AppShell live feed toggle", () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    // Silence React's act() environment warning and flush focus effects deterministically.
    // @ts-expect-error test-only global
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});
    Object.defineProperty(HTMLElement.prototype, "getClientRects", {
      configurable: true,
      value() {
        return {
          length: 1,
          item: () => null,
          [Symbol.iterator]: function* () {},
        };
      },
    });
    Element.prototype.scrollIntoView = vi.fn();
    localStorage.clear();
  });

  afterEach(() => {
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
  });

  it("opens settings when the live feed feature is unavailable", async () => {
    const root = createRoot(container);
    const onPatchOpsUxControls = vi.fn();
    const availableTabs: MissionControlTab[] = ["boards"];

    await act(async () => {
      root.render(
        <AppShell
          activeTab="boards"
          availableTabs={availableTabs}
          onTabChange={() => {}}
          healthState="healthy"
          wsState="connected"
          tokenConfigured
          incidentMode={false}
          onIncidentModeChange={() => {}}
          openBreakerCount={0}
          approvalsCount={0}
          jobsDue={0}
          schedulerRunning
          gatewayDraft="http://127.0.0.1:18789"
          onGatewayDraftChange={() => {}}
          tokenDraft="token"
          onTokenDraftChange={() => {}}
          onSaveConnection={async () => {}}
          onReconnect={async () => {}}
          onClearToken={async () => {}}
          onOpenSetupWizard={() => {}}
          onOpenHelpDocs={() => {}}
          onOpenGuidedTour={() => {}}
          notifications={[]}
          onDismissNotification={() => {}}
          onClearAllNotifications={() => {}}
          liveFeedEnabled={false}
          liveFeedOpen={false}
          liveFeedUnreadCount={33}
          onToggleLiveFeed={() => {}}
          opsUxConfig={DEFAULT_OPSUX_RUNTIME_CONFIG}
          opsUxConfigError={null}
          onPatchOpsUxControls={onPatchOpsUxControls}
          usageChartsEnabled={false}
          assistantSystemPrompt="You are the CarsinOS assistant."
          assistantSystemPromptDirty={false}
          assistantSystemPromptLoading={false}
          assistantSystemPromptSaving={false}
          assistantSystemPromptError={null}
          onAssistantSystemPromptChange={() => {}}
          onSaveAssistantSystemPrompt={async () => {}}
          onResetAssistantSystemPrompt={() => {}}
          onRestoreDefaultAssistantSystemPrompt={() => {}}
          quickGuideAvailable={true}
          quickGuideOpen={true}
          onToggleQuickGuide={() => {}}
        >
          <div>content</div>
        </AppShell>
      );
    });

    const toggle = container.querySelector('[data-testid="live-feed-toggle"]');
    expect(toggle).toBeTruthy();
    expect(toggle?.getAttribute("aria-disabled")).toBeNull();

    await act(async () => {
      toggle?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    await act(async () => {
      await Promise.resolve();
    });

    const checkboxes = Array.from(
      container.querySelectorAll('input[type="checkbox"]')
    ) as HTMLInputElement[];
    const checkbox = checkboxes[1] ?? null;
    expect(container.textContent).toContain("Choose what pages show");
    expect(checkbox).toBeTruthy();
  });

  it("does not render the old Appearance section inside settings", async () => {
    const root = createRoot(container);

    await act(async () => {
      root.render(
        <AppShell
          activeTab="boards"
          availableTabs={["boards"]}
          onTabChange={() => {}}
          healthState="healthy"
          wsState="connected"
          tokenConfigured
          incidentMode={false}
          onIncidentModeChange={() => {}}
          openBreakerCount={0}
          approvalsCount={0}
          jobsDue={0}
          schedulerRunning
          gatewayDraft="http://127.0.0.1:18789"
          onGatewayDraftChange={() => {}}
          tokenDraft="token"
          onTokenDraftChange={() => {}}
          onSaveConnection={async () => {}}
          onReconnect={async () => {}}
          onClearToken={async () => {}}
          onOpenSetupWizard={() => {}}
          onOpenHelpDocs={() => {}}
          onOpenGuidedTour={() => {}}
          notifications={[]}
          onDismissNotification={() => {}}
          onClearAllNotifications={() => {}}
          liveFeedEnabled
          liveFeedOpen={false}
          liveFeedUnreadCount={0}
          onToggleLiveFeed={() => {}}
          opsUxConfig={DEFAULT_OPSUX_RUNTIME_CONFIG}
          opsUxConfigError={null}
          onPatchOpsUxControls={() => {}}
          usageChartsEnabled={false}
          assistantSystemPrompt="You are the CarsinOS assistant."
          assistantSystemPromptDirty={false}
          assistantSystemPromptLoading={false}
          assistantSystemPromptSaving={false}
          assistantSystemPromptError={null}
          onAssistantSystemPromptChange={() => {}}
          onSaveAssistantSystemPrompt={async () => {}}
          onResetAssistantSystemPrompt={() => {}}
          onRestoreDefaultAssistantSystemPrompt={() => {}}
          quickGuideAvailable={true}
          quickGuideOpen={true}
          onToggleQuickGuide={() => {}}
        >
          <div>content</div>
        </AppShell>
      );
    });

    const settingsButton = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("title") === "Settings"
    );
    expect(settingsButton).toBeTruthy();

    await act(async () => {
      settingsButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(container.textContent).toContain("Assistant");
    expect(container.textContent).not.toContain("Appearance");
  });

  it("offers the Glass theme studio inside settings", async () => {
    const root = createRoot(container);

    await act(async () => {
      root.render(
        <AppShell
          activeTab="boards"
          availableTabs={["boards"]}
          onTabChange={() => {}}
          healthState="healthy"
          wsState="connected"
          tokenConfigured
          incidentMode={false}
          onIncidentModeChange={() => {}}
          openBreakerCount={0}
          approvalsCount={0}
          jobsDue={0}
          schedulerRunning
          gatewayDraft="http://127.0.0.1:18789"
          onGatewayDraftChange={() => {}}
          tokenDraft="token"
          onTokenDraftChange={() => {}}
          onSaveConnection={async () => {}}
          onReconnect={async () => {}}
          onClearToken={async () => {}}
          onOpenSetupWizard={() => {}}
          onOpenHelpDocs={() => {}}
          onOpenGuidedTour={() => {}}
          notifications={[]}
          onDismissNotification={() => {}}
          onClearAllNotifications={() => {}}
          liveFeedEnabled
          liveFeedOpen={false}
          liveFeedUnreadCount={0}
          onToggleLiveFeed={() => {}}
          opsUxConfig={DEFAULT_OPSUX_RUNTIME_CONFIG}
          opsUxConfigError={null}
          onPatchOpsUxControls={() => {}}
          usageChartsEnabled={false}
          assistantSystemPrompt="You are the CarsinOS assistant."
          assistantSystemPromptDirty={false}
          assistantSystemPromptLoading={false}
          assistantSystemPromptSaving={false}
          assistantSystemPromptError={null}
          onAssistantSystemPromptChange={() => {}}
          onSaveAssistantSystemPrompt={async () => {}}
          onResetAssistantSystemPrompt={() => {}}
          onRestoreDefaultAssistantSystemPrompt={() => {}}
          quickGuideAvailable={true}
          quickGuideOpen={true}
          onToggleQuickGuide={() => {}}
        >
          <div>content</div>
        </AppShell>
      );
    });

    const settingsButton = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("title") === "Settings"
    );
    await act(async () => {
      settingsButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(container.textContent).toContain("Theme studio");
    expect(container.querySelector("[data-testid='theme-active']")).toBeTruthy();
  });

  it("shows a focused memory review chip when ExecAss has learning proposals", async () => {
    const root = createRoot(container);
    const onTabChange = vi.fn();

    await act(async () => {
      root.render(
        <AppShell
          activeTab="boards"
          availableTabs={["boards", "focus"]}
          onTabChange={onTabChange}
          healthState="healthy"
          wsState="connected"
          tokenConfigured
          incidentMode={false}
          onIncidentModeChange={() => {}}
          openBreakerCount={0}
          approvalsCount={2}
          memoryReviewApprovalsCount={1}
          jobsDue={0}
          schedulerRunning
          gatewayDraft="http://127.0.0.1:18789"
          onGatewayDraftChange={() => {}}
          tokenDraft="token"
          onTokenDraftChange={() => {}}
          onSaveConnection={async () => {}}
          onReconnect={async () => {}}
          onClearToken={async () => {}}
          onOpenSetupWizard={() => {}}
          onOpenHelpDocs={() => {}}
          onOpenGuidedTour={() => {}}
          notifications={[]}
          onDismissNotification={() => {}}
          onClearAllNotifications={() => {}}
          liveFeedEnabled
          liveFeedOpen={false}
          liveFeedUnreadCount={0}
          onToggleLiveFeed={() => {}}
          opsUxConfig={DEFAULT_OPSUX_RUNTIME_CONFIG}
          opsUxConfigError={null}
          onPatchOpsUxControls={() => {}}
          usageChartsEnabled={false}
          assistantSystemPrompt="You are the CarsinOS assistant."
          assistantSystemPromptDirty={false}
          assistantSystemPromptLoading={false}
          assistantSystemPromptSaving={false}
          assistantSystemPromptError={null}
          onAssistantSystemPromptChange={() => {}}
          onSaveAssistantSystemPrompt={async () => {}}
          onResetAssistantSystemPrompt={() => {}}
          onRestoreDefaultAssistantSystemPrompt={() => {}}
          quickGuideAvailable={true}
          quickGuideOpen={false}
          onToggleQuickGuide={() => {}}
        >
          <div>content</div>
        </AppShell>
      );
    });

    const memoryChip = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Memory review: 1"
    );
    expect(memoryChip).toBeTruthy();

    await act(async () => {
      memoryChip?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(onTabChange).toHaveBeenCalledWith("focus");
  });
});
