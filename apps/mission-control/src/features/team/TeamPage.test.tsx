// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  Agent,
  AuthProfileResponse,
  RuntimeConnectionSettings,
  RuntimeRoutingConfigResponse,
} from "../../types";
import { buildAgentOrgModel } from "../strategy/strategyOrg";
import type { useStrategyController } from "../strategy/useStrategyController";
import { usePeopleRoutingController } from "../peopleRouting/usePeopleRoutingController";
import { TeamPage } from "./TeamPage";

const apiMocks = vi.hoisted(() => ({
  createAgent: vi.fn(),
  getRuntimeConfig: vi.fn(),
  listProviderCapabilities: vi.fn(),
  listProviderModels: vi.fn(),
  removeAgent: vi.fn(),
  updateRuntimeConfig: vi.fn(),
  updateAgent: vi.fn(),
}));

vi.mock("../../lib/api", () => ({
  ...apiMocks,
  GatewayApiError: class GatewayApiError extends Error {
    status: number;
    constructor(status: number, message: string) {
      super(message);
      this.status = status;
    }
  },
}));

type StrategyController = ReturnType<typeof useStrategyController>;

const SETTINGS: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

const AGENTS: Agent[] = [
  {
    agent_id: "agent-root",
    name: "Root",
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "standard",
    role_label: "Operations Director",
    reports_to_agent_id: null,
    memory_binding: null,
  },
  {
    agent_id: "default",
    name: "Local Assistant",
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "default",
    role_label: "Reliability Lead",
    reports_to_agent_id: "agent-root",
    memory_binding: {
      binding_id: "mno-default",
      provider_kind: "modelnumquamoblita",
      base_url: "http://127.0.0.1:4411",
      auth_mode: "none",
      enabled: true,
      trusted_local_operator_actions: true,
    },
  },
];

function routingFixture(): RuntimeRoutingConfigResponse {
  return {
    enabled: true,
    use_channel_defaults_as_fallback: false,
    local_operator_human_identity_id: "local-operator",
    dm_unmapped_policy: "approval_required",
    shared_unmapped_policy: "block",
    human_identities: [
      { human_identity_id: "local-operator", display_name: "You", enabled: true },
    ],
    platform_identity_links: [],
    assistant_assignments: [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "default",
        enabled: true,
      },
    ],
    lane_memory_policies: [],
  };
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  apiMocks.getRuntimeConfig.mockResolvedValue({
    config: { routing: routingFixture() },
  });
  apiMocks.updateRuntimeConfig.mockResolvedValue({
    config: { routing: routingFixture() },
  });
  apiMocks.listProviderCapabilities.mockResolvedValue({ items: [] });
  apiMocks.listProviderModels.mockResolvedValue({
    auth_profile_id: null,
    items: [],
  });
  apiMocks.createAgent.mockResolvedValue({});
  apiMocks.updateAgent.mockResolvedValue({});
  apiMocks.removeAgent.mockResolvedValue({});
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

function stubStrategyController(overrides?: Partial<Record<string, unknown>>) {
  const controller = {
    enabled: true,
    presets: [],
    org: buildAgentOrgModel(AGENTS),
    queueRefresh: vi.fn(),
    createBootstrapPreset: vi.fn(),
    updateBootstrapPreset: vi.fn(),
    importBootstrapPreset: vi.fn(),
    exportBootstrapPreset: vi.fn(),
    ...overrides,
  } as unknown as StrategyController;
  return controller;
}

