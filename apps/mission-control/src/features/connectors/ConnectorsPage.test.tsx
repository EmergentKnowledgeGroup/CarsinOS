// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ConnectorsPage } from "./ConnectorsPage";
import type { useConnectorsController } from "./useConnectorsController";
import { fixturePolicyResponse } from "../../glass/execass/fixtures";
import { DEFAULT_OPSUX_RUNTIME_CONFIG } from "../../lib/opsUxConfig";
import type { ExecassPolicyController } from "../execassPolicy/useExecassPolicyController";
import type { SetupSurfaceProps } from "../setup/SetupControls";

vi.mock("../../lib/api", () => ({
  approveDiscordPairing: vi.fn().mockRejectedValue(new Error("offline")),
  approveTelegramPairing: vi.fn().mockRejectedValue(new Error("offline")),
  denyDiscordPairing: vi.fn().mockRejectedValue(new Error("offline")),
  denyTelegramPairing: vi.fn().mockRejectedValue(new Error("offline")),
  getChannelConfig: vi.fn().mockRejectedValue(new Error("offline")),
  getChannelRuntimeStatus: vi.fn().mockRejectedValue(new Error("offline")),
  getDiscordPairingStatus: vi.fn().mockRejectedValue(new Error("offline")),
  getRuntimeConfig: vi.fn().mockRejectedValue(new Error("offline")),
  getTelegramPairingStatus: vi.fn().mockRejectedValue(new Error("offline")),
}));

type ConnectorsController = ReturnType<typeof useConnectorsController>;

function stubController(overrides?: Partial<Record<string, unknown>>) {
  const controller = {
    settings: { gateway_url: "http://127.0.0.1:18789" },
    agents: [],
    availability: "ready",
    availabilityMessage: null,
    catalog: [],
    installedConnectors: [],
    interactions: [],
    pausedInteractions: [],
    selectedConnector: null,
    selectedConnectorId: "",
    selectedConnectorDetail: null,
    selectedConnectorInteractions: [],
    selectedVersion: null,
    selectedVersionId: "",
    selectedPublishedTool: null,
    selectedPublishedToolId: "",
    selectedPublishedToolIds: [],
    selectedToolDetail: null,
    selectedConversion: null,
    health: null,
    importDraft: {},
    assignmentDraft: {},
    authBindingDraft: {},
    publishDraft: { selected_candidate_ids: [], alias_overrides: {} },
    interactionPayloadText: "",
    mutatingAction: null,
    detailLoading: false,
    detailError: null,
    toolDetailError: null,
    healthError: null,
    enabled: true,
    selectConnector: vi.fn(),
    setSelectedVersionId: vi.fn(),
    selectPublishedTool: vi.fn(),
    applyCatalogTemplate: vi.fn(),
    updateImportDraft: vi.fn(),
    resetImportDraft: vi.fn(),
    updateAssignmentDraft: vi.fn(),
    updateAuthBindingDraft: vi.fn(),
    togglePublishCandidate: vi.fn(),
    setPublishAlias: vi.fn(),
    setEnableAfterPublish: vi.fn(),
    togglePublishedToolSelection: vi.fn(),
    setInteractionPayloadText: vi.fn(),
    importFromDraft: vi.fn(),
    convertSelectedConnector: vi.fn(),
    publishSelectedTools: vi.fn(),
    unpublishSelectedTools: vi.fn(),
    rollbackSelectedVersion: vi.fn(),
    updateConnectorEnabled: vi.fn(),
    saveAssignmentDraft: vi.fn(),
    saveAuthBinding: vi.fn(),
    resumeInteraction: vi.fn(),
    refresh: vi.fn(),
    refreshHealth: vi.fn(),
    ...overrides,
  } as unknown as ConnectorsController;
  return controller;
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
  localStorage.clear();
});

function stubSetupSurface(): SetupSurfaceProps {
  return {
    gatewayDraft: "http://127.0.0.1:18789/",
    onGatewayDraftChange: vi.fn(),
    tokenDraft: "",
    onTokenDraftChange: vi.fn(),
    tokenConfigured: true,
    healthState: "up",
    wsState: "connected",
    onSaveConnection: vi.fn(async () => {}),
    onReconnect: vi.fn(async () => {}),
    onClearToken: vi.fn(async () => {}),
    onOpenSetupWizard: vi.fn(),
    onOpenGuidedTour: vi.fn(),
    opsUxConfig: DEFAULT_OPSUX_RUNTIME_CONFIG,
    opsUxConfigError: null,
    onPatchOpsUxControls: vi.fn(),
    usageChartsEnabled: false,
  };
}

