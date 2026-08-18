// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { NotifyFn } from "../../app/useAppController";
import type {
  Agent,
  AgentMemoryLaneStatusResponse,
  AgentMemoryStatusResponse,
  RuntimeConnectionSettings,
  RuntimeConfigResponse,
  RuntimeRoutingConfigResponse,
} from "../../types";
import {
  usePeopleRoutingController,
  type PeopleRoutingController,
} from "../peopleRouting/usePeopleRoutingController";
import { MemoryPage } from "./MemoryPage";
import { useMemoryController } from "./useMemoryController";

const apiMocks = vi.hoisted(() => ({
  getAgentMemoryStatus: vi.fn(),
  getAgentMemoryLaneStatuses: vi.fn(),
  listAgentMemoryCards: vi.fn(),
  getAgentMemoryCard: vi.fn(),
  getAgentMemoryAtom: vi.fn(),
  listAgentMemoryEpisodes: vi.fn(),
  getAgentMemoryGraphMap: vi.fn(),
  getAgentMemoryGraphNeighbors: vi.fn(),
  getAgentMemoryTurnWhy: vi.fn(),
  getAgentMemoryCitation: vi.fn(),
  getAgentMemoryRuntimeHealth: vi.fn(),
  getAgentMemoryTelemetrySummary: vi.fn(),
  getAgentMemoryTelemetryTurns: vi.fn(),
  getAgentMemoryDecisionReasons: vi.fn(),
  getRuntimeConfig: vi.fn(),
  updateRuntimeConfig: vi.fn(),
  syncMemorySources: vi.fn(),
}));

vi.mock("../../lib/api", () => apiMocks);

const SETTINGS: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

function makeAgent(agentId: string, name: string, bound: boolean): Agent {
  return {
    agent_id: agentId,
    name,
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "standard",
    role_label: null,
    reports_to_agent_id: null,
    memory_binding: bound
      ? {
          binding_id: `binding-${agentId}`,
          provider_kind: "modelnumquamoblita",
          base_url: "http://127.0.0.1:4411",
          auth_mode: "none",
          enabled: true,
          trusted_local_operator_actions: true,
        }
      : null,
  };
}

const AGENTS: Agent[] = [
  makeAgent("root", "Root", false),
  makeAgent("lyra", "Lyra", true),
];

function makeStatus(agentId: string): AgentMemoryStatusResponse {
  return {
    agent_id: agentId,
    binding_status: "available",
    binding: {
      binding_id: `binding-${agentId}`,
      provider_kind: "modelnumquamoblita",
      base_url: "http://127.0.0.1:4411",
      auth_mode: "none",
      enabled: true,
      trusted_local_operator_actions: true,
    },
    native_surface_availability: {
      cards: true,
      card_detail: true,
      atom_detail: true,
      graph_overview: true,
      graph_neighbors: true,
      episodes: true,
      turn_why: true,
      citation_lookup: true,
      runtime_health: true,
      telemetry_summary: true,
      telemetry_turns: true,
      decision_reasons: true,
    },
    orchestration: {
      enabled: true,
      transport: "http",
      health_status: "ok",
      degrade_mode: false,
      last_error_code: null,
      last_error: null,
    },
    native_runtime_status: null,
    native_runtime_health_mismatch: false,
  };
}

function wrap<T>(agentId: string, data: T) {
  return { agent_id: agentId, binding_id: `binding-${agentId}`, data };
}

function agentName(agentId: string): string {
  return agentId === "lyra" ? "Lyra" : "Root";
}

function cardsPayload(agentId: string) {
  const name = agentName(agentId);
  return {
    ok: true,
    total: 2,
    cards: [
      {
        atom_id: `atom-${agentId}-1`,
        card_id: `card-${agentId}-1`,
        kind: "fact",
        status: "active",
        summary: `${name} fact one`,
      },
      {
        atom_id: `atom-${agentId}-2`,
        card_id: `card-${agentId}-2`,
        kind: "note",
        status: "active",
        summary: `${name} fact two`,
      },
    ],
  };
}

function episodesPayload(agentId: string) {
  return {
    ok: true,
    total: 1,
    episodes: [
      {
        episode_id: `ep-${agentId}-1`,
        label: `${agentName(agentId)} episode`,
        status: "ok",
        run_id: `run-${agentId}-1`,
        updated_at_utc: "2026-07-01T00:00:00Z",
      },
    ],
  };
}

function graphMapPayload(agentId: string) {
  const cards = cardsPayload(agentId).cards;
  return {
    ok: true,
    total: 2,
    nodes: cards.map((card) => ({
      atom_id: card.atom_id,
      kind: card.kind,
      summary: card.summary,
    })),
    links: [
      {
        source: `atom-${agentId}-1`,
        target: `atom-${agentId}-2`,
        kind: "supports",
      },
    ],
    truncated: false,
  };
}

function graphNeighborsPayload(agentId: string, atomId: string) {
  return {
    ok: true,
    node: {
      atom_id: atomId,
      kind: "fact",
      summary: `${agentName(agentId)} neighborhood root`,
    },
    neighbors: [
      {
        atom_id: `atom-${agentId}-2`,
        kind: "note",
        summary: `${agentName(agentId)} neighbor`,
        distance: 1,
        via_edge_kind: "supports",
      },
    ],
    links: [
      { source: atomId, target: `atom-${agentId}-2`, kind: "supports" },
    ],
    depth: 1,
    node_limit: 36,
    link_limit: 72,
    requests_used: 2,
    truncated: false,
  };
}

function cardDetailPayload(agentId: string, cardId: string) {
  return {
    ok: true,
    card: {
      card_id: cardId,
      atom_id: `atom-${agentId}-1`,
      kind: "fact",
      status: "active",
      summary: `${agentName(agentId)} dossier ${cardId}`,
    },
    atom: { atom_id: `atom-${agentId}-1` },
    provenance_events: [{ kind: "observed" }],
  };
}

function atomDetailPayload(agentId: string, atomId: string) {
  return {
    ok: true,
    atom: {
      atom_id: atomId,
      kind: "fact",
      status: "active",
      summary: `${agentName(agentId)} atom detail`,
      label: `${agentName(agentId)} atom label`,
    },
  };
}

function telemetryTurnsPayload(agentId: string) {
  return {
    ok: true,
    limit: 12,
    turns: [
      {
        turn_id: `turn-${agentId}-1`,
        route: "chat",
        decision_reason: `${agentName(agentId)} memory hit`,
        latency_ms: 42,
        created_at_utc: "2026-07-02T00:00:00Z",
      },
    ],
  };
}

function turnWhyPayload(agentId: string) {
  return {
    ok: true,
    why: {
      decision: "used_memory",
      decision_reason: `${agentName(agentId)} because`,
      citations: [
        {
          citation_token: `cite-${agentId}-1`,
          label: `${agentName(agentId)} citation`,
        },
      ],
    },
  };
}

function citationPayload(agentId: string, token: string) {
  return {
    ok: true,
    citation: `${agentName(agentId)} citation text ${token}`,
    source_id: `src-${agentId}`,
  };
}