function TeamPageHarness({
  controller,
  agents,
  activeRoomId,
  authProfiles,
  onOpenPeopleRouting,
  onOpenAgentMemory,
}: {
  controller: StrategyController;
  agents: Agent[];
  activeRoomId: string | null;
  authProfiles: AuthProfileResponse[];
  onOpenPeopleRouting: () => void;
  onOpenAgentMemory?: (agentId: string) => void;
}) {
  // The real shared controller: TeamPage reads routed-people facts from the
  // same single People & Routing authority that Directory edits.
  const peopleRouting = usePeopleRoutingController({
    settings: SETTINGS,
    tokenConfigured: true,
    agents,
  });
  return (
    <TeamPage
      agents={agents}
      activeJobCount={0}
      settings={SETTINGS}
      strategyController={controller}
      onRefresh={() => {}}
      activeRoomId={activeRoomId}
      authProfiles={authProfiles}
      peopleRouting={peopleRouting}
      onOpenPeopleRouting={onOpenPeopleRouting}
      onOpenAgentMemory={onOpenAgentMemory}
    />
  );
}

async function render(
  controller: StrategyController,
  agents: Agent[] = AGENTS,
  options?: {
    activeRoomId?: string | null;
    authProfiles?: AuthProfileResponse[];
    onOpenPeopleRouting?: () => void;
    onOpenAgentMemory?: (agentId: string) => void;
  },
) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(
      <TeamPageHarness
        controller={controller}
        agents={agents}
        activeRoomId={options?.activeRoomId ?? "staff"}
        authProfiles={options?.authProfiles ?? []}
        onOpenPeopleRouting={options?.onOpenPeopleRouting ?? (() => {})}
        onOpenAgentMemory={options?.onOpenAgentMemory}
      />,
    );
  });
}

function findStaffCard(agentName: string): HTMLElement {
  const card = Array.from(container.querySelectorAll(".mc-team-card")).find(
    (candidate) =>
      candidate.querySelector(".mc-team-card-name")?.textContent === agentName,
  );
  expect(card, `missing staff card for ${agentName}`).toBeTruthy();
  return card as HTMLElement;
}

async function clickButton(label: string) {
  const button = Array.from(container.querySelectorAll("button")).find(
    (candidate) => candidate.textContent?.includes(label),
  );
  expect(button, `missing button ${label}`).toBeTruthy();
  await act(async () => button!.click());
}

