import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createJob,
  createAuthProfile,
  createWebSocketTicket,
  createBootstrapPreset,
  createSessionRun,
  fetchBoardCardAssetBlob,
  GatewayApiError,
  getAgentMemoryGraphNeighbors,
  getAgentMemoryStatus,
  getJobHistory,
  getRunbookDetail,
  getStrategySummary,
  getGatewayHealth,
  linkTaskBoardCard,
  listRunbooks,
  listTasks,
  getMissionControlUsage,
  removeAgent,
  getRuntimeConfig,
  revokeAuthProfile,
  updateRuntimeConfig,
  websocketUrlFromGateway,
} from "./api";
import { API_REQUEST_TIMEOUT_MS, API_RUN_REQUEST_TIMEOUT_MS } from "../constants";
import { getGatewayToken } from "./runtime";

vi.mock("./runtime", () => ({
  getGatewayToken: vi.fn(),
}));

describe("websocketUrlFromGateway", () => {
  it("normalizes an http gateway URL to ws with ticket query", () => {
    const wsUrl = websocketUrlFromGateway({ gateway_url: "127.0.0.1:18789" }, "ticket-123");
    expect(wsUrl).toBe("ws://127.0.0.1:18789/api/v1/ws?ticket=ticket-123");
  });

  it("upgrades https gateway URL to wss and encodes ticket", () => {
    const wsUrl = websocketUrlFromGateway(
      { gateway_url: "https://carsinos.local:443" },
      "ticket with spaces"
    );
    expect(wsUrl).toBe(
      "wss://carsinos.local/api/v1/ws?ticket=ticket+with+spaces"
    );
  });

  it("throws on invalid gateway URL", () => {
    expect(() => websocketUrlFromGateway({ gateway_url: "https://" }, "x")).toThrow(
      /Invalid Gateway URL/
    );
  });
});

