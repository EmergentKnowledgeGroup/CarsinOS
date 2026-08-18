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
          activeRoomId="boards"
          onRoomSelect={() => {}}
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
          activeRoomId="boards"
          onRoomSelect={() => {}}
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
          activeRoomId="boards"
          onRoomSelect={() => {}}
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

  it("lights exactly one elevator room by stable id when two rooms share a surface", async () => {
    const root = createRoot(container);
    const onRoomSelect = vi.fn();

    await act(async () => {
      root.render(
        <AppShell
          activeTab="team"
          availableTabs={["boards", "team"]}
          onTabChange={() => {}}
          activeRoomId="models"
          onRoomSelect={onRoomSelect}
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
          quickGuideOpen={false}
          onToggleQuickGuide={() => {}}
        >
          <div>content</div>
        </AppShell>
      );
    });

    const activeRooms = Array.from(
      container.querySelectorAll(".mc-nav-item-active")
    );
    expect(activeRooms).toHaveLength(1);
    expect(activeRooms[0]?.getAttribute("title")).toBe(
      "BF · Models & Providers"
    );
    expect(
      activeRooms[0]?.querySelector(".mc-nav-room-mark")?.textContent
    ).toBe("MP");

    const staffButton = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("title") === "2F · Staff Directory"
    );
    expect(staffButton).toBeTruthy();
    expect(
      staffButton?.querySelector(".mc-nav-room-mark")?.textContent
    ).toBe("SD");
    await act(async () => {
      staffButton?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onRoomSelect).toHaveBeenCalledWith("staff");
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
          activeRoomId="boards"
          onRoomSelect={() => {}}
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

describe("AppShell incident posture band", () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    // @ts-expect-error test-only global
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});
    localStorage.clear();
  });

  afterEach(() => {
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
  });

  async function renderShell(
    overrides: Partial<React.ComponentProps<typeof AppShell>> = {},
  ) {
    const root = createRoot(container);
    await act(async () => {
      root.render(
        <AppShell
          activeTab="boards"
          availableTabs={["boards", "focus", "connectors", "assistant"]}
          onTabChange={() => {}}
          activeRoomId="boards"
          onRoomSelect={() => {}}
          healthState="up"
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
          liveFeedEnabled={false}
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
          quickGuideAvailable={false}
          quickGuideOpen={false}
          onToggleQuickGuide={() => {}}
          {...overrides}
        >
          <div>content</div>
        </AppShell>,
      );
    });
    return root;
  }

  const breakerIncident = {
    posture: "incident",
    cause: "core-breaker",
    causeKey: "core-breaker:provider:anthropic",
    message: "A circuit breaker is open, so part of the system is paused.",
    detail: "Open: provider:anthropic.",
    target: { roomId: "breakers", label: "Breakers & Scheduler" },
  } as const;

  it("shows a quiet calm status word and no band when calm", async () => {
    await renderShell({ incidentPosture: { posture: "calm" } });
    const status = container.querySelector(
      '[data-testid="incident-posture-status"]',
    );
    expect(status?.textContent).toBe("Calm");
    expect(
      container.querySelector('[data-testid="incident-band"]'),
    ).toBeNull();
  });

  it("shows a checking status word while posture is unknown, never claiming calm", async () => {
    await renderShell({ incidentPosture: { posture: "unknown" } });
    const status = container.querySelector(
      '[data-testid="incident-posture-status"]',
    );
    expect(status?.textContent).toBe("Checking");
    expect(
      container.querySelector('[data-testid="incident-band"]'),
    ).toBeNull();
  });

  it("defaults to the unknown posture when no posture is provided", async () => {
    await renderShell();
    const status = container.querySelector(
      '[data-testid="incident-posture-status"]',
    );
    expect(status?.textContent).toBe("Checking");
  });

  it("renders one plain-language band with a walk-there action for an incident", async () => {
    const onRoomSelect = vi.fn().mockReturnValue(true);
    await renderShell({
      incidentPosture: breakerIncident,
      incidentWalkAvailable: true,
      onRoomSelect,
    });

    const band = container.querySelector('[data-testid="incident-band"]');
    expect(band).toBeTruthy();
    expect(band?.getAttribute("role")).toBe("status");
    expect(band?.getAttribute("aria-live")).toBe("polite");
    expect(band?.textContent).toContain("Incident");
    expect(band?.textContent).toContain(
      "A circuit breaker is open, so part of the system is paused.",
    );
    expect(band?.textContent).toContain("Open: provider:anthropic.");

    const status = container.querySelector(
      '[data-testid="incident-posture-status"]',
    );
    expect(status?.textContent).toBe("Incident");

    const walk = container.querySelector(
      '[data-testid="incident-band-walk"]',
    ) as HTMLButtonElement | null;
    expect(walk).toBeTruthy();
    expect(walk?.textContent).toContain("Breakers & Scheduler");

    await act(async () => {
      walk?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onRoomSelect).toHaveBeenCalledWith("breakers");
  });

  it("shows an honest note instead of a dead button when the room is unavailable", async () => {
    await renderShell({
      incidentPosture: breakerIncident,
      incidentWalkAvailable: false,
    });
    expect(
      container.querySelector('[data-testid="incident-band-walk"]'),
    ).toBeNull();
    const band = container.querySelector('[data-testid="incident-band"]');
    expect(band?.textContent).toContain(
      "Breakers & Scheduler is not available right now",
    );
  });
});