async function setLabeledInput(labelText: string, value: string) {
  const label = Array.from(container.querySelectorAll("label")).find(
    (candidate) => candidate.textContent?.trim().startsWith(labelText),
  );
  const input = label?.querySelector("input");
  expect(input, `missing input ${labelText}`).toBeTruthy();
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(input),
    "value",
  )?.set;
  setter?.call(input, value);
  await act(async () => {
    input!.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("TeamPage Trenches parity", () => {
  it("renders distinct content for the Team sections and pins without mutations", async () => {
    await render(stubStrategyController());

    // Agents (default section): role cards and memory bindings stay presented.
    expect(container.textContent).toContain("Root");
    expect(container.textContent).toContain("Operations Director");
    expect(container.textContent).toContain("Memory lane: mno-default");
    expect(container.textContent).toContain("Reports to Root");
    expect(container.querySelectorAll(".mc-team-card")).toHaveLength(
      AGENTS.length,
    );
    expect(container.textContent?.toLowerCase()).not.toContain(
      "temporary worker",
    );
    expect(container.textContent?.toLowerCase()).not.toContain("task worker");
    expect(
      Array.from(container.querySelectorAll("button")).filter(
        (button) => button.textContent?.includes("Role Card"),
      ).length,
    ).toBeGreaterThan(0);
    // Routed-people facts come from the shared authority.
    expect(container.textContent).toContain("Routed people: 1");

    // Strategy-gated Presets and Org surfaces stay reachable.
    await clickButton("Presets");
    expect(container.textContent).toContain("Bootstrap Presets");
    await clickButton("Org");
    expect(container.textContent).toContain("Org View");
    expect(container.textContent).toContain("Local Assistant");

    // The registry-backed pin adds the shortcut without touching Team data.
    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) =>
        button.getAttribute("aria-label") === "Pin Staff Directory to Office",
    );
    expect(pin).toBeTruthy();
    await act(async () => pin!.click());
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas",
    );
    expect(apiMocks.createAgent).not.toHaveBeenCalled();
    expect(apiMocks.updateAgent).not.toHaveBeenCalled();
    expect(apiMocks.removeAgent).not.toHaveBeenCalled();
    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
  });

  it("hands People & Routing off to the Directory room by stable id without a second editable truth", async () => {
    const onOpenPeopleRouting = vi.fn();
    await render(stubStrategyController(), AGENTS, { onOpenPeopleRouting });

    // The affordance stays reachable but no editable routing surface mounts here.
    await clickButton("People & Routing");
    expect(onOpenPeopleRouting).toHaveBeenCalledTimes(1);
    expect(container.textContent).not.toContain("People And Routing");
    expect(container.textContent).not.toContain("Save Routing");
    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
  });

  it("opens the authoritative role card with manager and routed-person context", async () => {
    await render(stubStrategyController());

    const localAssistantCard = Array.from(
      container.querySelectorAll<HTMLElement>(".mc-team-card"),
    ).find((card) => card.textContent?.includes("Local Assistant"));
    const roleCardButton = Array.from(
      localAssistantCard?.querySelectorAll("button") ?? [],
    ).find((button) => button.textContent?.includes("Role Card"));
    expect(roleCardButton).toBeTruthy();
    await act(async () => roleCardButton!.click());

    expect(container.textContent).toContain("Local Assistant — Role Card");
    expect(container.textContent).toContain("Manager Chain");
    expect(container.textContent).toContain("Root");
    expect(container.textContent).toContain("Routed People");
    expect(container.textContent).toContain("You");
    expect(container.textContent).toContain("Memory lane: mno-default");
  });

  it("preserves agent creation through the real modal form", async () => {
    await render(stubStrategyController());

    await clickButton("New Agent");
    expect(container.textContent).toContain("Create Agent");
    await setLabeledInput("Agent ID", "agent-beta");
    await setLabeledInput("Name", "Beta Agent");
    await clickButton("Create Agent");
    expect(apiMocks.createAgent).toHaveBeenCalledWith(
      SETTINGS,
      expect.objectContaining({ agent_id: "agent-beta", name: "Beta Agent" }),
    );
  });

  it("preserves edit and confirmed removal against the authoritative gateway", async () => {
    const confirmSpy = vi
      .spyOn(window, "confirm")
      .mockImplementation(() => true);
    try {
      await render(stubStrategyController());

      const edit = Array.from(container.querySelectorAll("button")).find(
        (button) => button.getAttribute("title") === "Edit agent",
      );
      expect(edit).toBeTruthy();
      await act(async () => edit!.click());
      expect(container.textContent).toContain("Edit Agent");
      await setLabeledInput("Name", "Root Prime");
      await clickButton("Save Changes");
      expect(apiMocks.updateAgent).toHaveBeenCalledWith(
        SETTINGS,
        "agent-root",
        expect.objectContaining({ name: "Root Prime" }),
      );

      await clickButton("Remove");
      expect(confirmSpy).toHaveBeenCalled();
      expect(apiMocks.removeAgent).toHaveBeenCalledWith(SETTINGS, "agent-root");
    } finally {
      confirmSpy.mockRestore();
    }
  });

  it("stops removal before the agent changes when routing cannot be read", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    try {
      await render(stubStrategyController());
      apiMocks.getRuntimeConfig.mockRejectedValueOnce(
        new Error("routing read failed"),
      );
      apiMocks.updateRuntimeConfig.mockClear();
      apiMocks.removeAgent.mockClear();

      await clickButton("Remove");

      expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
      expect(apiMocks.removeAgent).not.toHaveBeenCalled();
      expect(container.textContent).toContain(
        "was stopped before the agent changed",
      );
      expect(container.textContent).toContain(
        "Could not read the current people and routing configuration",
      );
    } finally {
      confirmSpy.mockRestore();
    }
  });

  it("restores exact routing when the gateway refuses agent removal", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const originalRouting = routingFixture();
    const detachedRouting = routingFixture();
    detachedRouting.assistant_assignments = [];
    try {
      await render(stubStrategyController());
      apiMocks.updateRuntimeConfig
        .mockResolvedValueOnce({ config: { routing: detachedRouting } })
        .mockResolvedValueOnce({ config: { routing: originalRouting } });
      apiMocks.removeAgent.mockRejectedValueOnce(
        new Error("agent still owns scheduled jobs"),
      );

      const assistantCard = Array.from(
        container.querySelectorAll<HTMLElement>(".mc-team-card"),
      ).find((card) => card.textContent?.includes("Local Assistant"));
      const removeButton = Array.from(
        assistantCard?.querySelectorAll("button") ?? [],
      ).find((button) => button.textContent?.includes("Remove"));
      expect(removeButton).toBeTruthy();
      await act(async () => removeButton!.click());

      expect(apiMocks.removeAgent).toHaveBeenCalledWith(
        SETTINGS,
        "default",
      );
      expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledTimes(2);
      expect(apiMocks.updateRuntimeConfig.mock.calls[0]?.[1]).toEqual({
        routing: detachedRouting,
      });
      expect(apiMocks.updateRuntimeConfig.mock.calls[1]?.[1]).toEqual({
        routing: originalRouting,
      });
      expect(container.textContent).toContain(
        "People & Routing was restored to its exact prior state.",
      );
    } finally {
      confirmSpy.mockRestore();
    }
  });

  it("hides the strategy-gated surfaces when the hub is off but keeps the pin", async () => {
    await render(stubStrategyController({ enabled: false }));

    const sectionLabels = Array.from(
      container.querySelectorAll(".mc-page-section-tabs button"),
    ).map((button) => button.textContent?.trim());
    expect(sectionLabels).toContain("Agents");
    expect(sectionLabels).toContain("People & Routing");
    expect(sectionLabels).not.toContain("Presets");
    expect(sectionLabels).not.toContain("Org");
    expect(
      Array.from(container.querySelectorAll("button")).find(
        (button) =>
          button.getAttribute("aria-label") === "Pin Staff Directory to Office",
      ),
    ).toBeTruthy();
  });
});