function routingFixture(): RuntimeRoutingConfigResponse {
  return {
    enabled: true,
    use_channel_defaults_as_fallback: false,
    local_operator_human_identity_id: "local-operator",
    dm_unmapped_policy: "approval_required",
    shared_unmapped_policy: "block",
    human_identities: [
      { human_identity_id: "local-operator", display_name: "You", enabled: true },
      { human_identity_id: "colleague", display_name: "Cole", enabled: true },
    ],
    platform_identity_links: [
      {
        provider: "discord",
        platform_user_id: "9001",
        human_identity_id: "local-operator",
        display_name: "You On Discord",
        enabled: true,
      },
    ],
    assistant_assignments: [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "lyra",
        enabled: true,
      },
      {
        human_identity_id: "colleague",
        assistant_agent_id: "root",
        enabled: true,
      },
    ],
    lane_memory_policies: [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "lyra",
        memory_mode: "mno_with_local_sources",
        lane_id: "lane-ly-1",
        local_memory_sources: ["notes/lyra.md"],
      },
    ],
  };
}

function runtimeConfigFixture(): RuntimeConfigResponse {
  return {
    schema_version: "1",
    global: {
      jwt_issuer_allowlist: [],
      jwt_audience_allowlist: [],
      trusted_proxy_allowlist: [],
      tls_termination_mode: "external",
      public_base_url: null,
      assistant_system_prompt: null,
    },
    providers: [],
    channels: {} as RuntimeConfigResponse["channels"],
    routing: routingFixture(),
    memory: {
      blend_mode: "local_augment",
      memory_md_sources: ["docs/memory.md"],
      numquam: {},
    },
    extensions: {},
    security: {},
    autonomy_guardrails: {},
    updated_at: 1_700_000_000,
  };
}

function laneStatusesFixture(agentId: string): AgentMemoryLaneStatusResponse[] {
  if (agentId !== "lyra") {
    return [];
  }
  return [
    {
      human_identity_id: "local-operator",
      assistant_agent_id: "lyra",
      lane_id: "lane-ly-1",
      configured_memory_mode: "mno_with_local_sources",
      effective_memory_mode: "mno_with_local_sources",
      source: "managed_lane",
      status: "available",
      detail: "Lane live",
      local_memory_sources: ["notes/lyra.md"],
      orchestration: null,
    },
  ];
}

interface GatewayFixture {
  statusByAgent: Record<string, AgentMemoryStatusResponse>;
  runtimeConfig: RuntimeConfigResponse;
}

let gateway: GatewayFixture;

function installDefaultMocks() {
  gateway = {
    statusByAgent: {
      lyra: makeStatus("lyra"),
      root: makeStatus("root"),
    },
    runtimeConfig: runtimeConfigFixture(),
  };
  apiMocks.getAgentMemoryStatus.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) => {
      const status = gateway.statusByAgent[agentId];
      if (!status) {
        throw new Error(`404 Not Found: no memory status for ${agentId}`);
      }
      return status;
    }
  );
  apiMocks.getAgentMemoryLaneStatuses.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      laneStatusesFixture(agentId)
  );
  apiMocks.listAgentMemoryCards.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, cardsPayload(agentId))
  );
  apiMocks.getAgentMemoryCard.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string, cardId: string) =>
      wrap(agentId, cardDetailPayload(agentId, cardId))
  );
  apiMocks.getAgentMemoryAtom.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string, atomId: string) =>
      wrap(agentId, atomDetailPayload(agentId, atomId))
  );
  apiMocks.listAgentMemoryEpisodes.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, episodesPayload(agentId))
  );
  apiMocks.getAgentMemoryGraphMap.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, graphMapPayload(agentId))
  );
  apiMocks.getAgentMemoryGraphNeighbors.mockImplementation(
    async (
      _settings: RuntimeConnectionSettings,
      agentId: string,
      query: { atom_id: string }
    ) => wrap(agentId, graphNeighborsPayload(agentId, query.atom_id))
  );
  apiMocks.getAgentMemoryTurnWhy.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, turnWhyPayload(agentId))
  );
  apiMocks.getAgentMemoryCitation.mockImplementation(
    async (
      _settings: RuntimeConnectionSettings,
      agentId: string,
      token: string
    ) => wrap(agentId, citationPayload(agentId, token))
  );
  apiMocks.getAgentMemoryRuntimeHealth.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, { ok: true, status: "ok", checked_at: "2026-07-02T00:00:00Z" })
  );
  apiMocks.getAgentMemoryTelemetrySummary.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, {
        ok: true,
        limit: 12,
        summary: [{ label: `${agentName(agentId)} summary row` }],
      })
  );
  apiMocks.getAgentMemoryTelemetryTurns.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, telemetryTurnsPayload(agentId))
  );
  apiMocks.getAgentMemoryDecisionReasons.mockImplementation(
    async (_settings: RuntimeConnectionSettings, agentId: string) =>
      wrap(agentId, {
        ok: true,
        reasons: [{ label: `${agentName(agentId)} decision reason` }],
      })
  );
  apiMocks.getRuntimeConfig.mockImplementation(async () => ({
    config: gateway.runtimeConfig,
  }));
  apiMocks.updateRuntimeConfig.mockImplementation(
    async (
      _settings: RuntimeConnectionSettings,
      payload: {
        routing?: RuntimeRoutingConfigResponse;
        memory?: RuntimeConfigResponse["memory"];
      }
    ) => {
      const next: RuntimeConfigResponse = {
        ...gateway.runtimeConfig,
        ...(payload.routing ? { routing: payload.routing } : {}),
        ...(payload.memory ? { memory: payload.memory } : {}),
      };
      gateway.runtimeConfig = next;
      return { config: next };
    }
  );
  apiMocks.syncMemorySources.mockImplementation(async () => ({
    items: [],
    synced: 1,
    failed: 0,
  }));
}

let root: Root | null = null;
let container: HTMLDivElement;
let latestController: ReturnType<typeof useMemoryController> | null = null;
let latestPeopleRouting: PeopleRoutingController | null = null;
const setNotice = vi.fn() as unknown as NotifyFn & ReturnType<typeof vi.fn>;
const onOpenAssistant = vi.fn();

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  installDefaultMocks();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  latestController = null;
  latestPeopleRouting = null;
  container.remove();
  localStorage.clear();
});

interface HarnessLanding {
  seq: number;
  kind: "plant" | "agent";
  agentId: string | null;
}

function Harness({
  settings,
  agents,
  enabled = true,
  preferredAgentId = null,
  tokenConfigured = true,
  landing,
}: {
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  enabled?: boolean;
  preferredAgentId?: string | null;
  tokenConfigured?: boolean;
  landing?: HarnessLanding;
}) {
  const peopleRouting = usePeopleRoutingController({
    settings,
    tokenConfigured,
    agents,
  });
  const controller = useMemoryController({
    settings,
    agents,
    enabled,
    preferredAgentId,
    tokenConfigured,
    peopleRouting,
    setNotice,
  });
  useEffect(() => {
    latestController = controller;
  }, [controller]);
  useEffect(() => {
    latestPeopleRouting = peopleRouting;
  }, [peopleRouting]);
  return (
    <MemoryPage
      controller={controller}
      onOpenAssistant={onOpenAssistant}
      landing={landing}
    />
  );
}

async function flush(times = 8) {
  for (let index = 0; index < times; index += 1) {
    await act(async () => {
      await Promise.resolve();
    });
  }
}

async function render(
  settings: RuntimeConnectionSettings = SETTINGS,
  agents: Agent[] = AGENTS,
  enabled = true,
  preferredAgentId: string | null = null,
  tokenConfigured = true,
  landing?: HarnessLanding
) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(
      <Harness
        settings={settings}
        agents={agents}
        enabled={enabled}
        preferredAgentId={preferredAgentId}
        tokenConfigured={tokenConfigured}
        landing={landing}
      />
    );
  });
  await flush();
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function findButton(label: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find(
    (candidate) => candidate.textContent?.includes(label)
  );
  expect(button, `missing button ${label}`).toBeTruthy();
  return button as HTMLButtonElement;
}

