// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  Agent,
  RuntimeConnectionSettings,
  RuntimeRoutingConfigResponse,
} from "../../types";
import { PeopleRoutingSection } from "./PeopleRoutingSection";
import {
  type PeopleRoutingController,
  usePeopleRoutingController,
} from "./usePeopleRoutingController";

const apiMocks = vi.hoisted(() => ({
  getRuntimeConfig: vi.fn(),
  updateRuntimeConfig: vi.fn(),
}));

vi.mock("../../lib/api", () => apiMocks);

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
    memory_binding: null,
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
let latestController: PeopleRoutingController | null = null;

beforeEach(() => {
  vi.clearAllMocks();
  apiMocks.getRuntimeConfig.mockResolvedValue({
    config: { routing: routingFixture() },
  });
  apiMocks.updateRuntimeConfig.mockResolvedValue({
    config: { routing: routingFixture() },
  });
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  latestController = null;
  container.remove();
});

function Harness({
  settings,
  agents,
  tokenConfigured = true,
}: {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  tokenConfigured?: boolean;
}) {
  const controller = usePeopleRoutingController({ settings, tokenConfigured, agents });
  useEffect(() => {
    latestController = controller;
  }, [controller]);
  return <PeopleRoutingSection controller={controller} agents={agents} />;
}

async function render(
  settings: RuntimeConnectionSettings = SETTINGS,
  agents: Agent[] = AGENTS,
  tokenConfigured = true,
) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(
      <Harness
        settings={settings}
        agents={agents}
        tokenConfigured={tokenConfigured}
      />,
    );
  });
}

function deferred<T>() {
  let resolve: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve: resolve! };
}

async function clickButton(label: string) {
  const button = Array.from(container.querySelectorAll("button")).find(
    (candidate) => candidate.textContent?.includes(label),
  );
  expect(button, `missing button ${label}`).toBeTruthy();
  await act(async () => button!.click());
}

async function setLabeledSelect(labelText: string, value: string) {
  const label = Array.from(container.querySelectorAll("label")).find(
    (candidate) => candidate.textContent?.trim().startsWith(labelText),
  );
  const select = label?.querySelector("select");
  expect(select, `missing select ${labelText}`).toBeTruthy();
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(select),
    "value",
  )?.set;
  setter?.call(select, value);
  await act(async () => {
    select!.dispatchEvent(new Event("change", { bubbles: true }));
  });
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