const OLLAMA_CAPABILITY = {
  provider: "ollama",
  supports_streaming: true,
  supports_tools: true,
  supports_json_mode: true,
  supports_vision: false,
  max_context_tokens: 32768,
  error_classes: ["rate_limit", "timeout"],
  retryable_error_classes: ["timeout"],
};

const OLLAMA_PROFILE: AuthProfileResponse = {
  auth_profile_id: "profile-ollama-1",
  provider: "ollama",
  display_name: "Local Ollama",
  auth_mode: "none",
  risk_level: "low",
  enabled: true,
  kill_switch_scope: "none",
  api_base_url: "http://127.0.0.1:11434",
  created_at: 0,
  updated_at: 0,
};

function pinByLabel(label: string): HTMLButtonElement | undefined {
  return Array.from(container.querySelectorAll("button")).find(
    (button) => button.getAttribute("aria-label") === label,
  );
}

function sectionTab(label: string): HTMLButtonElement | undefined {
  return Array.from(
    container.querySelectorAll<HTMLButtonElement>(
      ".mc-page-section-tabs button",
    ),
  ).find((button) => button.textContent?.trim() === label);
}

describe("TeamPage Basement room seam", () => {
  it("lands on the Models surface for the models room and on Agents for staff", async () => {
    const controller = stubStrategyController();
    await render(controller, AGENTS, { activeRoomId: "models" });
    expect(sectionTab("Models & Providers")?.className).toContain(
      "mc-page-section-btn-active",
    );
    expect(container.textContent).not.toContain("Meet Your Agents");
    expect(pinByLabel("Pin Models & Providers to Office")).toBeTruthy();
    expect(pinByLabel("Pin Staff Directory to Office")).toBeUndefined();

    await render(controller, AGENTS, { activeRoomId: "staff" });
    expect(sectionTab("Agents")?.className).toContain(
      "mc-page-section-btn-active",
    );
    expect(container.textContent).toContain("Meet Your Agents");
    expect(pinByLabel("Pin Staff Directory to Office")).toBeTruthy();
    expect(pinByLabel("Pin Models & Providers to Office")).toBeUndefined();
  });

  it("keeps the user's section across unrelated rerenders and relands only on a real room change", async () => {
    const controller = stubStrategyController();
    await render(controller, AGENTS, { activeRoomId: "staff" });
    await clickButton("Models & Providers");
    expect(sectionTab("Models & Providers")?.className).toContain(
      "mc-page-section-btn-active",
    );
    await render(controller, AGENTS, { activeRoomId: "staff" });
    expect(sectionTab("Models & Providers")?.className).toContain(
      "mc-page-section-btn-active",
    );

    await render(controller, AGENTS, { activeRoomId: "models" });
    await clickButton("Presets");
    expect(sectionTab("Presets")?.className).toContain(
      "mc-page-section-btn-active",
    );
    await render(controller, AGENTS, { activeRoomId: "models" });
    expect(sectionTab("Presets")?.className).toContain(
      "mc-page-section-btn-active",
    );

    await render(controller, AGENTS, { activeRoomId: "staff" });
    expect(sectionTab("Agents")?.className).toContain(
      "mc-page-section-btn-active",
    );
  });
});

