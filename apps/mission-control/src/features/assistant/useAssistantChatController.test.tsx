// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { NotifyFn } from "../../app/useAppController";
import type { BoardSummary } from "../../app/useRuntimeConnectionController";
import type {
  Agent,
  AuthProfileResponse,
  BoardDetail,
  CreateMessageResponse,
  CreateRunResponse,
  CreateSessionResponse,
  GetRuntimeConfigResponse,
  ListMessagesResponse,
  ListProviderCapabilitiesResponse,
  ListProviderModelsResponse,
  RuntimeConnectionSettings,
  UpdateBoardCardResponse,
} from "../../types";
import { useAssistantChatController } from "./useAssistantChatController";
import {
  createBoardCard,
  createSession,
  createSessionMessage,
  createSessionRun,
  getBoard,
  getRuntimeConfig,
  getSession,
  listProviderCapabilities,
  listProviderModels,
  listSessionMessages,
} from "../../lib/api";

vi.mock("../../lib/api", () => ({
  createBoardCard: vi.fn(),
  createSession: vi.fn(),
  createSessionMessage: vi.fn(),
  createSessionRun: vi.fn(),
  getSession: vi.fn(),
  getBoard: vi.fn(),
  getRuntimeConfig: vi.fn(),
  listProviderCapabilities: vi.fn(),
  listProviderModels: vi.fn(),
  listSessionMessages: vi.fn(),
}));

function makeAgent(agent_id: string): Agent {
  return {
    agent_id,
    name: agent_id,
    model_provider: "anthropic",
    model_id: "claude-sonnet-4-5",
    memory_binding: null,
  };
}

function makeSessionResponse(
  sessionId: string,
  agentId: string,
  sessionKey = `lane:human:local-operator:assistant:${agentId}:main`
): CreateSessionResponse {
  return {
    session: {
      session_id: sessionId,
      session_key: sessionKey,
      agent_id: agentId,
      title: null,
      created_at: 1,
      updated_at: 1,
      closed_at: null,
      message_count: 0,
      run_count: 0,
    },
  };
}

function makeRuntimeConfigResponse(
  enabled = true,
  assignedAgentIds: string[] = ["claude"],
  localOperatorEnabled = true
): GetRuntimeConfigResponse {
  return {
    config: {
      schema_version: "runtime.v1",
      global: {
        jwt_issuer_allowlist: [],
        jwt_audience_allowlist: [],
        trusted_proxy_allowlist: [],
        tls_termination_mode: "none",
        public_base_url: null,
        assistant_system_prompt: null,
      },
      providers: [],
      channels: {
        discord: {
          enabled: false,
          bot_token_secret_ref: null,
          operation_mode: "transport",
          api_base_url: null,
          transport_timeout_ms: null,
          transport_retry_attempts: null,
          application_id: null,
          intents: [],
          staging_guild_ids: [],
          staging_channel_ids: [],
        },
        telegram: {
          enabled: false,
          bot_token_secret_ref: null,
          operation_mode: "transport",
          api_base_url: null,
          transport_timeout_ms: null,
          transport_retry_attempts: null,
          long_poll_timeout_seconds: null,
          webhook_mode: "disabled",
          webhook_url: null,
          staging_chat_ids: [],
        },
      },
      routing: {
        enabled,
        use_channel_defaults_as_fallback: false,
        local_operator_human_identity_id:
          assignedAgentIds.length > 0 ? "local-operator" : null,
        dm_unmapped_policy: "approval_required",
        shared_unmapped_policy: "block",
        human_identities: [
          {
            human_identity_id: "local-operator",
            display_name: "You",
            enabled: localOperatorEnabled,
          },
        ],
        platform_identity_links: [],
        assistant_assignments: assignedAgentIds.map((assistant_agent_id) => ({
          human_identity_id: "local-operator",
          assistant_agent_id,
          enabled: true,
        })),
        lane_memory_policies: [],
      },
      memory: {
        blend_mode: "smart",
        memory_md_sources: [],
        numquam: {},
      },
      extensions: {},
      security: {},
      autonomy_guardrails: {},
      updated_at: 1,
    },
  };
}

function makeCapabilitiesResponse(): ListProviderCapabilitiesResponse {
  return {
    contract_version: "providers.capabilities.v1",
    items: [
      {
        provider: "anthropic",
        supports_streaming: true,
        supports_tools: true,
        supports_json_mode: true,
        supports_vision: true,
        max_context_tokens: 200_000,
        error_classes: [],
        retryable_error_classes: [],
      },
    ],
  };
}