describe("PeopleRoutingSection authoritative Directory home", () => {
  it("presents the human card with its assignment and memory chips plus the setup view", async () => {
    await render();

    expect(container.textContent).toContain("People And Routing");
    expect(container.textContent).toContain("local-operator");
    expect(container.textContent).toContain("assistant: Local Assistant");
    expect(container.textContent).toContain("runtime default memory");
    await clickButton("Routing Setup");
    expect(container.textContent).toContain(
      "Humans currently active in routing.",
    );
    expect(container.textContent).toContain("Unknown DMs");
    expect(container.textContent).toContain("Unknown shared-space messages");
  });

  it("saves the complete routing configuration exactly as drafted", async () => {
    const savedRouting = routingFixture();
    savedRouting.assistant_assignments = [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "agent-root",
        enabled: true,
      },
    ];
    apiMocks.updateRuntimeConfig.mockResolvedValueOnce({
      config: { routing: savedRouting },
    });
    await render();

    await setLabeledSelect("Assistant", "agent-root");
    await clickButton("Save Routing");

    expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledWith(SETTINGS, {
      routing: savedRouting,
    });
    expect(container.textContent).toContain("assistant: Root");
    expect(container.textContent).toContain("People and routing saved.");
  });

  it("retains the draft and stays honest when the save fails", async () => {
    apiMocks.updateRuntimeConfig.mockRejectedValueOnce(new Error("gateway down"));
    await render();

    await setLabeledSelect("Assistant", "agent-root");
    await clickButton("Save Routing");

    expect(container.textContent).toContain(
      "Saving people and routing failed:",
    );
    expect(container.textContent).toContain("gateway down");
    // The failed save must retain the owner's draft: the pending assistant
    // stays selected and Save Routing stays armed for a retry.
    expect(container.textContent).toContain("assistant: Root");
    const save = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("Save Routing"),
    );
    expect(save?.disabled).toBe(false);
  });

  it("blocks an invalid platform link before any write reaches the gateway", async () => {
    await render();

    await clickButton("Add Link");
    // Provider defaults to discord but the platform user id stays empty.
    await setLabeledInput("Label", "half-filled link");
    await clickButton("Save Routing");

    expect(container.textContent).toContain(
      "Every linked account needs both a provider and that person’s platform user ID.",
    );
    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
  });

  it("refuses to remove the local operator and resets the draft on demand", async () => {
    await render();

    await clickButton("Remove Person");
    expect(container.textContent).toContain(
      "You cannot remove the local app operator",
    );
    expect(container.textContent).toContain("local-operator");

    await setLabeledSelect("Assistant", "agent-root");
    expect(container.textContent).toContain("assistant: Root");
    await clickButton("Reset");
    expect(container.textContent).toContain("assistant: Local Assistant");
    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
  });

  it("shows the honest unconfigured and load-failure states", async () => {
    await render({ gateway_url: "" });
    expect(container.textContent).toContain(
      "Connect Mission Control to the gateway before you manage people and routing.",
    );

    apiMocks.getRuntimeConfig.mockRejectedValue(new Error("boom-load"));
    await act(async () => root?.unmount());
    root = null;
    await render();
    expect(container.textContent).toContain(
      "People and routing could not load.",
    );
    expect(container.textContent).toContain("boom-load");
  });

  it("does not fetch until a token exists, then loads when one is configured", async () => {
    await render(SETTINGS, AGENTS, false);

    expect(apiMocks.getRuntimeConfig).not.toHaveBeenCalled();
    expect(container.textContent).toContain(
      "Connect Mission Control to the gateway before you manage people and routing.",
    );

    await render(SETTINGS, AGENTS, true);
    expect(apiMocks.getRuntimeConfig).toHaveBeenCalledTimes(1);
    expect(container.textContent).toContain("local-operator");
  });

  it("keeps the newest refresh truth when an earlier read resolves late", async () => {
    await render();
    apiMocks.getRuntimeConfig.mockClear();
    const first = deferred<{ config: { routing: RuntimeRoutingConfigResponse } }>();
    const second = deferred<{ config: { routing: RuntimeRoutingConfigResponse } }>();
    const firstRouting = routingFixture();
    firstRouting.human_identities[0]!.display_name = "Old truth";
    const secondRouting = routingFixture();
    secondRouting.human_identities[0]!.display_name = "New truth";
    apiMocks.getRuntimeConfig
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    await act(async () => {
      void latestController!.loadRoutingConfig();
      void latestController!.loadRoutingConfig();
    });
    expect(apiMocks.getRuntimeConfig).toHaveBeenCalledTimes(2);

    await act(async () => {
      second.resolve({ config: { routing: secondRouting } });
      await second.promise;
    });
    expect(container.textContent).toContain("New truth");

    await act(async () => {
      first.resolve({ config: { routing: firstRouting } });
      await first.promise;
    });
    expect(container.textContent).toContain("New truth");
    expect(container.textContent).not.toContain("Old truth");
    const refresh = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("Refresh"),
    );
    expect(refresh?.disabled).toBe(false);
  });

  it("only starts one save for same-tick duplicate requests", async () => {
    await render();
    await setLabeledSelect("Assistant", "agent-root");
    const pendingSave = deferred<{ config: { routing: RuntimeRoutingConfigResponse } }>();
    apiMocks.updateRuntimeConfig.mockReturnValueOnce(pendingSave.promise);

    await act(async () => {
      void latestController!.saveRoutingDraft();
      void latestController!.saveRoutingDraft();
    });
    expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledTimes(1);

    await act(async () => {
      pendingSave.resolve({ config: { routing: routingFixture() } });
      await pendingSave.promise;
    });
  });

  it("does not let a pre-save read overwrite the saved routing truth", async () => {
    await render();
    await setLabeledSelect("Assistant", "agent-root");
    const pendingRead = deferred<{ config: { routing: RuntimeRoutingConfigResponse } }>();
    const savedRouting = routingFixture();
    savedRouting.assistant_assignments = [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "agent-root",
        enabled: true,
      },
    ];
    apiMocks.getRuntimeConfig.mockReturnValueOnce(pendingRead.promise);
    apiMocks.updateRuntimeConfig.mockResolvedValueOnce({ config: { routing: savedRouting } });

    let save: Promise<void> | undefined;
    await act(async () => {
      void latestController!.loadRoutingConfig();
      save = latestController!.saveRoutingDraft();
      await save;
    });
    expect(container.textContent).toContain("assistant: Root");

    await act(async () => {
      pendingRead.resolve({ config: { routing: routingFixture() } });
      await pendingRead.promise;
    });
    expect(container.textContent).toContain("assistant: Root");
    expect(container.textContent).not.toContain("assistant: Local Assistant");
  });

  it("refuses agent-routing cleanup when the authoritative read fails", async () => {
    await render();
    apiMocks.getRuntimeConfig.mockRejectedValueOnce(new Error("routing unavailable"));

    await act(async () => {
      await expect(latestController!.detachAgentRouting("default")).rejects.toThrow(
        "Could not read the current people and routing configuration",
      );
    });
    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
  });

  it("blocks a save instead of dropping a lane policy for an unavailable agent", async () => {
    const routing = routingFixture();
    routing.lane_memory_policies = [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "temporarily-missing",
        memory_mode: "inherit_runtime",
        lane_id: "human:local-operator:assistant:temporarily-missing",
        local_memory_sources: [],
      },
    ];
    apiMocks.getRuntimeConfig.mockResolvedValueOnce({ config: { routing } });
    await render();
    await setLabeledSelect("Assistant", "agent-root");
    await clickButton("Save Routing");

    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
    expect(container.textContent).toContain(
      'A lane memory policy still references unavailable assistant "temporarily-missing".',
    );
  });

  it("shows four people per page and marks stable human IDs read-only to assistive tech", async () => {
    const routing = routingFixture();
    routing.human_identities = Array.from({ length: 5 }, (_, index) => ({
      human_identity_id: `person-${index + 1}`,
      display_name: `Person ${index + 1}`,
      enabled: true,
    }));
    routing.local_operator_human_identity_id = "person-1";
    routing.assistant_assignments = routing.human_identities.map((human) => ({
      human_identity_id: human.human_identity_id,
      assistant_agent_id: "default",
      enabled: true,
    }));
    apiMocks.getRuntimeConfig.mockResolvedValueOnce({ config: { routing } });
    await render();

    expect(container.textContent).toContain("Person 1");
    expect(container.textContent).toContain("Person 4");
    expect(container.textContent).not.toContain("Person 5");
    const humanId = Array.from(container.querySelectorAll("label")).find(
      (label) => label.textContent?.trim().startsWith("Human ID"),
    )?.querySelector("input");
    expect(humanId?.getAttribute("aria-readonly")).toBe("true");

    await clickButton("Next");
    expect(container.textContent).toContain("Person 5");
    expect(container.textContent).not.toContain("Person 1");
  });

  it("refreshes through the real load seam", async () => {
    await render();
    apiMocks.getRuntimeConfig.mockClear();
    await clickButton("Refresh");
    expect(apiMocks.getRuntimeConfig).toHaveBeenCalledTimes(1);
  });

  it("confirms once before Refresh discards an unsaved routing draft", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    try {
      await render();
      await setLabeledSelect("Assistant", "agent-root");
      apiMocks.getRuntimeConfig.mockClear();

      await clickButton("Refresh");
      expect(confirmSpy).toHaveBeenCalledWith(
        "Refresh people and routing? Your unsaved routing changes will be discarded.",
      );
      expect(apiMocks.getRuntimeConfig).not.toHaveBeenCalled();
      expect(container.textContent).toContain("assistant: Root");

      confirmSpy.mockReturnValue(true);
      await clickButton("Refresh");
      expect(apiMocks.getRuntimeConfig).toHaveBeenCalledTimes(1);
      expect(container.textContent).toContain("assistant: Local Assistant");
    } finally {
      confirmSpy.mockRestore();
    }
  });
});