function stubPolicyController(
  overrides: Partial<ExecassPolicyController> = {},
): ExecassPolicyController {
  return {
    phase: "loaded",
    policy: fixturePolicyResponse(),
    error: null,
    draft: null,
    conflict: false,
    updateBusy: false,
    refresh: vi.fn(async () => {}),
    beginDraft: vi.fn(),
    setDraftProfile: vi.fn(),
    setDraftRule: vi.fn(),
    setDraftChangeSummary: vi.fn(),
    discardDraft: vi.fn(),
    reconcileDraft: vi.fn(() => ({ ok: true as const, message: "rebased" })),
    applyDraft: vi.fn(async () => ({ ok: true as const, message: "ok" })),
    ...overrides,
  };
}

async function render(
  controller: ConnectorsController,
  activeRoomId: string | null,
  setupSurface: SetupSurfaceProps = stubSetupSurface(),
  policyController: ExecassPolicyController = stubPolicyController(),
) {
  await act(async () => {
    if (!root) {
      root = createRoot(container);
    }
    root.render(
      <ConnectorsPage
        controller={controller}
        onOpenSimpleIntegrationWizard={() => {}}
        activeRoomId={activeRoomId}
        setupSurface={setupSurface}
        policyController={policyController}
      />,
    );
  });
}

function tabButton(label: string): HTMLButtonElement | undefined {
  return Array.from(
    container.querySelectorAll<HTMLButtonElement>(
      ".mc-connectors-tab-bar button",
    ),
  ).find((button) => button.textContent?.startsWith(label));
}

function pinButton(): HTMLButtonElement | undefined {
  return Array.from(container.querySelectorAll("button")).find(
    (button) =>
      button.getAttribute("aria-label") === "Pin Connectors to Office",
  );
}