async function clickButton(label: string) {
  const button = findButton(label);
  await act(async () => button.click());
  await flush();
}

function findLabeledControl<T extends HTMLElement>(
  labelText: string,
  selector: "select" | "input" | "textarea"
): T {
  const label = Array.from(container.querySelectorAll("label")).find(
    (candidate) => candidate.textContent?.trim().startsWith(labelText)
  );
  const control = label?.querySelector(selector);
  expect(control, `missing ${selector} ${labelText}`).toBeTruthy();
  return control as unknown as T;
}

async function setLabeledControl(
  labelText: string,
  selector: "select" | "input" | "textarea",
  value: string,
  eventName: "change" | "input"
) {
  const control = findLabeledControl<
    HTMLSelectElement | HTMLInputElement | HTMLTextAreaElement
  >(labelText, selector);
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(control),
    "value"
  )?.set;
  setter?.call(control, value);
  await act(async () => {
    control.dispatchEvent(new Event(eventName, { bubbles: true }));
  });
  await flush();
}

const setLabeledSelect = (label: string, value: string) =>
  setLabeledControl(label, "select", value, "change");
const setLabeledInput = (label: string, value: string) =>
  setLabeledControl(label, "input", value, "input");
const setLabeledTextarea = (label: string, value: string) =>
  setLabeledControl(label, "textarea", value, "input");

describe("MemoryPage availability states", () => {
  it("shows the disabled panel and never calls the gateway when the hub is off", async () => {
    await render(SETTINGS, AGENTS, false);

    expect(container.textContent).toContain("Memory is turned off");
    expect(apiMocks.getAgentMemoryStatus).not.toHaveBeenCalled();
    expect(apiMocks.listAgentMemoryCards).not.toHaveBeenCalled();
  });

  it("waits honestly when no agents exist yet", async () => {
    await render(SETTINGS, []);

    expect(container.textContent).toContain("Loading Memory");
    expect(apiMocks.getAgentMemoryStatus).not.toHaveBeenCalled();
  });

  it("reports an unsupported gateway without inventing data", async () => {
    apiMocks.getAgentMemoryStatus.mockRejectedValue(
      new Error("404 Not Found: Cannot GET /api/v1/agents/lyra/memory/status")
    );
    await render();

    expect(container.textContent).toContain("Memory not available");
    expect(container.textContent).toContain(
      "The connected gateway does not expose the Memory surface yet."
    );
    expect(apiMocks.listAgentMemoryCards).not.toHaveBeenCalled();
  });

  it("surfaces a real load failure with its message", async () => {
    apiMocks.getAgentMemoryStatus.mockRejectedValue(
      new Error("500 memory backend exploded")
    );
    await render();

    expect(container.textContent).toContain("Memory failed to load");
    expect(container.textContent).toContain("500 memory backend exploded");
  });
});

function findPostureCard(agentName: string): HTMLElement {
  const card = Array.from(
    container.querySelectorAll(".mc-memory-lane-card")
  ).find((candidate) => candidate.querySelector("strong")?.textContent === agentName);
  expect(card, `missing posture card for ${agentName}`).toBeTruthy();
  return card as HTMLElement;
}

describe("MemoryPage global plant landing", () => {
  it("lands on the global plant with runtime defaults and honest per-assistant posture", async () => {
    await render();

    expect(container.textContent).toContain("Runtime Memory Defaults");
    expect(container.textContent).toContain("Memory + runtime local support");
    expect(container.textContent).toContain("1 runtime local source");
    expect(container.textContent).toContain("runtime memory loaded");
    expect(container.textContent).toContain("Binding Posture");

    // The inspected assistant shows its server-backed status.
    const lyraCard = findPostureCard("Lyra");
    expect(lyraCard.textContent).toContain("memory bound");
    expect(lyraCard.textContent).toContain("checked: available");
    expect(lyraCard.textContent).toContain("1/1 lanes ready");

    // The other assistant's health is never invented from missing facts.
    const rootCard = findPostureCard("Root");
    expect(rootCard.textContent).toContain("no memory binding");
    expect(rootCard.textContent).toContain("not checked in this sitting");
    expect(rootCard.textContent).not.toContain("checked: available");
    // Global routing posture: the colleague routes to Root even though
    // Root has never been inspected in this sitting.
    expect(rootCard.textContent).toContain("1 routed person");
  });

  it("opens the exact assistant from the plant posture board", async () => {
    await render();

    const rootCard = findPostureCard("Root");
    const inspect = Array.from(rootCard.querySelectorAll("button")).find(
      (button) => button.textContent?.includes("Inspect memory")
    );
    expect(inspect).toBeTruthy();
    await act(async () => inspect!.click());
    await flush();

    const select = findLabeledControl<HTMLSelectElement>("Assistant", "select");
    expect(select.value).toBe("root");
    expect(container.textContent).toContain("Root fact one");
    expect(container.textContent).not.toContain("Lyra fact one");
  });

  it("keeps the lane map with its policy editor under Routing", async () => {
    await render();
    await clickButton("Routing");

    expect(container.textContent).toContain("Lane Map");
    expect(container.textContent).toContain("routing on");
    expect(container.textContent).toContain("You");
    expect(container.textContent).toContain("local-operator");
    expect(container.textContent).toContain("Memory + local");
    expect(container.textContent).toContain("Memory ready");
    expect(container.textContent).toContain("lane runtime");
    expect(container.textContent).toContain("Lane live");
    expect(container.textContent).toContain("lane-ly-1");
    // The other human routes to a different assistant and stays off this lane map.
    expect(container.textContent).not.toContain("Cole");
  });
});

describe("MemoryPage landing intent seam", () => {
  it("relands on the plant when the newest room intent is the plant", async () => {
    await render();
    await clickButton("Library");
    expect(container.textContent).toContain("Lyra fact one");

    await render(SETTINGS, AGENTS, true, null, true, {
      seq: 1,
      kind: "plant",
      agentId: null,
    });

    expect(container.textContent).toContain("Binding Posture");
    expect(container.textContent).not.toContain("Lyra fact one");
  });

  it("lands on the exact requested agent from a Staff-card intent", async () => {
    await render();
    // The intent source (the Staff-card handler) selects the exact agent in
    // its event context, then hands the landing intent to the page.
    await act(async () => {
      latestController!.setSelectedAgentId("root");
    });
    await render(SETTINGS, AGENTS, true, null, true, {
      seq: 1,
      kind: "agent",
      agentId: "root",
    });

    const select = findLabeledControl<HTMLSelectElement>("Assistant", "select");
    expect(select.value).toBe("root");
    expect(container.textContent).toContain("Root fact one");
    expect(container.textContent).not.toContain("Lyra fact one");
  });

  it("keeps internal section choices across unrelated rerenders", async () => {
    await render(SETTINGS, AGENTS, true, null, true, {
      seq: 1,
      kind: "plant",
      agentId: null,
    });
    await clickButton("Episodes");
    expect(container.textContent).toContain("Lyra episode");

    await render(SETTINGS, AGENTS, true, null, true, {
      seq: 1,
      kind: "plant",
      agentId: null,
    });

    expect(container.textContent).toContain("Lyra episode");
    expect(container.textContent).not.toContain("Binding Posture");
  });

  it("does not reset the drill-in when the intent names the already-selected agent", async () => {
    await render();
    await clickButton("Library");
    expect(container.textContent).toContain("Lyra fact one");
    const statusCalls = apiMocks.getAgentMemoryStatus.mock.calls.length;

    await render(SETTINGS, AGENTS, true, null, true, {
      seq: 1,
      kind: "agent",
      agentId: "lyra",
    });

    expect(container.textContent).toContain("Lyra fact one");
    expect(apiMocks.getAgentMemoryStatus.mock.calls.length).toBe(statusCalls);
  });
});

