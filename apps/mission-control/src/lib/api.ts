import { getGatewayToken } from "./runtime";
import { API_REQUEST_TIMEOUT_MS, DEFAULT_GATEWAY_URL } from "../constants";
import type {
  AckAgentMailMessageResponse,
  AgentMailFileLeaseResponse,
  AgentMailThreadDetailResponse,
  AgentProviderProfileOrderResponse,
  AnthropicSetupTokenIngestResponse,
  AuthProfileResponse,
  BoardDetail,
  BoardDetailResponse,
  CreateAgentResponse,
  CreateAgentMailFileLeaseResponse,
  CreateAgentMailThreadResponse,
  CreateAuthProfileResponse,
  CreateMemoryNoteResponse,
  GetChannelRuntimeStatusResponse,
  HealthResponse,
  JobStatusResponse,
  ListAgentMailFileLeasesResponse,
  ListAgentMailMessagesResponse,
  ListAgentMailThreadsResponse,
  ListMemoryNotesResponse,
  ListAuthProfilesResponse,
  ListApprovalsResponse,
  ListJobsResponse,
  ListPluginRuntimeStatusResponse,
  ListPluginsResponse,
  ListProviderCapabilitiesResponse,
  ListProviderModelsResponse,
  ListSkillsResponse,
  ListAgentsResponse,
  ListBoardsResponse,
  MissionControlCalendarWeekResponse,
  MissionControlFocusResponse,
  MoveBoardCardResponse,
  OpenAiOauthFinishResponse,
  OpenAiOauthStartResponse,
  PluginManifestResponse,
  ReleaseAgentMailFileLeaseResponse,
  ResolveApprovalResponse,
  RuntimeConnectionSettings,
  RunJobNowResponse,
  RunBoardCardResponse,
  SendAgentMailMessageResponse,
  StatusResponse,
  UpdatePluginResponse,
  UpdateSkillStateResponse,
  UpdateJobResponse,
  UpdateBoardCardResponse,
  UpdateAgentResponse,
  UploadAgentMailAttachmentResponse,
  UploadBoardCardAssetResponse,
} from "../types";

function normalizeGatewayUrl(gatewayUrl: string): string {
  const trimmed = gatewayUrl.trim();
  if (!trimmed) {
    throw new Error("Gateway URL is required.");
  }

  const withScheme =
    trimmed.startsWith("http://") || trimmed.startsWith("https://")
      ? trimmed
      : `http://${trimmed}`;

  try {
    const parsed = new URL(withScheme);
    return `${parsed.origin}/`;
  } catch {
    throw new Error(
      `Invalid Gateway URL: "${trimmed}". Expected something like "${DEFAULT_GATEWAY_URL}".`
    );
  }
}

function resolveApiUrl(gatewayUrl: string, path: string): string {
  const base = normalizeGatewayUrl(gatewayUrl);
  const normalizedPath = path.startsWith("/") ? path.slice(1) : path;
  return new URL(normalizedPath, base).toString();
}

interface ApiRequestOptions {
  method?: "GET" | "POST";
  body?: unknown;
}

async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs = API_REQUEST_TIMEOUT_MS
): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = globalThis.setTimeout(() => {
    controller.abort();
  }, timeoutMs);
  try {
    return await fetch(url, {
      ...init,
      signal: controller.signal,
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new Error(`Gateway request timed out after ${timeoutMs}ms.`);
    }
    throw error;
  } finally {
    globalThis.clearTimeout(timeoutId);
  }
}