describe("ConnectorsPage Basement room seam", () => {
  it("lands on connector management when the Connectors room is requested", async () => {
    await render(stubController(), "connectors");
    expect(tabButton("Registry")?.className).toContain("active");
    expect(container.textContent).toContain("Installed registry");
  });

  it("lands the Setup room on its distinct product surface, not the connector tabs", async () => {
    await render(stubController(), "setup");
    expect(
      container.querySelector('[data-testid="setup-room-page"]'),
    ).toBeTruthy();
    expect(container.textContent).toContain("Gateway connection");
    expect(container.textContent).toContain("Feature switches");
    expect(container.textContent).toContain("Open setup wizard");
    expect(container.textContent).toContain("Start guided tour");
    // The connector tab strip and quick-setup cards belong to the
    // Connectors room and must not leak into Setup.
    expect(container.querySelector(".mc-connectors-tab-bar")).toBeNull();
    expect(container.textContent).not.toContain("Quick Setup");
  });

  it.each([
    ["disabled", { enabled: false, availability: "disabled" }],
    ["unsupported", { enabled: true, availability: "unsupported" }],
    ["error", { enabled: true, availability: "error" }],
    ["cold loading", { enabled: true, availability: "loading" }],
  ] as const)(
    "keeps Setup available through a %s Connectors controller",
    async (_label, override) => {
      const controller = stubController({
        ...override,
        installedConnectors: [],
      });
      await render(controller, "setup");

      expect(container.querySelector('[data-testid="setup-room-page"]')).not.toBeNull();
      expect(container.textContent).toContain("Gateway connection");
      expect(container.textContent).not.toContain("Connectors are disabled");
      expect(container.textContent).not.toContain("Connectors surface unavailable");
      expect(container.textContent).not.toContain("Connectors failed to load");
      expect(container.textContent).not.toContain("Loading Connectors");
    },
  );

  it("lands the Policy room on its distinct owner-language surface", async () => {
    await render(stubController(), "policy");
    expect(
      container.querySelector('[data-testid="policy-room-page"]'),
    ).toBeTruthy();
    expect(container.textContent).toContain("The deal");
    expect(container.textContent).toContain("Autonomy profile");
    // The connector tab strip belongs to the Connectors room only.
    expect(container.querySelector(".mc-connectors-tab-bar")).toBeNull();
    expect(container.textContent).not.toContain("Quick Setup");
  });

  it.each([
    ["disabled", { enabled: false, availability: "disabled" }],
    ["unsupported", { enabled: true, availability: "unsupported" }],
    ["error", { enabled: true, availability: "error" }],
    ["cold loading", { enabled: true, availability: "loading" }],
  ] as const)(
    "keeps Policy available through a %s Connectors controller",
    async (_label, override) => {
      const controller = stubController({
        ...override,
        installedConnectors: [],
      });
      await render(controller, "policy");

      expect(
        container.querySelector('[data-testid="policy-room-page"]'),
      ).not.toBeNull();
      expect(container.textContent).toContain("Autonomy profile");
      expect(container.textContent).not.toContain("Connectors are disabled");
      expect(container.textContent).not.toContain("Connectors surface unavailable");
      expect(container.textContent).not.toContain("Connectors failed to load");
      expect(container.textContent).not.toContain("Loading Connectors");
    },
  );

  it("keeps the user's connector tab across a Policy room visit and relands exactly", async () => {
    const controller = stubController();
    await render(controller, "connectors");
    await act(async () => tabButton("Catalog")!.click());
    expect(tabButton("Catalog")?.className).toContain("active");

    await render(controller, "policy");
    expect(
      container.querySelector('[data-testid="policy-room-page"]'),
    ).toBeTruthy();

    await render(controller, "connectors");
    expect(tabButton("Registry")?.className).toContain("active");
  });

  it("offers the Policy pin only under the Policy room identity", async () => {
    const controller = stubController();
    await render(controller, "policy");
    const policyPin = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("aria-label") === "Pin Policy to Office",
    );
    expect(policyPin).toBeTruthy();

    await render(controller, "connectors");
    const stalePolicyPin = Array.from(
      container.querySelectorAll("button"),
    ).find(
      (button) => button.getAttribute("aria-label") === "Pin Policy to Office",
    );
    expect(stalePolicyPin).toBeUndefined();
  });

  it("keeps connector quick setup reachable inside the Connectors room", async () => {
    await render(stubController(), "connectors");
    await act(async () => tabButton("Setup")!.click());
    expect(tabButton("Setup")?.className).toContain("active");
    expect(container.textContent).toContain("Quick Setup");
  });

  it("keeps the user's tab across unrelated rerenders and relands only on a real room change", async () => {
    const controller = stubController();
    await render(controller, "connectors");
    expect(tabButton("Registry")?.className).toContain("active");

    // The user picks Catalog; an unrelated rerender must not reset it.
    await act(async () => tabButton("Catalog")!.click());
    expect(tabButton("Catalog")?.className).toContain("active");
    await render(controller, "connectors");
    expect(tabButton("Catalog")?.className).toContain("active");

    // Visiting the Setup room shows its surface without touching the
    // Connectors tab truth; the exact room change back relands Registry.
    await render(controller, "setup");
    expect(
      container.querySelector('[data-testid="setup-room-page"]'),
    ).toBeTruthy();
    await render(controller, "connectors");
    expect(tabButton("Registry")?.className).toContain("active");

    // Internal tab choice survives while the room identity is unchanged.
    await act(async () => tabButton("Import")!.click());
    expect(tabButton("Import")?.className).toContain("active");
    await render(controller, "connectors");
    expect(tabButton("Import")?.className).toContain("active");

    // Leaving for an unrelated room and returning relands honestly.
    await render(controller, "boards");
    await render(controller, "connectors");
    expect(tabButton("Registry")?.className).toContain("active");
  });

  it("offers the Connectors pin only under the Connectors room identity", async () => {
    const controller = stubController();
    await render(controller, "connectors");
    const pin = pinButton();
    expect(pin).toBeTruthy();
    await act(async () => pin!.click());
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas",
    );

    await render(controller, "setup");
    expect(pinButton()).toBeUndefined();
  });

  it("offers the Setup pin only on the Setup room surface", async () => {
    const controller = stubController();
    await render(controller, "setup");
    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("aria-label") === "Pin Setup to Office",
    );
    expect(pin).toBeTruthy();

    await render(controller, "connectors");
    expect(
      Array.from(container.querySelectorAll("button")).find(
        (button) => button.getAttribute("aria-label") === "Pin Setup to Office",
      ),
    ).toBeUndefined();
  });

  it("keeps every honest state and exposes no dead pin on any of them", async () => {
    const states: Array<[Partial<Record<string, unknown>>, string]> = [
      [
        { enabled: false, availability: "disabled" },
        "Connectors are disabled",
      ],
      [
        { availability: "unsupported" },
        "Connectors surface unavailable",
      ],
      [
        { availability: "error", availabilityMessage: "boom" },
        "Connectors failed to load",
      ],
      [{ availability: "loading" }, "Loading Connectors"],
    ];
    for (const [overrides, title] of states) {
      await render(stubController(overrides), "connectors");
      expect(container.textContent).toContain(title);
      expect(pinButton()).toBeUndefined();
      await act(async () => root?.unmount());
      root = null;
      container.replaceChildren();
    }
  });
});