describe("MemoryPage seven-section parity", () => {

  it("selects the first memory-bound agent and shows its Library", async () => {
    await render();
    await clickButton("Library");

    const select = findLabeledControl<HTMLSelectElement>("Assistant", "select");
    expect(select.value).toBe("lyra");
    expect(container.textContent).toContain("Lyra fact one");
    expect(container.textContent).toContain("Lyra fact two");
    expect(container.textContent).toContain("binding-lyra");
    expect(container.textContent).toContain("orchestration: ok");
    expect(container.textContent).toContain("health aligned");
  });

  it("shows Episodes for the selected agent", async () => {
    await render();
    await clickButton("Episodes");

    expect(container.textContent).toContain("Lyra episode");
    expect(container.textContent).toContain("run-lyra-1");
    expect(container.textContent).toContain("2026-07-01T00:00:00Z");
  });

  it("shows the Graph overview and auto-loaded neighborhood", async () => {
    await render();
    await clickButton("Graph");

    expect(container.textContent).toContain("nodes: 2");
    expect(container.textContent).toContain("links: 1");
    expect(container.textContent).toContain("complete");
    expect(container.textContent).toContain("Lyra neighborhood root");
    expect(container.textContent).toContain("Lyra neighbor");
    expect(container.textContent).toContain("depth 1");
    expect(container.textContent).toContain("requests 2");
    expect(container.textContent).toContain(
      "supports: atom-lyra-1 → atom-lyra-2"
    );
  });

  it("shows Details for the auto-selected first card", async () => {
    await render();
    await clickButton("Details");

    expect(container.textContent).toContain("Card dossier");
    expect(container.textContent).toContain("Lyra dossier card-lyra-1");
    expect(container.textContent).toContain("Atom detail");
    expect(container.textContent).toContain("Lyra atom detail");
    expect(container.textContent).toContain("Provenance");
    expect(container.textContent).toContain("observed");
  });

  it("walks Reasoning from turn to why to citation", async () => {
    await render();
    await clickButton("Reasoning");

    expect(container.textContent).toContain("Recent turns");
    expect(container.textContent).toContain("Turn 1");
    expect(container.textContent).toContain("Lyra memory hit");
    expect(container.textContent).toContain("Decision why");
    expect(container.textContent).toContain("used_memory");
    expect(container.textContent).toContain("Lyra because");
    expect(container.textContent).toContain("Citation drilldown");
    expect(container.textContent).toContain("Lyra citation");
    expect(container.textContent).toContain("Lyra citation text cite-lyra-1");
  });

  it("shows Health, telemetry summary, and decision reasons", async () => {
    await render();
    await clickButton("Health");

    expect(container.textContent).toContain("Health + Diagnostics");
    expect(container.textContent).toContain("ok");
    expect(container.textContent).toContain("2026-07-02T00:00:00Z");
    expect(container.textContent).toContain("Lyra summary row");
    expect(container.textContent).toContain("Lyra decision reason");
  });
});

describe("MemoryPage request identity and filters", () => {
  it("issues every core read with its exact identity and limits", async () => {
    await render();

    expect(apiMocks.getAgentMemoryStatus).toHaveBeenCalledWith(SETTINGS, "lyra");
    expect(apiMocks.listAgentMemoryCards).toHaveBeenCalledWith(SETTINGS, "lyra", {
      status: "all",
      q: undefined,
      limit: 60,
    });
    expect(apiMocks.listAgentMemoryEpisodes).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      { status: "all", q: undefined }
    );
    expect(apiMocks.getAgentMemoryGraphMap).toHaveBeenCalledWith(SETTINGS, "lyra", {
      status: "all",
      q: undefined,
      limit: 60,
    });
    expect(apiMocks.getAgentMemoryRuntimeHealth).toHaveBeenCalledWith(
      SETTINGS,
      "lyra"
    );
    expect(apiMocks.getAgentMemoryTelemetrySummary).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      { limit: 12 }
    );
    expect(apiMocks.getAgentMemoryTelemetryTurns).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      { limit: 12 }
    );
    expect(apiMocks.getAgentMemoryDecisionReasons).toHaveBeenCalledWith(
      SETTINGS,
      "lyra"
    );
    expect(apiMocks.getAgentMemoryLaneStatuses).toHaveBeenCalledWith(
      SETTINGS,
      "lyra"
    );
    expect(apiMocks.getAgentMemoryCard).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      "card-lyra-1"
    );
    expect(apiMocks.getAgentMemoryAtom).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      "atom-lyra-1"
    );
    expect(apiMocks.getAgentMemoryGraphNeighbors).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      {
        atom_id: "atom-lyra-1",
        depth: 1,
        node_limit: 36,
        link_limit: 72,
        include_root_detail: true,
        include_shared_language: true,
      }
    );
    expect(apiMocks.getAgentMemoryTurnWhy).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      "turn-lyra-1",
      { citations: true }
    );
    expect(apiMocks.getAgentMemoryCitation).toHaveBeenCalledWith(
      SETTINGS,
      "lyra",
      "cite-lyra-1"
    );
  });

  it("threads card search and status filters into the exact reload", async () => {
    await render();
    await clickButton("Library");

    await setLabeledInput("Search", "alpha");
    expect(apiMocks.listAgentMemoryCards).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra",
      { status: "all", q: "alpha", limit: 60 }
    );
    expect(apiMocks.getAgentMemoryGraphMap).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra",
      { status: "all", q: "alpha", limit: 60 }
    );

    await setLabeledSelect("Status", "active");
    expect(apiMocks.listAgentMemoryCards).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra",
      { status: "active", q: "alpha", limit: 60 }
    );
  });

  it("threads the episode search into the exact episodes reload", async () => {
    await render();
    await clickButton("Episodes");

    await setLabeledInput("Search", "shipping");
    expect(apiMocks.listAgentMemoryEpisodes).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra",
      { status: "all", q: "shipping" }
    );
  });

  it("fetches the exact card the operator picks", async () => {
    await render();
    await clickButton("Library");
    await clickButton("Lyra fact two");

    expect(apiMocks.getAgentMemoryCard).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra",
      "card-lyra-2"
    );
    expect(apiMocks.getAgentMemoryAtom).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra",
      "atom-lyra-2"
    );
    expect(apiMocks.getAgentMemoryGraphNeighbors).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra",
      expect.objectContaining({ atom_id: "atom-lyra-2" })
    );
  });
});