describe("TeamPage Models & Providers surface", () => {
  it("presents capability, profile, model, and assignment facts honestly", async () => {
    apiMocks.listProviderCapabilities.mockResolvedValue({
      items: [OLLAMA_CAPABILITY],
    });
    apiMocks.listProviderModels.mockResolvedValue({
      auth_profile_id: OLLAMA_PROFILE.auth_profile_id,
      items: [
        { model_id: "qwen3.5-9b-instruct", label: "Qwen 9B" },
        { model_id: "qwen3.5-4b-instruct", label: "Qwen 4B" },
      ],
    });
    await render(stubStrategyController(), AGENTS, {
      activeRoomId: "models",
      authProfiles: [OLLAMA_PROFILE],
    });

    const card = container.querySelector(
      '[data-testid="models-provider-ollama"]',
    );
    expect(card).toBeTruthy();
    expect(card?.textContent).toContain("streaming");
    expect(card?.textContent).toContain("no vision");
    expect(card?.textContent).toContain("tokens");
    expect(card?.textContent).toContain("2 known error classes (1 retryable)");
    expect(card?.textContent).toContain("Local Ollama");
    expect(card?.textContent).toContain("enabled");
    expect(card?.textContent).toContain("risk: low");
    expect(card?.textContent).toContain("http://127.0.0.1:11434");
    expect(card?.textContent).toContain("Profile: Local Ollama");
    expect(card?.textContent).toContain("qwen3.5-9b-instruct");
    expect(card?.textContent).toContain("in use: Root, Local Assistant");
    expect(card?.textContent).toContain("Assigned agents");
    expect(card?.textContent).toContain("Root");
  });

  it("scopes a model-discovery failure to its provider and retries through the real seam", async () => {
    apiMocks.listProviderCapabilities.mockResolvedValue({
      items: [OLLAMA_CAPABILITY, { ...OLLAMA_CAPABILITY, provider: "lmstudio" }],
    });
    apiMocks.listProviderModels.mockImplementation(
      (_settings: unknown, query: { provider: string }) =>
        query.provider === "lmstudio"
          ? Promise.reject(new Error("boom-lmstudio"))
          : Promise.resolve({
              auth_profile_id: null,
              items: [{ model_id: "qwen3.5-9b-instruct", label: "Qwen 9B" }],
            }),
    );
    await render(stubStrategyController(), AGENTS, {
      activeRoomId: "models",
    });

    const ollamaCard = container.querySelector(
      '[data-testid="models-provider-ollama"]',
    );
    const lmstudioCard = container.querySelector(
      '[data-testid="models-provider-lmstudio"]',
    );
    expect(ollamaCard?.textContent).toContain("qwen3.5-9b-instruct");
    expect(ollamaCard?.textContent).not.toContain("Model discovery failed");
    expect(lmstudioCard?.textContent).toContain("boom-lmstudio");

    // The retry drives the real refresh seam and recovers only this provider.
    apiMocks.listProviderModels.mockResolvedValue({
      auth_profile_id: null,
      items: [{ model_id: "recovered-model", label: "Recovered" }],
    });
    const retry = Array.from(
      lmstudioCard?.querySelectorAll("button") ?? [],
    ).find((button) => button.textContent?.includes("Retry"));
    expect(retry).toBeTruthy();
    await act(async () => retry!.click());
    expect(
      apiMocks.listProviderModels,
    ).toHaveBeenCalledWith(SETTINGS, expect.objectContaining({
      provider: "lmstudio",
      refresh: true,
    }));
    expect(
      container
        .querySelector('[data-testid="models-provider-lmstudio"]')
        ?.textContent,
    ).toContain("recovered-model");
  });

  it("keeps a capability failure explicit instead of pretending an empty catalog", async () => {
    apiMocks.listProviderCapabilities.mockRejectedValue(new Error("caps-down"));
    apiMocks.listProviderModels.mockResolvedValue({
      auth_profile_id: OLLAMA_PROFILE.auth_profile_id,
      items: [],
    });
    await render(stubStrategyController(), AGENTS, {
      activeRoomId: "models",
      authProfiles: [OLLAMA_PROFILE],
    });

    expect(container.textContent).toContain(
      "Provider capability facts could not be loaded.",
    );
    expect(container.textContent).toContain("caps-down");
    // Assigned and configured providers stay visible with honest
    // capability-unavailable facts rather than vanishing.
    const card = container.querySelector(
      '[data-testid="models-provider-ollama"]',
    );
    expect(card?.textContent).toContain("capability facts unavailable");
    expect(card?.textContent).toContain("Local Ollama");
    expect(card?.textContent).toContain("Context window: unknown");
  });

  it("normalizes provider identity and scopes catalogs to each enabled auth profile", async () => {
    const backupProfile: AuthProfileResponse = {
      ...OLLAMA_PROFILE,
      auth_profile_id: "ollama-backup",
      provider: " OLLAMA ",
      display_name: "Backup Ollama",
      api_base_url: "http://127.0.0.1:22468",
    };
    apiMocks.listProviderCapabilities.mockResolvedValue({
      items: [{ ...OLLAMA_CAPABILITY, provider: "Ollama" }],
    });
    apiMocks.listProviderModels.mockImplementation(
      (
        _settings: unknown,
        query: { provider: string; auth_profile_id?: string },
      ) =>
        Promise.resolve({
          auth_profile_id: query.auth_profile_id ?? null,
          items: [
            {
              model_id:
                query.auth_profile_id === "ollama-backup"
                  ? "backup-model"
                  : "primary-model",
              label: "Scoped model",
            },
          ],
        }),
    );
    await render(
      stubStrategyController(),
      [{ ...AGENTS[0], model_provider: " OLLAMA " }],
      {
        activeRoomId: "models",
        authProfiles: [OLLAMA_PROFILE, backupProfile],
      },
    );

    expect(
      container.querySelectorAll('[data-testid="models-provider-ollama"]'),
    ).toHaveLength(1);
    expect(
      container.querySelector(
        '[data-testid="models-catalog-ollama-profile-ollama-1"]',
      )
        ?.textContent,
    ).toContain("primary-model");
    expect(
      container.querySelector('[data-testid="models-catalog-ollama-ollama-backup"]')
        ?.textContent,
    ).toContain("backup-model");
    expect(apiMocks.listProviderModels).toHaveBeenCalledWith(
      SETTINGS,
      expect.objectContaining({
        provider: "ollama",
        auth_profile_id: "profile-ollama-1",
      }),
    );
    expect(apiMocks.listProviderModels).toHaveBeenCalledWith(
      SETTINGS,
      expect.objectContaining({
        provider: "ollama",
        auth_profile_id: "ollama-backup",
      }),
    );
  });

  it("forces every catalog through refresh and rejects older overlapping results", async () => {
    let resolveStale:
      | ((value: {
          auth_profile_id: null;
          items: { model_id: string; label: string }[];
        }) => void)
      | null = null;
    const staleRequest = new Promise<{
      auth_profile_id: null;
      items: { model_id: string; label: string }[];
    }>((resolve) => {
      resolveStale = resolve;
    });
    apiMocks.listProviderCapabilities.mockResolvedValue({
      items: [OLLAMA_CAPABILITY],
    });
    apiMocks.listProviderModels
      .mockImplementationOnce(() => staleRequest)
      .mockResolvedValue({
        auth_profile_id: null,
        items: [{ model_id: "fresh-model", label: "Fresh" }],
      });
    await render(stubStrategyController(), AGENTS, {
      activeRoomId: "models",
    });

    const refresh = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "Refresh",
    );
    expect(refresh).toBeTruthy();
    await act(async () => refresh!.click());
    expect(apiMocks.listProviderModels).toHaveBeenLastCalledWith(
      SETTINGS,
      expect.objectContaining({ provider: "ollama", refresh: true }),
    );
    expect(container.textContent).toContain("fresh-model");

    await act(async () => {
      resolveStale?.({
        auth_profile_id: null,
        items: [{ model_id: "stale-model", label: "Stale" }],
      });
      await staleRequest;
    });
    expect(container.textContent).toContain("fresh-model");
    expect(container.textContent).not.toContain("stale-model");
  });

  it("shows unconfigured assignments without requesting an unsupported model catalog", async () => {
    apiMocks.listProviderCapabilities.mockResolvedValue({
      items: [
        {
          ...OLLAMA_CAPABILITY,
          provider: "unconfigured",
        },
      ],
    });
    await render(
      stubStrategyController(),
      [
        {
          ...AGENTS[0],
          model_provider: "unconfigured",
          model_id: "unconfigured",
        },
      ],
      { activeRoomId: "models" },
    );

    expect(
      container.querySelector('[data-testid="models-provider-unconfigured"]')
        ?.textContent,
    ).toContain(
      "Model discovery does not apply to unconfigured assignments.",
    );
    expect(apiMocks.listProviderModels).not.toHaveBeenCalled();
  });
});