function makeModelsResponse(): ListProviderModelsResponse {
  return {
    contract_version: "providers.models.v1",
    provider: "anthropic",
    auth_profile_id: null,
    items: [
      {
        model_id: "claude-sonnet-4-5",
        label: "Claude Sonnet 4.5",
      },
    ],
  };
}

function makeMessagesResponse(): ListMessagesResponse {
  return { items: [] };
}

function makeRunResponse(sessionId: string): CreateRunResponse {
  return {
    run: {
      run_id: "run-1",
      session_id: sessionId,
      status: "succeeded",
      model_provider: "anthropic",
      model_id: "claude-sonnet-4-5",
      started_at: 1,
      ended_at: 2,
      error_text: null,
      usage_json: null,
      created_at: 1,
    },
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((innerResolve, innerReject) => {
    resolve = innerResolve;
    reject = innerReject;
  });
  return { promise, resolve, reject };
}

function makeMessageResponse(): CreateMessageResponse {
  return {
    message: {
      message_id: "msg-1",
      session_id: "sess-1",
      source_channel: "assistant-chat",
      source_peer_id: null,
      source_message_id: null,
      role: "user",
      content_text: "hello",
      content_format: "markdown",
      created_at: 1,
    },
  };
}

type Controller = ReturnType<typeof useAssistantChatController>;

function Harness(props: {
  onReady: (controller: Controller) => void;
  settings: RuntimeConnectionSettings;
  agents: Agent[];
  authProfiles?: AuthProfileResponse[];
  setNotice: NotifyFn;
  corePromptDirty?: boolean;
  saveCorePrompt?: () => Promise<void>;
  boards?: BoardSummary[];
}) {
  const controller = useAssistantChatController({
    settings: props.settings,
    tokenConfigured: true,
    agents: props.agents,
    authProfiles: props.authProfiles ?? [],
    boards: props.boards ?? [],
    setNotice: props.setNotice,
    corePrompt: "You are carsinOS.",
    corePromptSaved: "You are carsinOS.",
    corePromptLoading: false,
    corePromptSaving: false,
    corePromptError: null,
    corePromptDirty: props.corePromptDirty ?? false,
    setCorePrompt: () => {},
    saveCorePrompt: props.saveCorePrompt ?? (async () => {}),
    resetCorePrompt: () => {},
    restoreDefaultCorePrompt: () => {},
  });
  useEffect(() => {
    props.onReady(controller);
  }, [controller, props]);
  return null;
}

describe("useAssistantChatController", () => {
  let container: HTMLDivElement;
  let root: Root;
  let latest: Controller | null;

  const settings: RuntimeConnectionSettings = {
    gateway_url: "http://127.0.0.1:18789",
  };

  const flush = async () => {
    await act(async () => {
      await Promise.resolve();
    });
  };

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    latest = null;
    // @ts-expect-error test-only global
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;

    vi.mocked(listProviderCapabilities).mockResolvedValue(makeCapabilitiesResponse());
    vi.mocked(listProviderModels).mockResolvedValue(makeModelsResponse());
    vi.mocked(listSessionMessages).mockResolvedValue(makeMessagesResponse());
    vi.mocked(getRuntimeConfig).mockResolvedValue(makeRuntimeConfigResponse());
    vi.mocked(createSession).mockResolvedValue(makeSessionResponse("sess-1", "claude"));
    vi.mocked(createSessionMessage).mockResolvedValue(makeMessageResponse());
    vi.mocked(createSessionRun).mockResolvedValue(makeRunResponse("sess-1"));
    vi.mocked(getSession).mockResolvedValue(makeSessionResponse("sess-1", "claude"));
    vi.mocked(getBoard).mockResolvedValue({
      board: {
        board_id: "board-1",
        board_key: "board-1",
        name: "Board 1",
        board_type: "kanban",
        created_at: 1,
        updated_at: 1,
        column_count: 0,
        card_count: 0,
      },
      columns: [],
      cards: [],
    } satisfies BoardDetail);
    vi.mocked(createBoardCard).mockResolvedValue({
      card: {
        card_id: "card-1",
        board_id: "board-1",
        column_id: "col-1",
        title: "Card",
        description: null,
        owner_kind: "agent",
        owner_agent_id: "claude",
        owner_human_id: null,
        due_at: null,
        tags: [],
        script_markdown: null,
        linked_session_id: null,
        latest_run_id: null,
        position: 1,
        created_at: 1,
        updated_at: 1,
        assets: [],
      },
    } satisfies UpdateBoardCardResponse);
  });

  afterEach(async () => {
    await act(async () => {
      root.unmount();
    });
    document.body.innerHTML = "";
    vi.clearAllMocks();
    vi.unstubAllGlobals();
  });

  it("stays stable while agents are still loading and does not read a missing first agent", async () => {
    const setNotice = vi.fn();

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    expect(latest).toBeTruthy();
    expect(latest?.selectedAgentId).toBe("");
    expect(setNotice).not.toHaveBeenCalledWith(
      expect.objectContaining({
        tone: "error",
      })
    );
  });

  it("creates Assistant sessions through the canonical human lane without injecting a system message", async () => {
    const setNotice = vi.fn();
    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    expect(latest).toBeTruthy();
    await act(async () => {
      latest?.setDraft("Hello from Assistant");
    });
    await act(async () => {
      await latest?.send();
    });

    expect(createSession).toHaveBeenCalledWith(settings, {
      agent_id: "claude",
      human_identity_id: "local-operator",
    });
    expect(createSessionMessage).toHaveBeenCalledTimes(1);
    expect(createSessionMessage).toHaveBeenCalledWith(settings, "sess-1", {
      role: "user",
      content_text: "Hello from Assistant",
      content_format: "markdown",
      source_channel: "assistant-chat",
    });
    expect(setNotice).not.toHaveBeenCalledWith(
      expect.objectContaining({
        message: expect.stringContaining("People-based lane routing is off"),
      })
    );
  });

  it("shows the pending user message while the model run is still blocking", async () => {
    const setNotice = vi.fn();
    const runDeferred = deferred<CreateRunResponse>();
    vi.mocked(createSessionRun).mockReturnValueOnce(runDeferred.promise);
    vi.mocked(createSessionMessage).mockResolvedValueOnce({
      message: {
        ...makeMessageResponse().message,
        content_text: "show progress please",
      },
    });

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    await act(async () => {
      latest?.setDraft("show progress please");
    });

    let sendPromise: Promise<void> | undefined;
    await act(async () => {
      sendPromise = latest?.send();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(latest?.busy).toBe(true);
    expect(latest?.sendStatus).toContain("Waiting for");
    expect(latest?.messages.some((message) => message.content_text === "show progress please"))
      .toBe(true);

    await act(async () => {
      runDeferred.resolve(makeRunResponse("sess-1"));
      await sendPromise;
    });
  });

  it("uses the explicit local assistant route even when the legacy routing toggle is disabled", async () => {
    const setNotice = vi.fn();
    vi.mocked(getRuntimeConfig).mockResolvedValue(makeRuntimeConfigResponse(false));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();
    await flush();

    await act(async () => {
      latest?.setDraft("Hello from Assistant");
    });
    await act(async () => {
      await latest?.send();
    });


    expect(createSession).toHaveBeenCalledWith(settings, {
      agent_id: "claude",
      human_identity_id: "local-operator",
    });
    expect(setNotice).not.toHaveBeenCalledWith(
      expect.objectContaining({
        message: expect.stringContaining("People-based lane routing is off"),
      })
    );
  });

  it("refuses to open a pinned transcript when Team routing cannot be loaded on a cold start", async () => {
    const setNotice = vi.fn();
    vi.mocked(getRuntimeConfig).mockRejectedValue(new Error("routing offline"));
    vi.mocked(getSession).mockResolvedValue(makeSessionResponse("sess-2", "lyra"));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude"), makeAgent("lyra")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    await act(async () => {
      await latest?.openSession("sess-2");
    });

    expect(getSession).not.toHaveBeenCalled();
    expect(latest?.sessionMode).toBe("canonical_lane");
    expect(setNotice).toHaveBeenCalledWith(
      expect.objectContaining({
        tone: "error",
        message: expect.stringContaining("Team routing could not load cleanly"),
      })
    );
  });

  it("refreshes Team routing before opening a session and allows a newly assigned assistant", async () => {
    const setNotice = vi.fn();
    vi.mocked(getRuntimeConfig).mockResolvedValue(
      makeRuntimeConfigResponse(true)
    );
    vi.mocked(getRuntimeConfig)
      .mockResolvedValueOnce(makeRuntimeConfigResponse(true, ["claude"]))
      .mockResolvedValueOnce(makeRuntimeConfigResponse(true, ["lyra"]));
    vi.mocked(getSession).mockResolvedValue(makeSessionResponse("sess-2", "lyra"));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude"), makeAgent("lyra")]}
          setNotice={setNotice}
          authProfiles={[]}
        />
      );
    });
    await flush();
    expect(latest?.selectedAgentId).toBe("claude");

    await act(async () => {
      await latest?.openSession("sess-2");
    });
    await flush();
    await flush();

    expect(getSession).toHaveBeenCalledWith(settings, "sess-2");
    expect(latest?.selectedAgentId).toBe("lyra");
    expect(latest?.sessionId).toBe("sess-2");
  });

  it("does not reuse cached routing after the gateway URL changes and the new gateway fails", async () => {
    const setNotice = vi.fn();
    const firstSettings = settings;
    const secondSettings: RuntimeConnectionSettings = {
      gateway_url: "http://127.0.0.1:18890",
    };
    vi.mocked(getRuntimeConfig)
      .mockResolvedValueOnce(makeRuntimeConfigResponse(true, ["lyra"]))
      .mockRejectedValueOnce(new Error("new gateway offline"));
    vi.mocked(getSession).mockResolvedValue(makeSessionResponse("sess-2", "lyra"));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={firstSettings}
          agents={[makeAgent("lyra")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={secondSettings}
          agents={[makeAgent("lyra")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    await act(async () => {
      await latest?.openSession("sess-2");
    });

    expect(getSession).not.toHaveBeenCalled();
    expect(setNotice).toHaveBeenCalledWith(
      expect.objectContaining({
        tone: "error",
        message: expect.stringContaining("Team routing could not load cleanly"),
      })
    );
  });

  it("re-resolves the canonical lane on later sends instead of sticking to a stale session id", async () => {
    const setNotice = vi.fn();
    vi.mocked(createSession)
      .mockResolvedValueOnce(makeSessionResponse("sess-1", "claude"))
      .mockResolvedValueOnce(makeSessionResponse("sess-2", "claude"));
    vi.mocked(createSessionRun)
      .mockResolvedValueOnce(makeRunResponse("sess-1"))
      .mockResolvedValueOnce(makeRunResponse("sess-2"));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();
    vi.mocked(getRuntimeConfig).mockClear();

    await act(async () => {
      latest?.setDraft("first");
    });
    await act(async () => {
      await latest?.send();
    });
    expect(latest?.sessionId).toBe("sess-1");

    await act(async () => {
      latest?.setDraft("second");
    });
    await act(async () => {
      await latest?.send();
    });

    expect(createSession).toHaveBeenCalledTimes(2);
    expect(createSessionRun).toHaveBeenLastCalledWith(settings, "sess-2", {});
    expect(latest?.sessionId).toBe("sess-2");
    expect(getRuntimeConfig).not.toHaveBeenCalled();
  });

  it("does not restore a persisted message into the draft when run creation fails", async () => {
    const setNotice = vi.fn();
    vi.mocked(createSessionMessage).mockResolvedValueOnce({
      message: {
        ...makeMessageResponse().message,
        message_id: "msg-persisted",
        content_text: "already persisted",
      },
    });
    vi.mocked(createSessionRun).mockRejectedValueOnce(new Error("provider down"));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    await act(async () => {
      latest?.setDraft("already persisted");
    });
    await act(async () => {
      await latest?.send();
    });

    expect(createSessionMessage).toHaveBeenCalledTimes(1);
    expect(createSessionRun).toHaveBeenCalledTimes(1);
    expect(latest?.draft).toBe("");
    expect(latest?.messages.some((message) => message.message_id === "msg-persisted")).toBe(true);
  });

  it("keeps sending on an explicitly opened pinned transcript until returning to the canonical lane", async () => {
    const setNotice = vi.fn();
    vi.mocked(getRuntimeConfig).mockResolvedValue(makeRuntimeConfigResponse(true, ["lyra"]));
    vi.mocked(getSession).mockResolvedValue(makeSessionResponse("sess-2", "lyra"));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude"), makeAgent("lyra")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    await act(async () => {
      await latest?.openSession("sess-2");
    });
    await flush();

    expect(latest?.selectedAgentId).toBe("lyra");
    expect(latest?.sessionId).toBe("sess-2");
    expect(latest?.sessionMode).toBe("pinned_session");

    await act(async () => {
      latest?.setDraft("stay pinned");
    });
    await act(async () => {
      await latest?.send();
    });

    expect(createSession).not.toHaveBeenCalled();
    expect(createSessionRun).toHaveBeenCalledWith(settings, "sess-2", {
      model_provider: "anthropic",
      model_id: "claude-sonnet-4-5",
    });
  });

  it("revalidates pinned transcripts on send and drops back to the canonical lane after routing drift", async () => {
    const setNotice = vi.fn();
    vi.mocked(getRuntimeConfig)
      .mockResolvedValueOnce(makeRuntimeConfigResponse(true, ["lyra"]))
      .mockResolvedValueOnce(makeRuntimeConfigResponse(true, ["lyra"]))
      .mockResolvedValueOnce(makeRuntimeConfigResponse(true, ["claude"]));
    vi.mocked(getSession).mockResolvedValue(makeSessionResponse("sess-2", "lyra"));

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude"), makeAgent("lyra")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    await act(async () => {
      await latest?.openSession("sess-2");
    });
    await flush();

    await act(async () => {
      latest?.setDraft("drifted");
    });
    await act(async () => {
      await latest?.send();
    });

    expect(createSessionRun).not.toHaveBeenCalled();
    expect(latest?.sessionMode).toBe("canonical_lane");
    expect(latest?.sessionId).toBeNull();
    expect(setNotice).toHaveBeenCalledWith(
      expect.objectContaining({
        tone: "error",
        message: expect.stringContaining("not routed to the local operator anymore"),
      })
    );
  });

  it("keeps disabled local operators from surfacing assistant choices", async () => {
    const setNotice = vi.fn();
    vi.mocked(getRuntimeConfig).mockResolvedValue(
      makeRuntimeConfigResponse(true, ["claude"], false)
    );

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude"), makeAgent("lyra")]}
          setNotice={setNotice}
        />
      );
    });
    await flush();

    expect(latest?.availableAgents).toHaveLength(0);
    expect(latest?.assistantAvailabilityMessage).toContain("disabled");
  });

  it("saves the shared prompt without mutating the lane transcript", async () => {
    const setNotice = vi.fn();
    const saveCorePrompt = vi.fn().mockResolvedValue(undefined);

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude")]}
          setNotice={setNotice}
          corePromptDirty
          saveCorePrompt={saveCorePrompt}
        />
      );
    });
    await flush();

    await act(async () => {
      await latest?.injectCorePrompt();
    });

    expect(saveCorePrompt).toHaveBeenCalledTimes(1);
    expect(createSessionMessage).not.toHaveBeenCalled();
    expect(setNotice).toHaveBeenCalledWith(
      expect.objectContaining({
        tone: "info",
        message: expect.stringContaining("Shared prompt saved"),
      })
    );
  });

  it("uses the pinned assistant as the board owner when exporting a pinned reply", async () => {
    const setNotice = vi.fn();
    vi.mocked(getRuntimeConfig).mockResolvedValue(makeRuntimeConfigResponse(true, ["lyra"]));
    vi.mocked(getSession).mockResolvedValue(makeSessionResponse("sess-2", "lyra"));
    vi.mocked(listSessionMessages).mockResolvedValue({
      items: [
        {
          message_id: "msg-a",
          session_id: "sess-2",
          source_channel: "assistant-chat",
          source_peer_id: null,
          source_message_id: null,
          role: "assistant",
          content_text: "Pinned reply",
          content_format: "markdown",
          created_at: 2,
        },
      ],
    });
    vi.mocked(getBoard).mockResolvedValue({
      board: {
        board_id: "board-1",
        board_key: "board-1",
        name: "Board 1",
        board_type: "kanban",
        created_at: 1,
        updated_at: 1,
        column_count: 1,
        card_count: 0,
      },
      columns: [
        {
          column_id: "col-1",
          board_id: "board-1",
          column_key: "todo",
          name: "Todo",
          position: 1,
          created_at: 1,
          updated_at: 1,
        },
      ],
      cards: [],
    } satisfies BoardDetail);

    await act(async () => {
      root.render(
        <Harness
          onReady={(controller) => {
            latest = controller;
          }}
          settings={settings}
          agents={[makeAgent("claude"), makeAgent("lyra")]}
          setNotice={setNotice}
          boards={[{ board_id: "board-1", name: "Board 1" }]}
        />
      );
    });
    await flush();

    await act(async () => {
      await latest?.openSession("sess-2");
    });
    await flush();

    await act(async () => {
      await latest?.sendLastAssistantToBoard();
    });

    expect(createBoardCard).toHaveBeenCalledWith(settings, "board-1", {
      column_id: "col-1",
      title: "Pinned reply",
      owner_kind: "agent",
      owner_agent_id: "lyra",
    });
  });
});