describe("MemoryPage surface gating and honest partial states", () => {
  it("locks each native surface that the gateway does not offer", async () => {
    const status = makeStatus("lyra");
    status.native_surface_availability = {
      ...status.native_surface_availability,
      cards: false,
      episodes: false,
      graph_overview: false,
      graph_neighbors: false,
      turn_why: false,
    };
    gateway.statusByAgent.lyra = status;
    await render();

    expect(apiMocks.listAgentMemoryCards).not.toHaveBeenCalled();
    expect(apiMocks.listAgentMemoryEpisodes).not.toHaveBeenCalled();
    expect(apiMocks.getAgentMemoryGraphMap).not.toHaveBeenCalled();
    expect(apiMocks.getAgentMemoryGraphNeighbors).not.toHaveBeenCalled();
    expect(apiMocks.getAgentMemoryTurnWhy).not.toHaveBeenCalled();
    // Health stays live even while list surfaces are missing.
    expect(apiMocks.getAgentMemoryRuntimeHealth).toHaveBeenCalled();

    await clickButton("Library");
    expect(container.textContent).toContain(
      "Memory cards are not available for this agent."
    );
    await clickButton("Episodes");
    expect(container.textContent).toContain(
      "Episodes are not available for this agent."
    );
    await clickButton("Graph");
    expect(container.textContent).toContain(
      "Knowledge graph is not available for this agent."
    );
    expect(container.textContent).toContain(
      "Related concepts view is not available for this agent."
    );
    await clickButton("Reasoning");
    expect(container.textContent).toContain(
      "Reasoning and source inspection is not available for this agent."
    );
  });

  it("never reads memory surfaces for an unconfigured binding and says so", async () => {
    const status = makeStatus("lyra");
    status.binding_status = "unconfigured";
    status.binding = null;
    gateway.statusByAgent.lyra = status;
    await render();

    expect(apiMocks.listAgentMemoryCards).not.toHaveBeenCalled();
    expect(apiMocks.getAgentMemoryRuntimeHealth).not.toHaveBeenCalled();
    await clickButton("Library");
    expect(container.textContent).toContain(
      "This agent doesn't have memory set up yet."
    );
  });

  it("tells the truth about an unauthorized binding", async () => {
    const status = makeStatus("lyra");
    status.binding_status = "unauthorized";
    gateway.statusByAgent.lyra = status;
    await render();

    expect(apiMocks.listAgentMemoryCards).not.toHaveBeenCalled();
    await clickButton("Library");
    expect(container.textContent).toContain(
      "authentication is missing or expired"
    );
  });

  it("keeps reading under a degraded binding while warning honestly", async () => {
    const status = makeStatus("lyra");
    status.binding_status = "degraded";
    gateway.statusByAgent.lyra = status;
    await render();

    expect(apiMocks.listAgentMemoryCards).toHaveBeenCalled();
    await clickButton("Library");
    expect(container.textContent).toContain("memory connection is degraded");
    expect(container.textContent).toContain("Lyra fact one");
  });

  it("scopes a card detail failure without poisoning the other sections", async () => {
    apiMocks.getAgentMemoryCard.mockRejectedValue(
      new Error("card lookup timed out")
    );
    await render();
    await clickButton("Details");

    expect(container.textContent).toContain("card lookup timed out");
    await clickButton("Library");
    expect(container.textContent).toContain("Lyra fact one");
    await clickButton("Health");
    expect(container.textContent).toContain("Lyra summary row");
  });

  it("scopes a graph neighbors failure to the Related Concepts panel", async () => {
    apiMocks.getAgentMemoryGraphNeighbors.mockRejectedValue(
      new Error("neighborhood exploded")
    );
    await render();
    await clickButton("Graph");

    expect(container.textContent).toContain("neighborhood exploded");
    expect(container.textContent).toContain("Lyra fact one");
  });

  it("shows the lane status error inside the lane map without killing lanes", async () => {
    apiMocks.getAgentMemoryLaneStatuses.mockRejectedValue(
      new Error("lane status broke")
    );
    await render();
    await clickButton("Routing");

    expect(container.textContent).toContain("Lane memory status could not load.");
    expect(container.textContent).toContain("lane status broke");
    expect(container.textContent).toContain("You");
  });

  it("shows the routing snapshot error when lane routing cannot load", async () => {
    apiMocks.getRuntimeConfig.mockRejectedValue(new Error("config gone"));
    await render();
    await clickButton("Routing");

    expect(container.textContent).toContain("Lane routing could not load.");
    expect(container.textContent).toContain("config gone");
  });
});

describe("MemoryPage agent switching, stale responses, and cache isolation", () => {
  it("switches agents and shows only the new agent's memory", async () => {
    await render();
    await clickButton("Library");
    await setLabeledSelect("Assistant", "root");

    expect(apiMocks.getAgentMemoryStatus).toHaveBeenLastCalledWith(
      SETTINGS,
      "root"
    );
    expect(container.textContent).toContain("Root fact one");
    expect(container.textContent).not.toContain("Lyra fact one");
    expect(container.textContent).toContain("binding-root");
  });

  it("discards a slow response from a previously selected agent", async () => {
    const lyraCards = deferred<ReturnType<typeof wrap>>();
    apiMocks.listAgentMemoryCards.mockImplementation(
      async (_settings: RuntimeConnectionSettings, agentId: string) => {
        if (agentId === "lyra") {
          return lyraCards.promise;
        }
        return wrap(agentId, cardsPayload(agentId));
      }
    );
    await render();
    await clickButton("Library");
    expect(container.textContent).not.toContain("Lyra fact one");

    await setLabeledSelect("Assistant", "root");
    expect(container.textContent).toContain("Root fact one");

    lyraCards.resolve(wrap("lyra", cardsPayload("lyra")));
    await flush();

    expect(container.textContent).toContain("Root fact one");
    expect(container.textContent).not.toContain("Lyra fact one");
  });

  it("reuses the per-binding cache when returning to an agent", async () => {
    await render();
    await clickButton("Library");
    await setLabeledSelect("Assistant", "root");
    expect(container.textContent).toContain("Root fact one");

    // Returning to Lyra: status resolves but the fresh cards read hangs.
    const hangingCards = deferred<never>();
    apiMocks.listAgentMemoryCards.mockImplementation(
      async (_settings: RuntimeConnectionSettings, agentId: string) => {
        if (agentId === "lyra") {
          return hangingCards.promise;
        }
        return wrap(agentId, cardsPayload(agentId));
      }
    );
    await setLabeledSelect("Assistant", "lyra");

    expect(container.textContent).toContain("Lyra fact one");
    expect(container.textContent).not.toContain("Root fact one");
  });

  it("never fires cross-agent detail requests when the derived selection flips", async () => {
    await render();
    await clickButton("Library");
    expect(container.textContent).toContain("Lyra fact one");

    // A late preferred-agent arrival flips the DERIVED selection without an
    // explicit click. No detail request may pair the new agent with the old
    // agent's card, atom, graph, turn, or citation identity.
    await render(SETTINGS, AGENTS, true, "root");

    for (const call of apiMocks.getAgentMemoryCard.mock.calls) {
      expect(call[2]).toContain(String(call[1]));
    }
    for (const call of apiMocks.getAgentMemoryAtom.mock.calls) {
      expect(call[2]).toContain(String(call[1]));
    }
    for (const call of apiMocks.getAgentMemoryGraphNeighbors.mock.calls) {
      expect(
        (call[2] as { atom_id: string }).atom_id
      ).toContain(String(call[1]));
    }
    for (const call of apiMocks.getAgentMemoryTurnWhy.mock.calls) {
      expect(call[2]).toContain(String(call[1]));
    }
    for (const call of apiMocks.getAgentMemoryCitation.mock.calls) {
      expect(call[2]).toContain(String(call[1]));
    }
    expect(container.textContent).toContain("Root fact one");
    expect(container.textContent).not.toContain("Lyra fact one");
  });

  it("hides old posture and lane facts immediately while a derived new-agent load is delayed", async () => {
    await render();
    await clickButton("Routing");
    expect(latestController!.status?.agent_id).toBe("lyra");
    expect(container.textContent).toContain("Lane live");

    const rootStatus = deferred<AgentMemoryStatusResponse>();
    const rootLanes = deferred<AgentMemoryLaneStatusResponse[]>();
    apiMocks.getAgentMemoryStatus.mockImplementation(
      async (_settings: RuntimeConnectionSettings, agentId: string) =>
        agentId === "root" ? rootStatus.promise : makeStatus(agentId)
    );
    apiMocks.getAgentMemoryLaneStatuses.mockImplementation(
      async (_settings: RuntimeConnectionSettings, agentId: string) =>
        agentId === "root" ? rootLanes.promise : laneStatusesFixture(agentId)
    );

    // preferredAgentId changes selection without the controller setter.
    await render(SETTINGS, AGENTS, true, "root");

    expect(latestController!.selectedAgentId).toBe("root");
    expect(latestController!.status).toBeNull();
    expect(latestController!.laneStatuses).toEqual([]);
    expect(latestController!.runtimeHealthResponse).toBeNull();
    expect(latestController!.graphMapResponse).toBeNull();
    expect(latestController!.telemetrySummaryResponse).toBeNull();
    expect(latestController!.telemetryTurnsResponse).toBeNull();
    expect(latestController!.decisionReasonsResponse).toBeNull();
    expect(container.textContent).not.toContain("Lane live");
    expect(apiMocks.getAgentMemoryStatus).toHaveBeenLastCalledWith(SETTINGS, "root");
    expect(apiMocks.getAgentMemoryLaneStatuses).toHaveBeenLastCalledWith(SETTINGS, "root");

    const rootLane = laneStatusesFixture("lyra").map((lane) => ({
      ...lane,
      assistant_agent_id: "root",
      detail: "Root lane live",
    }));
    await act(async () => {
      rootStatus.resolve(makeStatus("root"));
      rootLanes.resolve(rootLane);
      await Promise.all([rootStatus.promise, rootLanes.promise]);
    });
    await flush();

    expect(latestController!.status?.agent_id).toBe("root");
    expect(latestController!.laneStatuses).toEqual(rootLane);
    expect(container.textContent).not.toContain("Lane live");
  });

  it("ignores a stale card detail once the operator picks another card", async () => {
    const slowDetail = deferred<ReturnType<typeof wrap>>();
    apiMocks.getAgentMemoryCard.mockImplementation(
      async (
        _settings: RuntimeConnectionSettings,
        agentId: string,
        cardId: string
      ) => {
        if (cardId === "card-lyra-1") {
          return slowDetail.promise;
        }
        return wrap(agentId, cardDetailPayload(agentId, cardId));
      }
    );
    await render();
    await clickButton("Library");
    await clickButton("Lyra fact two");
    await clickButton("Details");
    expect(container.textContent).toContain("Lyra dossier card-lyra-2");

    slowDetail.resolve(wrap("lyra", cardDetailPayload("lyra", "card-lyra-1")));
    await flush();
    expect(container.textContent).toContain("Lyra dossier card-lyra-2");
    expect(container.textContent).not.toContain("Lyra dossier card-lyra-1");
  });
});