async function requestJson<T>(
  settings: RuntimeConnectionSettings,
  path: string,
  options: ApiRequestOptions = {}
): Promise<T> {
  const token = await getGatewayToken();
  if (!token) {
    throw new Error("Gateway token is not configured.");
  }

  const response = await fetchWithTimeout(
    resolveApiUrl(settings.gateway_url, path),
    {
      method: options.method ?? "GET",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    }
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${response.status} ${response.statusText}: ${text}`);
  }

  return (await response.json()) as T;
}

export async function getGatewayHealth(
  settings: RuntimeConnectionSettings
): Promise<HealthResponse> {
  return requestJson<HealthResponse>(settings, "/api/v1/health");
}

export async function getGatewayStatus(
  settings: RuntimeConnectionSettings
): Promise<StatusResponse> {
  return requestJson<StatusResponse>(settings, "/api/v1/status");
}

export async function listProviderCapabilities(
  settings: RuntimeConnectionSettings,
  query?: {
    provider?: string;
  }
): Promise<ListProviderCapabilitiesResponse> {
  const params = new URLSearchParams();
  if (query?.provider?.trim()) {
    params.set("provider", query.provider.trim());
  }
  const suffix = params.toString();
  const path = suffix
    ? `/api/v1/providers/capabilities?${suffix}`
    : "/api/v1/providers/capabilities";
  return requestJson<ListProviderCapabilitiesResponse>(settings, path);
}

export async function listProviderModels(
  settings: RuntimeConnectionSettings,
  query: {
    provider: string;
    agent_id?: string;
    auth_profile_id?: string;
    refresh?: boolean;
  }
): Promise<ListProviderModelsResponse> {
  const provider = query.provider.trim();
  if (!provider) {
    throw new Error("provider is required");
  }
  const params = new URLSearchParams();
  params.set("provider", provider);
  if (query.agent_id?.trim()) {
    params.set("agent_id", query.agent_id.trim());
  }
  if (query.auth_profile_id?.trim()) {
    params.set("auth_profile_id", query.auth_profile_id.trim());
  }
  if (query.refresh === true) {
    params.set("refresh", "true");
  }
  return requestJson<ListProviderModelsResponse>(
    settings,
    `/api/v1/providers/models?${params.toString()}`
  );
}

export async function listBoards(
  settings: RuntimeConnectionSettings
): Promise<ListBoardsResponse> {
  return requestJson<ListBoardsResponse>(settings, "/api/v1/boards");
}

export async function getBoard(
  settings: RuntimeConnectionSettings,
  boardId: string
): Promise<BoardDetail> {
  const response = await requestJson<BoardDetailResponse>(
    settings,
    `/api/v1/boards/${encodeURIComponent(boardId)}`
  );
  return {
    board: response.board,
    columns: [...response.columns].sort((a, b) => a.position - b.position),
    cards: [...response.cards].sort((a, b) => a.position - b.position),
  };
}

export async function listAgents(
  settings: RuntimeConnectionSettings
): Promise<ListAgentsResponse> {
  return requestJson<ListAgentsResponse>(settings, "/api/v1/agents");
}

export async function createAgent(
  settings: RuntimeConnectionSettings,
  payload: {
    agent_id: string;
    name: string;
    workspace_root?: string;
    model_provider?: string;
    model_id?: string;
    tool_profile?: string;
  }
): Promise<CreateAgentResponse> {
  return requestJson<CreateAgentResponse>(settings, "/api/v1/agents", {
    method: "POST",
    body: payload,
  });
}

export async function updateAgent(
  settings: RuntimeConnectionSettings,
  agentId: string,
  payload: {
    name?: string;
    workspace_root?: string;
    model_provider?: string;
    model_id?: string;
    tool_profile?: string;
  }
): Promise<UpdateAgentResponse> {
  return requestJson<UpdateAgentResponse>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}/update`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function getMissionControlCalendarWeek(
  settings: RuntimeConnectionSettings
): Promise<MissionControlCalendarWeekResponse> {
  return requestJson<MissionControlCalendarWeekResponse>(
    settings,
    "/api/v1/mission-control/calendar/week"
  );
}

export async function getMissionControlFocus(
  settings: RuntimeConnectionSettings,
  limit = 50
): Promise<MissionControlFocusResponse> {
  return requestJson<MissionControlFocusResponse>(
    settings,
    `/api/v1/mission-control/focus?limit=${encodeURIComponent(String(limit))}`
  );
}

export async function listMemoryNotes(
  settings: RuntimeConnectionSettings,
  limit = 20
): Promise<ListMemoryNotesResponse> {
  return requestJson<ListMemoryNotesResponse>(
    settings,
    `/api/v1/memory/notes?limit=${encodeURIComponent(String(limit))}`
  );
}

export async function createMemoryNote(
  settings: RuntimeConnectionSettings,
  payload: {
    title?: string;
    body: string;
    tags?: string[];
  }
): Promise<CreateMemoryNoteResponse> {
  return requestJson<CreateMemoryNoteResponse>(settings, "/api/v1/memory/notes", {
    method: "POST",
    body: payload,
  });
}

export async function listJobs(
  settings: RuntimeConnectionSettings,
  limit = 100
): Promise<ListJobsResponse> {
  return requestJson<ListJobsResponse>(
    settings,
    `/api/v1/jobs?limit=${encodeURIComponent(String(limit))}&include_disabled=true`
  );
}

export async function getJobsStatus(
  settings: RuntimeConnectionSettings
): Promise<JobStatusResponse> {
  return requestJson<JobStatusResponse>(settings, "/api/v1/jobs/status");
}

export async function runJobNow(
  settings: RuntimeConnectionSettings,
  jobId: string
): Promise<RunJobNowResponse> {
  return requestJson<RunJobNowResponse>(
    settings,
    `/api/v1/jobs/${encodeURIComponent(jobId)}/run`,
    {
      method: "POST",
      body: {},
    }
  );
}

export async function setJobEnabledState(
  settings: RuntimeConnectionSettings,
  jobId: string,
  enabled: boolean
): Promise<UpdateJobResponse> {
  return requestJson<UpdateJobResponse>(
    settings,
    `/api/v1/jobs/${encodeURIComponent(jobId)}/update`,
    {
      method: "POST",
      body: { enabled },
    }
  );
}

export async function listApprovals(
  settings: RuntimeConnectionSettings,
  status = "requested",
  limit = 100
): Promise<ListApprovalsResponse> {
  return requestJson<ListApprovalsResponse>(
    settings,
    `/api/v1/approvals?status=${encodeURIComponent(status)}&limit=${encodeURIComponent(String(limit))}`
  );
}

export async function resolveApproval(
  settings: RuntimeConnectionSettings,
  approvalId: string,
  decision: "approve" | "deny"
): Promise<ResolveApprovalResponse> {
  return requestJson<ResolveApprovalResponse>(
    settings,
    `/api/v1/approvals/${encodeURIComponent(approvalId)}/resolve`,
    {
      method: "POST",
      body: {
        decision,
      },
    }
  );
}

export async function listAuthProfiles(
  settings: RuntimeConnectionSettings,
  options: {
    provider?: string;
    includeDisabled?: boolean;
  } = {}
): Promise<AuthProfileResponse[]> {
  const search = new URLSearchParams();
  if (options.provider?.trim()) {
    search.set("provider", options.provider.trim());
  }
  if (options.includeDisabled !== undefined) {
    search.set("include_disabled", String(options.includeDisabled));
  }
  const suffix = search.size > 0 ? `?${search.toString()}` : "";
  const response = await requestJson<ListAuthProfilesResponse>(
    settings,
    `/api/v1/auth/profiles${suffix}`
  );
  return response.items;
}

export async function createAuthProfile(
  settings: RuntimeConnectionSettings,
  payload: {
    provider: string;
    display_name: string;
    auth_mode: string;
    risk_level: string;
    enabled?: boolean;
    kill_switch_scope?: string;
    api_base_url?: string;
    credentials_json?: Record<string, unknown>;
  }
): Promise<CreateAuthProfileResponse> {
  return requestJson<CreateAuthProfileResponse>(settings, "/api/v1/auth/profiles", {
    method: "POST",
    body: payload,
  });
}

export async function getAgentProviderProfileOrder(
  settings: RuntimeConnectionSettings,
  agentId: string,
  provider: string
): Promise<AgentProviderProfileOrderResponse> {
  return requestJson<AgentProviderProfileOrderResponse>(
    settings,
    `/api/v1/auth/agents/${encodeURIComponent(agentId)}/providers/${encodeURIComponent(provider)}/profile-order`
  );
}

export async function setAgentProviderProfileOrder(
  settings: RuntimeConnectionSettings,
  agentId: string,
  provider: string,
  profileIds: string[]
): Promise<AgentProviderProfileOrderResponse> {
  return requestJson<AgentProviderProfileOrderResponse>(
    settings,
    `/api/v1/auth/agents/${encodeURIComponent(agentId)}/providers/${encodeURIComponent(provider)}/profile-order`,
    {
      method: "POST",
      body: {
        profile_ids: profileIds,
      },
    }
  );
}

export async function startOpenAiOauth(
  settings: RuntimeConnectionSettings,
  payload: {
    display_name?: string;
    redirect_uri?: string;
    client_id?: string;
    scope?: string;
    authorize_url?: string;
    token_url?: string;
    api_base_url?: string;
  } = {}
): Promise<OpenAiOauthStartResponse> {
  return requestJson<OpenAiOauthStartResponse>(settings, "/api/v1/auth/openai/oauth/start", {
    method: "POST",
    body: payload,
  });
}

/**
 * Completes the OpenAI OAuth flow and returns `OpenAiOauthFinishResponse`.
 * Use exactly one completion mode:
 * 1) Preferred: provide `callback_url`.
 * 2) Fallback: provide both `code` and `state`.
 *
 * `oauth_session_id` is always required. `display_name` and `api_base_url` are optional
 * profile attributes.
 */
export async function finishOpenAiOauth(
  settings: RuntimeConnectionSettings,
  payload: {
    oauth_session_id: string;
    callback_url?: string;
    code?: string;
    state?: string;
    display_name?: string;
    api_base_url?: string;
  }
): Promise<OpenAiOauthFinishResponse> {
  const callbackUrl = payload.callback_url?.trim() || undefined;
  const code = payload.code?.trim() || undefined;
  const state = payload.state?.trim() || undefined;
  const hasCallbackUrl = Boolean(callbackUrl);
  const hasCode = Boolean(code);
  const hasState = Boolean(state);

  if (hasCallbackUrl && (hasCode || hasState)) {
    throw new Error("Provide either callback_url or code+state, not both.");
  }
  if (!hasCallbackUrl && !(hasCode && hasState)) {
    throw new Error("Provide callback_url, or provide both code and state.");
  }

  return requestJson<OpenAiOauthFinishResponse>(
    settings,
    "/api/v1/auth/openai/oauth/finish",
    {
      method: "POST",
      body: {
        ...payload,
        callback_url: callbackUrl,
        code,
        state,
      },
    }
  );
}

export async function ingestAnthropicSetupToken(
  settings: RuntimeConnectionSettings,
  payload: {
    display_name: string;
    setup_token: string;
    api_base_url?: string;
    enabled?: boolean;
    kill_switch_scope?: string;
  }
): Promise<AnthropicSetupTokenIngestResponse> {
  return requestJson<AnthropicSetupTokenIngestResponse>(
    settings,
    "/api/v1/auth/anthropic/setup-token/ingest",
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function listSkills(
  settings: RuntimeConnectionSettings,
  includeDisabled = true
): Promise<ListSkillsResponse> {
  return requestJson<ListSkillsResponse>(
    settings,
    `/api/v1/extensions/skills?include_disabled=${encodeURIComponent(String(includeDisabled))}`
  );
}

export async function setSkillEnabled(
  settings: RuntimeConnectionSettings,
  skillId: string,
  enabled: boolean
): Promise<UpdateSkillStateResponse> {
  return requestJson<UpdateSkillStateResponse>(
    settings,
    `/api/v1/extensions/skills/${encodeURIComponent(skillId)}/state`,
    {
      method: "POST",
      body: { enabled },
    }
  );
}

export async function listPlugins(
  settings: RuntimeConnectionSettings,
  includeDisabled = true
): Promise<ListPluginsResponse> {
  return requestJson<ListPluginsResponse>(
    settings,
    `/api/v1/extensions/plugins?include_disabled=${encodeURIComponent(String(includeDisabled))}`
  );
}

export async function listPluginRuntimeStatus(
  settings: RuntimeConnectionSettings,
  includeDisabled = true
): Promise<ListPluginRuntimeStatusResponse> {
  return requestJson<ListPluginRuntimeStatusResponse>(
    settings,
    `/api/v1/extensions/plugins/status?include_disabled=${encodeURIComponent(String(includeDisabled))}`
  );
}

export async function setPluginEnabled(
  settings: RuntimeConnectionSettings,
  plugin: PluginManifestResponse,
  enabled: boolean,
  reason?: string
): Promise<UpdatePluginResponse> {
  return requestJson<UpdatePluginResponse>(
    settings,
    `/api/v1/extensions/plugins/${encodeURIComponent(plugin.plugin_id)}/update`,
    {
      method: "POST",
      body: {
        manifest: {
          ...plugin,
          enabled,
        },
        reason: reason ?? null,
      },
    }
  );
}

export async function getChannelRuntimeStatus(
  settings: RuntimeConnectionSettings
): Promise<GetChannelRuntimeStatusResponse> {
  return requestJson<GetChannelRuntimeStatusResponse>(
    settings,
    "/api/v1/channels/runtime/status"
  );
}

export async function reconnectChannelRuntime(
  settings: RuntimeConnectionSettings,
  provider: string
): Promise<{
  status: {
    provider: string;
    healthy: boolean;
    lifecycle_state: string;
  };
}> {
  return requestJson(settings, "/api/v1/channels/runtime/reconnect", {
    method: "POST",
    body: {
      provider,
    },
  });
}

export async function listAgentMailThreads(
  settings: RuntimeConnectionSettings,
  options: {
    kind?: string;
    mailbox?: "all" | "inbox" | "outbox";
    principalId?: string;
    search?: string;
    limit?: number;
  } = {}
): Promise<ListAgentMailThreadsResponse> {
  const params = new URLSearchParams();
  if (options.kind?.trim()) {
    params.set("kind", options.kind.trim());
  }
  if (options.mailbox && options.mailbox !== "all") {
    params.set("mailbox", options.mailbox);
  }
  if (options.principalId?.trim()) {
    params.set("principal_id", options.principalId.trim());
  }
  if (options.search?.trim()) {
    params.set("search", options.search.trim());
  }
  if (options.limit !== undefined) {
    params.set("limit", String(options.limit));
  }
  const suffix = params.size > 0 ? `?${params.toString()}` : "";
  return requestJson<ListAgentMailThreadsResponse>(
    settings,
    `/api/v1/agent-mail/threads${suffix}`
  );
}

export async function createAgentMailThread(
  settings: RuntimeConnectionSettings,
  payload: {
    kind?: "direct" | "room";
    subject: string;
    participants: string[];
  }
): Promise<CreateAgentMailThreadResponse> {
  return requestJson<CreateAgentMailThreadResponse>(settings, "/api/v1/agent-mail/threads", {
    method: "POST",
    body: payload,
  });
}

export async function getAgentMailThread(
  settings: RuntimeConnectionSettings,
  threadId: string
): Promise<AgentMailThreadDetailResponse> {
  return requestJson<AgentMailThreadDetailResponse>(
    settings,
    `/api/v1/agent-mail/threads/${encodeURIComponent(threadId)}`
  );
}

export async function listAgentMailMessages(
  settings: RuntimeConnectionSettings,
  threadId: string,
  limit = 300
): Promise<ListAgentMailMessagesResponse> {
  return requestJson<ListAgentMailMessagesResponse>(
    settings,
    `/api/v1/agent-mail/threads/${encodeURIComponent(threadId)}/messages?limit=${encodeURIComponent(String(limit))}`
  );
}

export async function sendAgentMailMessage(
  settings: RuntimeConnectionSettings,
  threadId: string,
  payload: {
    sender_principal?: string;
    sender_kind?: string;
    body_text: string;
    metadata_json?: Record<string, unknown>;
    recipients: string[];
  }
): Promise<SendAgentMailMessageResponse> {
  return requestJson<SendAgentMailMessageResponse>(
    settings,
    `/api/v1/agent-mail/threads/${encodeURIComponent(threadId)}/messages`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function ackAgentMailMessage(
  settings: RuntimeConnectionSettings,
  messageId: string,
  recipientPrincipal?: string
): Promise<AckAgentMailMessageResponse> {
  return requestJson<AckAgentMailMessageResponse>(
    settings,
    `/api/v1/agent-mail/messages/${encodeURIComponent(messageId)}/ack`,
    {
      method: "POST",
      body: {
        recipient_principal: recipientPrincipal,
      },
    }
  );
}

export async function uploadAgentMailAttachment(
  settings: RuntimeConnectionSettings,
  messageId: string,
  payload: {
    filename: string;
    mime: string;
    content_base64: string;
  }
): Promise<UploadAgentMailAttachmentResponse> {
  return requestJson<UploadAgentMailAttachmentResponse>(
    settings,
    `/api/v1/agent-mail/messages/${encodeURIComponent(messageId)}/attachments/upload`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function fetchAgentMailAttachmentBlob(
  settings: RuntimeConnectionSettings,
  messageId: string,
  attachmentId: string
): Promise<Blob> {
  const token = await getGatewayToken();
  if (!token) {
    throw new Error("Gateway token is not configured.");
  }
  const response = await fetchWithTimeout(
    resolveApiUrl(
      settings.gateway_url,
      `/api/v1/agent-mail/messages/${encodeURIComponent(messageId)}/attachments/${encodeURIComponent(attachmentId)}`
    ),
    {
      method: "GET",
      headers: {
        Authorization: `Bearer ${token}`,
      },
    }
  );
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${response.status} ${response.statusText}: ${text}`);
  }
  return response.blob();
}

export async function listAgentMailFileLeases(
  settings: RuntimeConnectionSettings,
  options: {
    holderPrincipal?: string;
    includeReleased?: boolean;
  } = {}
): Promise<AgentMailFileLeaseResponse[]> {
  // Intentionally returns the unwrapped lease array for direct list rendering in UI panels.
  const params = new URLSearchParams();
  if (options.holderPrincipal?.trim()) {
    params.set("holder_principal", options.holderPrincipal.trim());
  }
  if (options.includeReleased !== undefined) {
    params.set("include_released", String(options.includeReleased));
  }
  const suffix = params.size > 0 ? `?${params.toString()}` : "";
  const response = await requestJson<ListAgentMailFileLeasesResponse>(
    settings,
    `/api/v1/agent-mail/leases${suffix}`
  );
  return response.items;
}

export async function createAgentMailFileLease(
  settings: RuntimeConnectionSettings,
  payload: {
    holder_principal?: string;
    glob_pattern: string;
    exclusive: boolean;
    ttl_ms: number;
    note?: string;
  }
): Promise<CreateAgentMailFileLeaseResponse> {
  return requestJson<CreateAgentMailFileLeaseResponse>(
    settings,
    "/api/v1/agent-mail/leases",
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function releaseAgentMailFileLease(
  settings: RuntimeConnectionSettings,
  leaseId: string,
  holderPrincipal?: string
): Promise<ReleaseAgentMailFileLeaseResponse> {
  return requestJson<ReleaseAgentMailFileLeaseResponse>(
    settings,
    `/api/v1/agent-mail/leases/${encodeURIComponent(leaseId)}/release`,
    {
      method: "POST",
      body: {
        holder_principal: holderPrincipal,
      },
    }
  );
}

export async function createBoardCard(
  settings: RuntimeConnectionSettings,
  boardId: string,
  payload: {
    column_id: string;
    title: string;
    owner_kind?: string;
    owner_agent_id?: string;
    owner_human_id?: string;
  }
): Promise<UpdateBoardCardResponse> {
  return requestJson<UpdateBoardCardResponse>(
    settings,
    `/api/v1/boards/${encodeURIComponent(boardId)}/cards/create`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function updateBoardCard(
  settings: RuntimeConnectionSettings,
  boardId: string,
  cardId: string,
  payload: {
    title?: string;
    description?: string | null;
    owner_kind?: string;
    owner_agent_id?: string | null;
    owner_human_id?: string | null;
    due_at?: number | null;
    tags?: string[] | null;
    script_markdown?: string | null;
  }
): Promise<UpdateBoardCardResponse> {
  return requestJson<UpdateBoardCardResponse>(
    settings,
    `/api/v1/boards/${encodeURIComponent(boardId)}/cards/${encodeURIComponent(cardId)}/update`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function moveBoardCard(
  settings: RuntimeConnectionSettings,
  boardId: string,
  cardId: string,
  payload: {
    column_id: string;
    before_card_id?: string;
  }
): Promise<MoveBoardCardResponse> {
  return requestJson<MoveBoardCardResponse>(
    settings,
    `/api/v1/boards/${encodeURIComponent(boardId)}/cards/${encodeURIComponent(cardId)}/move`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function runBoardCard(
  settings: RuntimeConnectionSettings,
  boardId: string,
  cardId: string
): Promise<RunBoardCardResponse> {
  return requestJson<RunBoardCardResponse>(
    settings,
    `/api/v1/boards/${encodeURIComponent(boardId)}/cards/${encodeURIComponent(cardId)}/run`,
    {
      method: "POST",
      body: {},
    }
  );
}

export async function uploadBoardCardAsset(
  settings: RuntimeConnectionSettings,
  boardId: string,
  cardId: string,
  payload: {
    filename: string;
    mime: string;
    content_base64: string;
  }
): Promise<UploadBoardCardAssetResponse> {
  return requestJson<UploadBoardCardAssetResponse>(
    settings,
    `/api/v1/boards/${encodeURIComponent(boardId)}/cards/${encodeURIComponent(cardId)}/assets/upload`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function fetchBoardCardAssetBlob(
  settings: RuntimeConnectionSettings,
  boardId: string,
  cardId: string,
  cardAssetId: string
): Promise<Blob> {
  const token = await getGatewayToken();
  if (!token) {
    throw new Error("Gateway token is not configured.");
  }

  const response = await fetchWithTimeout(
    resolveApiUrl(
      settings.gateway_url,
      `/api/v1/boards/${encodeURIComponent(boardId)}/cards/${encodeURIComponent(cardId)}/assets/${encodeURIComponent(cardAssetId)}`
    ),
    {
      method: "GET",
      headers: {
        Authorization: `Bearer ${token}`,
      },
    }
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${response.status} ${response.statusText}: ${text}`);
  }

  return response.blob();
}

export function websocketUrlFromGateway(
  settings: RuntimeConnectionSettings,
  token: string
): string {
  const base = new URL(normalizeGatewayUrl(settings.gateway_url));
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = "/api/v1/ws";
  base.searchParams.set("token", token);
  return base.toString();
}