describe("TeamPage exact memory drill-in entry", () => {
  it("opens the exact agent's memory from its own staff card", async () => {
    const onOpenAgentMemory = vi.fn();
    await render(stubStrategyController(), AGENTS, { onOpenAgentMemory });

    const assistantCard = findStaffCard("Local Assistant");
    const memoryButton = Array.from(
      assistantCard.querySelectorAll("button"),
    ).find((button) => button.textContent?.trim() === "Memory");
    expect(memoryButton, "missing Memory button").toBeTruthy();
    await act(async () => memoryButton!.click());
    expect(onOpenAgentMemory).toHaveBeenCalledWith("default");

    const rootCard = findStaffCard("Root");
    const rootMemoryButton = Array.from(
      rootCard.querySelectorAll("button"),
    ).find((button) => button.textContent?.trim() === "Memory");
    await act(async () => rootMemoryButton!.click());
    expect(onOpenAgentMemory).toHaveBeenLastCalledWith("agent-root");
  });

  it("exposes no dead memory control when the memory room is unavailable", async () => {
    await render(stubStrategyController(), AGENTS, {});

    const deadButtons = Array.from(container.querySelectorAll("button")).filter(
      (button) => button.textContent?.trim() === "Memory",
    );
    expect(deadButtons).toHaveLength(0);
  });
});