describe("MemoryPage runtime defaults, lane policy, and sync mutations", () => {
  it("saves runtime memory defaults with the exact config body and clears drafts only after success", async () => {
    await render();

    await setLabeledSelect("Default memory mode", "mno_primary");
    await setLabeledTextarea(
      "Runtime local files",
      "docs/memory.md\ndocs/extra.md"
    );
    await clickButton("Save runtime defaults");

    expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledWith(SETTINGS, {
      memory: {
        blend_mode: "mno_primary",
        memory_md_sources: ["docs/memory.md", "docs/extra.md"],
        numquam: {},
      },
    });
    expect(setNotice).toHaveBeenCalledWith({
      tone: "info",
      message: "Runtime memory defaults saved.",
    });
    expect(container.textContent).toContain("2 runtime local sources");
    expect(container.textContent).toContain("Memory first");
    // Lane statuses refresh after the authoritative save.
    expect(apiMocks.getAgentMemoryLaneStatuses).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra"
    );
  });

  it("keeps the runtime defaults draft when the save fails", async () => {
    apiMocks.updateRuntimeConfig.mockRejectedValue(new Error("write refused"));
    await render();

    await setLabeledSelect("Default memory mode", "mno_primary");
    await clickButton("Save runtime defaults");

    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: expect.stringContaining("Saving runtime memory defaults failed:"),
    });
    const modeSelect = findLabeledControl<HTMLSelectElement>(
      "Default memory mode",
      "select"
    );
    expect(modeSelect.value).toBe("mno_primary");
    expect(findButton("Save runtime defaults").disabled).toBe(false);
  });

  it("saves a lane policy into the full routing object without dropping other routing facts", async () => {
    await render();
    await clickButton("Routing");

    await setLabeledSelect("Memory mode", "local_only");
    await setLabeledTextarea("Lane local files", "notes/lyra.md\nnotes/new.md");
    await clickButton("Save lane settings");

    const expectedRouting = routingFixture();
    expectedRouting.lane_memory_policies = [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "lyra",
        memory_mode: "local_only",
        lane_id: "lane-ly-1",
        local_memory_sources: ["notes/lyra.md", "notes/new.md"],
      },
    ];
    expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledWith(SETTINGS, {
      routing: expectedRouting,
    });
    expect(setNotice).toHaveBeenCalledWith({
      tone: "info",
      message: "Lane memory settings saved.",
    });
    expect(apiMocks.getAgentMemoryLaneStatuses).toHaveBeenLastCalledWith(
      SETTINGS,
      "lyra"
    );
  });

  it("keeps the lane draft when the lane policy save fails", async () => {
    apiMocks.updateRuntimeConfig.mockRejectedValue(new Error("routing locked"));
    await render();
    await clickButton("Routing");

    await setLabeledSelect("Memory mode", "local_only");
    await clickButton("Save lane settings");

    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: expect.stringContaining("Saving lane memory mode failed:"),
    });
    const modeSelect = findLabeledControl<HTMLSelectElement>(
      "Memory mode",
      "select"
    );
    expect(modeSelect.value).toBe("local_only");
    expect(findButton("Save lane settings").disabled).toBe(false);
  });

  it("syncs runtime memory files with an empty scope body and reports counts", async () => {
    apiMocks.syncMemorySources.mockResolvedValue({
      items: [],
      synced: 2,
      failed: 0,
    });
    await render();

    await clickButton("Sync runtime files now");

    expect(apiMocks.syncMemorySources).toHaveBeenCalledWith(SETTINGS, {});
    expect(setNotice).toHaveBeenCalledWith({
      tone: "info",
      message: "Runtime memory sync complete: 2 files synced.",
    });
  });

  it("reports a partial runtime sync as a failure", async () => {
    apiMocks.syncMemorySources.mockResolvedValue({
      items: [],
      synced: 1,
      failed: 2,
    });
    await render();

    await clickButton("Sync runtime files now");

    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: "Runtime memory sync finished with issues: 1 synced, 2 failed.",
    });
  });

  it("syncs lane memory files with the exact lane scope body", async () => {
    await render();
    await clickButton("Routing");

    await clickButton("Sync lane files");

    expect(apiMocks.syncMemorySources).toHaveBeenCalledWith(SETTINGS, {
      human_identity_id: "local-operator",
      assistant_agent_id: "lyra",
    });
    expect(setNotice).toHaveBeenCalledWith({
      tone: "info",
      message: "Lane memory sync complete: 1 file synced.",
    });
  });

  it("refuses every mutation without a configured gateway", async () => {
    await render({ gateway_url: "" });

    expect(latestController).toBeTruthy();
    let saved = true;
    await act(async () => {
      saved = await latestController!.saveRuntimeMemoryDefaults("mno_primary", []);
    });
    expect(saved).toBe(false);
    let laneSaved = true;
    await act(async () => {
      laneSaved = await latestController!.saveLaneMemoryPolicy(
        "local-operator",
        "lyra",
        "local_only"
      );
    });
    expect(laneSaved).toBe(false);
    let synced = true;
    await act(async () => {
      synced = await latestController!.syncRuntimeMemoryDefaults();
    });
    expect(synced).toBe(false);
    let laneSynced = true;
    await act(async () => {
      laneSynced = await latestController!.syncLaneMemorySources(
        "local-operator",
        "lyra"
      );
    });
    expect(laneSynced).toBe(false);

    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
    expect(apiMocks.syncMemorySources).not.toHaveBeenCalled();
    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: "Connect to the gateway before saving runtime memory defaults.",
    });
    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: "Connect to the gateway before saving lane memory policy.",
    });
    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: "Connect to the gateway before syncing runtime memory files.",
    });
    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: "Connect to the gateway before syncing lane memory files.",
    });
  });
});