describe("request URL resolution", () => {
  beforeEach(() => {
    vi.mocked(getGatewayToken).mockResolvedValue("token-123");
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("uses the latest gateway URL when runtime settings change mid-session", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await getGatewayHealth({ gateway_url: "http://127.0.0.1:19789" });
    await getGatewayHealth({ gateway_url: "http://127.0.0.1:19890" });

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "http://127.0.0.1:19789/api/v1/health",
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      })
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://127.0.0.1:19890/api/v1/health",
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      })
    );
  });

  it("creates websocket tickets over authenticated HTTP before ws connect", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify({ ticket: "ws-ticket-1", expires_at: 1234 }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      createWebSocketTicket({ gateway_url: "http://127.0.0.1:19789" })
    ).resolves.toEqual({ ticket: "ws-ticket-1", expires_at: 1234 });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:19789/api/v1/ws-ticket",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      })
    );
  });

  it("throws typed config errors when the gateway token is missing", async () => {
    vi.mocked(getGatewayToken).mockResolvedValue(null);

    await expect(getGatewayHealth({ gateway_url: "http://127.0.0.1:19789" })).rejects.toMatchObject(
      {
        name: "GatewayApiError",
        kind: "config",
        path: "/api/v1/health",
      } satisfies Partial<GatewayApiError>
    );
  });

  it("throws typed HTTP errors for asset/blob requests", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response("missing asset", {
        status: 404,
        statusText: "Not Found",
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      fetchBoardCardAssetBlob(
        { gateway_url: "http://127.0.0.1:19789" },
        "board-1",
        "card-1",
        "asset-1"
      )
    ).rejects.toMatchObject({
      name: "GatewayApiError",
      kind: "http",
      status: 404,
      path: "/api/v1/boards/board-1/cards/card-1/assets/asset-1",
      message: "404 Not Found",
      responseBody: "missing asset",
    } satisfies Partial<GatewayApiError>);
  });

  it("includes gateway JSON error details in HTTP error messages", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify({ error: "That session key is already rebound." }), {
        status: 409,
        statusText: "Conflict",
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(getGatewayHealth({ gateway_url: "http://127.0.0.1:19789" })).rejects.toMatchObject(
      {
        name: "GatewayApiError",
        kind: "http",
        status: 409,
        message: "409 Conflict: That session key is already rebound.",
      } satisfies Partial<GatewayApiError>
    );
  });

  it("throws typed timeout errors when fetch aborts", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new DOMException("aborted", "AbortError"));
    vi.stubGlobal("fetch", fetchMock);

    await expect(getGatewayHealth({ gateway_url: "http://127.0.0.1:19789" })).rejects.toMatchObject(
      {
        name: "GatewayApiError",
        kind: "timeout",
        path: "/api/v1/health",
      } satisfies Partial<GatewayApiError>
    );
  });

  it("fetches job history with an encoded job id and bounded limit", async () => {
    const responseBody = {
      items: [
        {
          job_run_id: "run-1",
          job_id: "job/with slash",
          trigger_kind: "manual",
          status: "succeeded",
          attempt: 1,
          started_at: 123,
          ended_at: 456,
          error_text: null,
          output_json: "{}",
          created_at: 120,
        },
      ],
    };
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify(responseBody), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      getJobHistory({ gateway_url: "http://127.0.0.1:19789" }, "job/with slash", 5005)
    ).resolves.toEqual(responseBody);

    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:19789/api/v1/jobs/job%2Fwith%20slash/history?limit=1000",
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      })
    );
  });

  it("creates jobs with the expected endpoint, body, and authorization", async () => {
    const responseBody = {
      job: {
        job_id: "job-1",
        agent_id: "default",
        name: "ExecAss Check in",
        enabled: true,
        schedule_kind: "interval",
        interval_seconds: 3600,
        run_at_ms: null,
        cron_expr: null,
        payload_json: "{}",
        max_retries: 1,
        retry_backoff_ms: 1000,
        timeout_ms: 60000,
        last_run_at: null,
        last_error: null,
        created_at: 123,
        updated_at: 123,
      },
    };
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify(responseBody), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    const payload = {
      agent_id: "default",
      name: "ExecAss Check in",
      enabled: true,
      schedule_kind: "interval",
      interval_seconds: 3600,
      payload_json: { preset: "execass.check_in" },
    };
    await expect(createJob({ gateway_url: "http://127.0.0.1:19789" }, payload)).resolves.toEqual(
      responseBody
    );

    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:19789/api/v1/jobs/add",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
          "Content-Type": "application/json",
        }),
        body: JSON.stringify(payload),
      })
    );
  });

  it("coalesces bursty runtime config reads so hidden panels do not stampede the gateway", async () => {
    const responseBody = {
      config: {
        global: {},
        routing: {},
        channels: {},
        memory: {},
      },
    };
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify(responseBody), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    const settings = { gateway_url: "http://127.0.0.1:19891" };
    const [first, second, third] = await Promise.all([
      getRuntimeConfig(settings),
      getRuntimeConfig(settings),
      getRuntimeConfig(settings),
    ]);

    expect(first).toEqual(responseBody);
    expect(second).toEqual(responseBody);
    expect(third).toEqual(responseBody);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledWith(
      "http://127.0.0.1:19891/api/v1/config/runtime",
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      })
    );
  });

  it("invalidates the runtime config cache after saving config", async () => {
    const firstRuntime = {
      config: {
        global: { assistant_system_prompt: "first" },
        routing: {},
        channels: {},
        memory: {},
      },
    };
    const updatedRuntime = {
      config: {
        global: { assistant_system_prompt: "updated" },
        routing: {},
        channels: {},
        memory: {},
      },
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify(firstRuntime), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        })
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify(updatedRuntime), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        })
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify(updatedRuntime), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        })
      );
    vi.stubGlobal("fetch", fetchMock);

    const settings = { gateway_url: "http://127.0.0.1:19892" };
    await expect(getRuntimeConfig(settings)).resolves.toEqual(firstRuntime);
    await expect(
      updateRuntimeConfig(settings, {
        global: updatedRuntime.config.global as never,
      })
    ).resolves.toEqual(updatedRuntime);
    await expect(getRuntimeConfig(settings)).resolves.toEqual(updatedRuntime);

    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "http://127.0.0.1:19892/api/v1/config/runtime",
      expect.objectContaining({ method: "POST" })
    );
  });

  it("builds mission-control usage query with window + timezone metadata", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(
        JSON.stringify({
          contract_version: "mc-usage-v1",
          available: false,
          window: "today",
          timezone: "UTC",
          currency: "USD",
          window_start_utc: null,
          window_end_utc: null,
          estimated_cost_total: null,
          token_input_total: null,
          token_output_total: null,
          by_agent: null,
          by_model: null,
          by_provider: null,
          by_time: null,
          by_job: null,
          by_card: null,
          budget_thresholds: null,
          updated_at_utc: null,
          reason_code: "USAGE_UNAVAILABLE",
          detail: "stub",
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }
      )
    );
    vi.stubGlobal("fetch", fetchMock);

    await getMissionControlUsage(
      { gateway_url: "http://127.0.0.1:19999" },
      {
        window: "today",
        timezone: "America/Chicago",
        tz_offset_minutes: -360,
      }
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [calledUrl] = fetchMock.mock.calls[0] as [string];
    expect(calledUrl).toContain("/api/v1/mission-control/usage?");
    expect(calledUrl).toContain("window=today");
    expect(calledUrl).toContain("timezone=America%2FChicago");
    expect(calledUrl).toContain("tz_offset_minutes=-360");
  });

  it("hits remove-agent, direct auth profile, and revoke endpoints with POST", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(JSON.stringify({ removed: true, valid: true, profile: {} }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      })
    );
    vi.stubGlobal("fetch", fetchMock);

    await removeAgent({ gateway_url: "http://127.0.0.1:18888" }, "assistant-main");
    await createAuthProfile(
      { gateway_url: "http://127.0.0.1:18888" },
      {
        provider: "anthropic",
        display_name: "claude-primary",
        auth_mode: "api_key",
        risk_level: "high",
        enabled: true,
        kill_switch_scope: "profile",
        api_base_url: "https://api.anthropic.com",
        credentials_json: { api_key: "token-1" },
      }
    );
    await revokeAuthProfile(
      { gateway_url: "http://127.0.0.1:18888" },
      "profile-1",
      { reason: "reauth", remove_secret: true }
    );

    expect(fetchMock).toHaveBeenCalledTimes(3);
    const [removeUrl, removeInit] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(removeUrl).toContain("/api/v1/agents/assistant-main/remove");
    expect(removeInit.method).toBe("POST");
    expect(removeInit.body).toBeUndefined();

    const [profileUrl, profileInit] = fetchMock.mock.calls[1] as [string, RequestInit];
    expect(profileUrl).toContain("/api/v1/auth/profiles");
    expect(profileInit.method).toBe("POST");
    expect(profileInit.body).toBe(
      JSON.stringify({
        provider: "anthropic",
        display_name: "claude-primary",
        auth_mode: "api_key",
        risk_level: "high",
        enabled: true,
        kill_switch_scope: "profile",
        api_base_url: "https://api.anthropic.com",
        credentials_json: { api_key: "token-1" },
      })
    );
    expect((profileInit.headers as Record<string, string>)["Content-Type"]).toBe(
      "application/json"
    );

    const [revokeUrl, revokeInit] = fetchMock.mock.calls[2] as [string, RequestInit];
    expect(revokeUrl).toContain("/api/v1/security/auth-profiles/profile-1/revoke");
    expect(revokeInit.method).toBe("POST");
    expect(revokeInit.body).toBe(
      JSON.stringify({
        reason: "reauth",
        remove_secret: true,
      })
    );
    expect((revokeInit.headers as Record<string, string>)["Content-Type"]).toBe(
      "application/json"
    );
  });

  it("builds strategy query URLs and link mutations with operator metadata", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(
        JSON.stringify({
          generated_at_ms: 0,
          currency: "USD",
          blocked_task_count: 0,
          blocked_tasks: [],
          stale_task_count: 0,
          stale_tasks: [],
          spend_by_agent: [],
          spend_by_project: [],
          unattributed_spend_total: 0,
          goal_progress: [],
          critical_approval_backlog_count: 0,
          critical_approval_backlog: [],
          items: [],
          next_cursor: null,
          task: {
            task_id: "task-1",
            project_id: "project-1",
            parent_task_id: null,
            title: "Task",
            detail: "",
            status: "todo",
            priority: "normal",
            owner_agent_id: null,
            due_at: null,
            blocked_reason: null,
            linked_board_card_id: "card-1",
            linked_job_id: null,
            latest_run_id: null,
            latest_session_id: null,
            created_at: 0,
            updated_at: 0,
          },
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }
      )
    );
    vi.stubGlobal("fetch", fetchMock);

    await getStrategySummary(
      { gateway_url: "http://127.0.0.1:18888" },
      { timezone: "America/Chicago", tz_offset_minutes: -360 }
    );
    await listTasks(
      { gateway_url: "http://127.0.0.1:18888" },
      { limit: 25, cursor: "cursor-1", owner_agent_id: "agent-1", blocked: true }
    );
    await linkTaskBoardCard(
      { gateway_url: "http://127.0.0.1:18888" },
      "task-1",
      { board_card_id: "card-1", force_reassign: true }
    );

    expect(fetchMock).toHaveBeenCalledTimes(3);
    const [summaryUrl] = fetchMock.mock.calls[0] as [string];
    expect(summaryUrl).toContain("/api/v1/mission-control/strategy/summary?");
    expect(summaryUrl).toContain("timezone=America%2FChicago");
    expect(summaryUrl).toContain("tz_offset_minutes=-360");

    const [tasksUrl] = fetchMock.mock.calls[1] as [string];
    expect(tasksUrl).toContain("/api/v1/tasks?");
    expect(tasksUrl).toContain("limit=25");
    expect(tasksUrl).toContain("cursor=cursor-1");
    expect(tasksUrl).toContain("owner_agent_id=agent-1");
    expect(tasksUrl).toContain("blocked=true");

    const [linkUrl, linkInit] = fetchMock.mock.calls[2] as [string, RequestInit];
    expect(linkUrl).toContain("/api/v1/tasks/task-1/links/board-card");
    expect(linkInit.method).toBe("POST");
    expect(linkInit.body).toBe(
      JSON.stringify({
        board_card_id: "card-1",
        force_reassign: true,
      })
    );
  });

  it("builds runbook list filters and detail paths", async () => {
    const fetchMock = vi
      .fn()
      .mockImplementationOnce(async () =>
        new Response(
          JSON.stringify({
            generated_at_ms: 0,
            items: [],
            counts_by_status: {
              pending: 0,
              active: 0,
              waiting: 0,
              blocked: 0,
              failed: 0,
              completed: 0,
              limited: 0,
            },
            next_cursor: null,
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      )
      .mockImplementationOnce(async () =>
        new Response(
          JSON.stringify({
            runbook_id: "strategy_task_execution:task-1",
            runbook_kind: "strategy_task_execution",
            template_id: "strategy-task-execution",
            template_version: "mc-runbook-v1",
            anchor_kind: "task",
            anchor_id: "task-1/primary",
            title: "Task runbook",
            status: "blocked",
            status_reason: "Waiting on approval",
            generated_at_ms: 0,
            selected_execution_ref: null,
            active_step_id: "blocked",
            next_step_ids: ["resume"],
            linked_entities: [],
            steps: [],
            history: [],
            actions: [],
            source_facts: [],
            availability: {
              is_limited: false,
              is_stale: false,
              last_refresh_at_ms: 0,
              missing_source_kinds: [],
              stale_reason: null,
            },
            warnings: [],
            owner_agent_id: "agent-1",
            owner_agent_label: "Agent 1",
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          }
        )
      );
    vi.stubGlobal("fetch", fetchMock);

    await listRunbooks(
      { gateway_url: "http://127.0.0.1:18888" },
      {
        kind: "strategy_task_execution",
        status: "blocked",
        owner_agent_id: "agent-1",
        query: "approval backlog",
        linked_task_id: "task-1",
        linked_project_id: "project-1",
        linked_goal_id: "goal-1",
        limit: 25,
        cursor: "cursor token",
      }
    );
    await getRunbookDetail(
      { gateway_url: "http://127.0.0.1:18888" },
      "strategy_task_execution",
      "task-1/primary"
    );

    expect(fetchMock).toHaveBeenCalledTimes(2);

    const [listUrl] = fetchMock.mock.calls[0] as [string];
    expect(listUrl).toContain("/api/v1/mission-control/runbooks?");
    expect(listUrl).toContain("kind=strategy_task_execution");
    expect(listUrl).toContain("status=blocked");
    expect(listUrl).toContain("owner_agent_id=agent-1");
    expect(listUrl).toContain("query=approval+backlog");
    expect(listUrl).toContain("linked_task_id=task-1");
    expect(listUrl).toContain("linked_project_id=project-1");
    expect(listUrl).toContain("linked_goal_id=goal-1");
    expect(listUrl).toContain("limit=25");
    expect(listUrl).toContain("cursor=cursor+token");

    const [detailUrl] = fetchMock.mock.calls[1] as [string];
    expect(detailUrl).toContain(
      "/api/v1/mission-control/runbooks/strategy_task_execution/task-1%2Fprimary"
    );
  });

  it("posts bootstrap preset manager defaults", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(
        JSON.stringify({
          preset: {
            schema_version: "bootstrap-preset-v1",
            preset_key: "lead",
            display_name: "Lead",
            description: "desc",
            role_label: "Lead",
            provider_path: "openai",
            default_model_provider: "openai",
            default_model_id: "gpt-5",
            default_tool_profile: "standard",
            default_workspace_root: ".",
            default_reports_to_agent_id: "agent-root",
            setup_notes: "notes",
            created_at: 0,
            updated_at: 0,
          },
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }
      )
    );
    vi.stubGlobal("fetch", fetchMock);

    await createBootstrapPreset(
      { gateway_url: "http://127.0.0.1:18888" },
      {
        preset_key: "lead",
        display_name: "Lead",
        description: "desc",
        role_label: "Lead",
        provider_path: "openai",
        default_model_provider: "openai",
        default_model_id: "gpt-5",
        default_tool_profile: "standard",
        default_workspace_root: ".",
        default_reports_to_agent_id: "agent-root",
        setup_notes: "notes",
      }
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [presetUrl, presetInit] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(presetUrl).toContain("/api/v1/bootstrap-presets");
    expect(presetInit.method).toBe("POST");
    expect(presetInit.body).toBe(
      JSON.stringify({
        preset_key: "lead",
        display_name: "Lead",
        description: "desc",
        role_label: "Lead",
        provider_path: "openai",
        default_model_provider: "openai",
        default_model_id: "gpt-5",
        default_tool_profile: "standard",
        default_workspace_root: ".",
        default_reports_to_agent_id: "agent-root",
        setup_notes: "notes",
      })
    );
  });

  it("builds assistant memory wrapper URLs with bounded graph parameters", async () => {
    const fetchMock = vi.fn().mockImplementation(async () =>
      new Response(
        JSON.stringify({
          status: {
            agent_id: "lyra",
            binding_status: "available",
            binding: null,
            native_surface_availability: {},
            orchestration: {},
            native_runtime_status: null,
            native_runtime_health_mismatch: false,
          },
          agent_id: "lyra",
          binding_id: "mno-lyra",
          data: {
            ok: true,
            node: { atom_id: "atm-1", kind: "event_card" },
            neighbors: [],
            links: [],
            depth: 1,
            node_limit: 36,
            link_limit: 72,
            requests_used: 1,
            truncated: false,
          },
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }
      )
    );
    vi.stubGlobal("fetch", fetchMock);

    await getAgentMemoryStatus({ gateway_url: "http://127.0.0.1:18888" }, "lyra");
    await getAgentMemoryGraphNeighbors(
      { gateway_url: "http://127.0.0.1:18888" },
      "lyra",
      {
        atom_id: "atm-1",
        depth: 1,
        node_limit: 36,
        link_limit: 72,
        include_root_detail: true,
        include_shared_language: false,
      }
    );

    expect(fetchMock).toHaveBeenCalledTimes(2);
    const [statusUrl] = fetchMock.mock.calls[0] as [string];
    expect(statusUrl).toContain("/api/v1/agents/lyra/memory/status");

    const [neighborsUrl] = fetchMock.mock.calls[1] as [string];
    expect(neighborsUrl).toContain("/api/v1/agents/lyra/memory/graph/neighbors?");
    expect(neighborsUrl).toContain("atom_id=atm-1");
    expect(neighborsUrl).toContain("depth=1");
    expect(neighborsUrl).toContain("node_limit=36");
    expect(neighborsUrl).toContain("link_limit=72");
    expect(neighborsUrl).toContain("include_root_detail=true");
    expect(neighborsUrl).toContain("include_shared_language=false");
  });

  it("uses the longer run timeout for blocking model execution", async () => {
    vi.useFakeTimers();
    vi.mocked(getGatewayToken).mockResolvedValue("token-123");
    const fetchMock = vi.fn((_url: string, init?: RequestInit) => {
      return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          reject(new DOMException("aborted", "AbortError"));
        });
      });
    });
    vi.stubGlobal("fetch", fetchMock);

    const promise = createSessionRun(
      { gateway_url: "http://127.0.0.1:18789" },
      "session-1"
    );
    const earlyFailure = vi.fn();
    promise.catch(earlyFailure);

    await vi.advanceTimersByTimeAsync(API_REQUEST_TIMEOUT_MS + 1);
    await Promise.resolve();
    expect(earlyFailure).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(API_RUN_REQUEST_TIMEOUT_MS - API_REQUEST_TIMEOUT_MS);
    await expect(promise).rejects.toMatchObject({
      kind: "timeout",
      message: `Gateway request timed out after ${API_RUN_REQUEST_TIMEOUT_MS}ms.`,
    });
    vi.useRealTimers();
  });
});