describe("AppShell dialog accessibility", () => {
  let container: HTMLDivElement;
  let root: ReturnType<typeof createRoot> | null;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = null;
    // @ts-expect-error test-only global
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});
    localStorage.clear();
  });

  afterEach(async () => {
    await act(async () => {
      root?.unmount();
    });
    root = null;
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
  });

  async function renderShell() {
    root = createRoot(container);
    await act(async () => {
      root!.render(
        <AppShell
          activeTab="boards"
          availableTabs={["boards"] as MissionControlTab[]}
          onTabChange={() => {}}
          activeRoomId="boards"
          onRoomSelect={() => {}}
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
          quickGuideAvailable={false}
          quickGuideOpen={false}
          onToggleQuickGuide={() => {}}
        >
          <div>content</div>
        </AppShell>
      );
    });
    return root;
  }

  it("marks the active room with aria-current in the elevator rail", async () => {
    await renderShell();
    const active = container.querySelector('[data-tour-id="nav-boards"]');
    expect(active?.getAttribute("aria-current")).toBe("page");
    const help = container.querySelector('[data-tour-id="nav-help-shortcut"]');
    expect(help?.getAttribute("aria-current")).toBeNull();
  });

  it("opens Settings as a labeled modal dialog and restores focus to the invoker", async () => {
    await renderShell();
    const configButton = container.querySelector<HTMLButtonElement>(
      '[data-tour-id="nav-config"]',
    );
    expect(configButton).toBeTruthy();
    await act(async () => {
      configButton!.focus();
      configButton!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    const dialog = document.querySelector('.mc-settings-modal');
    expect(dialog?.getAttribute("role")).toBe("dialog");
    expect(dialog?.getAttribute("aria-modal")).toBe("true");
    expect(dialog?.getAttribute("aria-label")).toBe("Settings");
    expect(dialog?.contains(document.activeElement)).toBe(true);
    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(document.querySelector(".mc-settings-modal")).toBeNull();
    expect(document.activeElement).toBe(configButton);
  });

  it("opens the command palette as a labeled dialog and restores focus on close", async () => {
    await renderShell();
    const trigger = container.querySelector<HTMLButtonElement>(
      '[data-tour-id="topbar-command"]',
    );
    expect(trigger).toBeTruthy();
    await act(async () => {
      trigger!.focus();
      trigger!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    const palette = document.querySelector(".mc-cmd-palette");
    expect(palette?.getAttribute("role")).toBe("dialog");
    expect(palette?.getAttribute("aria-modal")).toBe("true");
    expect(palette?.getAttribute("aria-label")).toBe("Command palette");
    expect(
      document.activeElement?.classList.contains("mc-cmd-input"),
    ).toBe(true);
    await act(async () => {
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(document.querySelector(".mc-cmd-palette")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
});