describe("MemoryPage configured-auth gating", () => {
  it("waits for configured authentication before reading anything", async () => {
    await render(SETTINGS, AGENTS, true, null, false);

    expect(apiMocks.getAgentMemoryStatus).not.toHaveBeenCalled();
    expect(apiMocks.getRuntimeConfig).not.toHaveBeenCalled();
    expect(apiMocks.getAgentMemoryLaneStatuses).not.toHaveBeenCalled();
    expect(container.textContent).toContain("Waiting for the gateway connection.");
  });

  it("starts reading only when authentication becomes configured", async () => {
    await render(SETTINGS, AGENTS, true, null, false);
    expect(apiMocks.getAgentMemoryStatus).not.toHaveBeenCalled();

    await render(SETTINGS, AGENTS, true, null, true);

    expect(apiMocks.getAgentMemoryStatus).toHaveBeenCalledWith(SETTINGS, "lyra");
    await clickButton("Library");
    expect(container.textContent).toContain("Lyra fact one");
  });

  it("discards an in-flight read when authentication is torn down", async () => {
    const slowStatus = deferred<AgentMemoryStatusResponse>();
    apiMocks.getAgentMemoryStatus.mockImplementation(async () => slowStatus.promise);
    await render(SETTINGS, AGENTS, true, null, true);

    await render(SETTINGS, AGENTS, true, null, false);
    slowStatus.resolve(makeStatus("lyra"));
    await flush();

    expect(container.textContent).toContain("Waiting for the gateway connection.");
    expect(apiMocks.listAgentMemoryCards).not.toHaveBeenCalled();
  });

  it("refuses every mutation without configured authentication", async () => {
    await render(SETTINGS, AGENTS, true, null, false);

    expect(latestController).toBeTruthy();
    let saved = true;
    await act(async () => {
      saved = await latestController!.saveRuntimeMemoryDefaults("mno_primary", []);
    });
    expect(saved).toBe(false);
    let laneSaved = true;
    await act(async () => {
      laneSaved = await latestController!.saveLaneMemoryPolicy(
        "local-operator",
        "lyra",
        "local_only"
      );
    });
    expect(laneSaved).toBe(false);
    let synced = true;
    await act(async () => {
      synced = await latestController!.syncRuntimeMemoryDefaults();
    });
    expect(synced).toBe(false);
    let laneSynced = true;
    await act(async () => {
      laneSynced = await latestController!.syncLaneMemorySources(
        "local-operator",
        "lyra"
      );
    });
    expect(laneSynced).toBe(false);

    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
    expect(apiMocks.syncMemorySources).not.toHaveBeenCalled();
  });
});

describe("MemoryPage same-tick mutation locks", () => {
  it("lets one synchronous lock own a same-tick double runtime-defaults save", async () => {
    await render();

    let results: boolean[] = [];
    await act(async () => {
      results = await Promise.all([
        latestController!.saveRuntimeMemoryDefaults("mno_primary", ["a.md"]),
        latestController!.saveRuntimeMemoryDefaults("mno_primary", ["a.md"]),
      ]);
    });
    await flush();

    expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledTimes(1);
    expect(results.filter(Boolean)).toHaveLength(1);
  });

  it("lets one synchronous lock own a same-tick double lane-policy save", async () => {
    await render();
    apiMocks.updateRuntimeConfig.mockClear();

    let results: boolean[] = [];
    await act(async () => {
      results = await Promise.all([
        latestController!.saveLaneMemoryPolicy("local-operator", "lyra", "local_only"),
        latestController!.saveLaneMemoryPolicy("local-operator", "lyra", "local_only"),
      ]);
    });
    await flush();

    expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledTimes(1);
    expect(results.filter(Boolean)).toHaveLength(1);
  });

  it("lets one synchronous lock own a same-tick double runtime sync", async () => {
    await render();

    await act(async () => {
      await Promise.all([
        latestController!.syncRuntimeMemoryDefaults(),
        latestController!.syncRuntimeMemoryDefaults(),
      ]);
    });
    await flush();

    expect(apiMocks.syncMemorySources).toHaveBeenCalledTimes(1);
  });

  it("lets one synchronous lock own a same-tick double lane sync", async () => {
    await render();

    await act(async () => {
      await Promise.all([
        latestController!.syncLaneMemorySources("local-operator", "lyra"),
        latestController!.syncLaneMemorySources("local-operator", "lyra"),
      ]);
    });
    await flush();

    expect(apiMocks.syncMemorySources).toHaveBeenCalledTimes(1);
  });
});

describe("MemoryPage shared People & Routing authority", () => {
  it("lands the lane policy save inside the shared routing authority state", async () => {
    await render();
    await clickButton("Routing");

    await setLabeledSelect("Memory mode", "local_only");
    await clickButton("Save lane settings");

    // The one shared People & Routing controller now holds the saved policy,
    // proving the write flowed through it instead of a second writer.
    const sharedPolicy = latestPeopleRouting?.routingConfig?.lane_memory_policies.find(
      (policy) =>
        policy.human_identity_id === "local-operator" &&
        policy.assistant_agent_id === "lyra"
    );
    expect(sharedPolicy?.memory_mode).toBe("local_only");
  });

  it("reloads posture and lane facts when the shared routing authority saves", async () => {
    await render();
    const statusCalls = apiMocks.getAgentMemoryStatus.mock.calls.length;
    const laneCalls = apiMocks.getAgentMemoryLaneStatuses.mock.calls.length;

    // A Front Desk save lands in the shared authority; Memory's posture must
    // pick up the new routing truth without a manual refresh click.
    await act(async () => {
      await latestPeopleRouting!.restoreRoutingConfig(routingFixture());
    });
    await flush();

    expect(apiMocks.getAgentMemoryStatus.mock.calls.length).toBeGreaterThan(
      statusCalls
    );
    expect(apiMocks.getAgentMemoryLaneStatuses.mock.calls.length).toBeGreaterThan(
      laneCalls
    );
  });

  it("refuses a lane policy save while Front Desk holds unsaved routing edits", async () => {
    await render();
    apiMocks.updateRuntimeConfig.mockClear();

    await act(async () => {
      latestPeopleRouting!.patchRoutingDraft((draft) => {
        draft.human_identities[0]!.display_name = "Edited At The Front Desk";
      });
    });
    await flush();

    let saved = true;
    await act(async () => {
      saved = await latestController!.saveLaneMemoryPolicy(
        "local-operator",
        "lyra",
        "local_only"
      );
    });
    expect(saved).toBe(false);
    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: expect.stringContaining("unsaved"),
    });
  });

  it("preserves the exact Front Desk draft when it changes during the lane-policy read", async () => {
    await render();
    apiMocks.getRuntimeConfig.mockClear();
    apiMocks.updateRuntimeConfig.mockClear();
    const pendingRead = deferred<{ config: { routing: RuntimeRoutingConfigResponse } }>();
    apiMocks.getRuntimeConfig.mockReturnValueOnce(pendingRead.promise);

    let saved: Promise<boolean> | undefined;
    await act(async () => {
      saved = latestController!.saveLaneMemoryPolicy(
        "local-operator",
        "lyra",
        "local_only"
      );
      await Promise.resolve();
    });
    expect(apiMocks.getRuntimeConfig).toHaveBeenCalledTimes(1);

    await act(async () => {
      latestPeopleRouting!.patchRoutingDraft((draft) => {
        draft.human_identities[0]!.display_name = "Exact Front Desk edit";
      });
    });
    const draftAfterEdit = JSON.stringify(latestPeopleRouting!.routingDraft);

    await act(async () => {
      pendingRead.resolve({ config: { routing: routingFixture() } });
      await pendingRead.promise;
      expect(await saved).toBe(false);
    });

    expect(apiMocks.updateRuntimeConfig).not.toHaveBeenCalled();
    expect(JSON.stringify(latestPeopleRouting!.routingDraft)).toBe(draftAfterEdit);
    expect(latestPeopleRouting!.routingDraft?.human_identities[0]?.display_name).toBe(
      "Exact Front Desk edit"
    );
  });

  it("preserves the exact Front Desk draft when it changes after the lane-policy read", async () => {
    await render();
    apiMocks.getRuntimeConfig.mockClear();
    apiMocks.updateRuntimeConfig.mockClear();
    const pendingRead = deferred<{ config: { routing: RuntimeRoutingConfigResponse } }>();
    const pendingWrite = deferred<{ config: { routing: RuntimeRoutingConfigResponse } }>();
    apiMocks.getRuntimeConfig.mockReturnValueOnce(pendingRead.promise);
    apiMocks.updateRuntimeConfig.mockReturnValueOnce(pendingWrite.promise);

    let saved: Promise<boolean> | undefined;
    await act(async () => {
      saved = latestController!.saveLaneMemoryPolicy(
        "local-operator",
        "lyra",
        "local_only"
      );
      await Promise.resolve();
    });
    await act(async () => {
      pendingRead.resolve({ config: { routing: routingFixture() } });
      await pendingRead.promise;
      await Promise.resolve();
    });
    expect(apiMocks.updateRuntimeConfig).toHaveBeenCalledTimes(1);

    await act(async () => {
      latestPeopleRouting!.patchRoutingDraft((draft) => {
        draft.human_identities[0]!.display_name = "Later exact Front Desk edit";
      });
    });
    const draftAfterEdit = JSON.stringify(latestPeopleRouting!.routingDraft);
    const savedRouting = routingFixture();
    savedRouting.lane_memory_policies = [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "lyra",
        memory_mode: "local_only",
        lane_id: "lane-ly-1",
        local_memory_sources: ["notes/lyra.md"],
      },
    ];

    await act(async () => {
      pendingWrite.resolve({ config: { routing: savedRouting } });
      await pendingWrite.promise;
      expect(await saved).toBe(true);
    });

    expect(JSON.stringify(latestPeopleRouting!.routingDraft)).toBe(draftAfterEdit);
    expect(latestPeopleRouting!.routingDraft?.human_identities[0]?.display_name).toBe(
      "Later exact Front Desk edit"
    );
    expect(latestPeopleRouting!.routingConfig?.lane_memory_policies).toEqual(
      savedRouting.lane_memory_policies
    );
    expect(latestPeopleRouting!.routingNotice).toEqual({
      tone: "info",
      message:
        "Lane memory policy saved. Directory / Front Desk changed while saving, so its newer draft was preserved.",
    });
  });
});

describe("MemoryPage Office pin", () => {
  it("pins the memory room from the ready plant with zero memory/runtime/routing mutations", async () => {
    await render();
    const callsBefore = {
      update: apiMocks.updateRuntimeConfig.mock.calls.length,
      sync: apiMocks.syncMemorySources.mock.calls.length,
      status: apiMocks.getAgentMemoryStatus.mock.calls.length,
      config: apiMocks.getRuntimeConfig.mock.calls.length,
    };

    await clickButton("Pin to Office");

    expect(container.textContent).toContain("On the Office canvas.");
    const layout = JSON.parse(
      localStorage.getItem("mc-glass-config-v1") ?? "{}"
    ).layout as Array<{ id: string; visible: boolean }>;
    expect(layout.find((entry) => entry.id === "memory")?.visible).toBe(true);
    // Pinning is a Glass config write only: no memory, runtime, sync, or
    // routing request may fire because of it.
    expect(apiMocks.updateRuntimeConfig.mock.calls.length).toBe(callsBefore.update);
    expect(apiMocks.syncMemorySources.mock.calls.length).toBe(callsBefore.sync);
    expect(apiMocks.getAgentMemoryStatus.mock.calls.length).toBe(callsBefore.status);
    expect(apiMocks.getRuntimeConfig.mock.calls.length).toBe(callsBefore.config);
  });
});

describe("MemoryPage refresh", () => {
  it("reloads memory, routing, and lane statuses on refresh", async () => {
    await render();
    apiMocks.getAgentMemoryStatus.mockClear();
    apiMocks.getRuntimeConfig.mockClear();
    apiMocks.getAgentMemoryLaneStatuses.mockClear();

    await clickButton("Library");
    await clickButton("Refresh");

    expect(apiMocks.getAgentMemoryStatus).toHaveBeenCalledWith(SETTINGS, "lyra");
    expect(apiMocks.getRuntimeConfig).toHaveBeenCalled();
    expect(apiMocks.getAgentMemoryLaneStatuses).toHaveBeenCalledWith(
      SETTINGS,
      "lyra"
    );
  });

  it("reports refresh partial failures without pretending success", async () => {
    await render();
    apiMocks.getRuntimeConfig.mockRejectedValue(new Error("routing refresh down"));

    await clickButton("Library");
    await clickButton("Refresh");

    expect(setNotice).toHaveBeenCalledWith({
      tone: "info",
      message: "Memory loaded, but the lane routing snapshot could not be refreshed.",
    });
  });

  it("reports a memory refresh failure as an error", async () => {
    await render();
    apiMocks.getAgentMemoryStatus.mockRejectedValue(new Error("status refresh died"));

    await clickButton("Library");
    await clickButton("Refresh");

    expect(setNotice).toHaveBeenCalledWith({
      tone: "error",
      message: expect.stringContaining("Memory refresh failed:"),
    });
  });
});
