import { getGatewayToken } from "./runtime";
import { API_REQUEST_TIMEOUT_MS, DEFAULT_GATEWAY_URL } from "../constants";
import type {
  AckAgentMailMessageResponse,
  AgentMemoryAtomDetailPayload,
  AgentMemoryBindingRequest,
  AgentMemoryCardDetailPayload,
  AgentMemoryCardsPayload,
  AgentMemoryCitationPayload,
  AgentMemoryDecisionReasonsPayload,
  AgentMemoryEpisodesPayload,
  AgentMemoryGraphMapPayload,
  AgentMemoryGraphNeighborsPayload,
  AgentMemoryJsonPayloadResponse,
  AgentMemoryRuntimeHealthPayload,
  AgentMemoryTelemetrySummaryPayload,
  AgentMemoryTelemetryTurnsPayload,
  AgentMemoryTurnWhyPayload,
  AgentMailFileLeaseResponse,
  AgentMailThreadDetailResponse,
  AgentProviderProfileOrderResponse,
  AnthropicSetupTokenIngestResponse,
  AnthropicSetupTokenValidateResponse,
  AuthProfileResponse,
  BoardDetail,
  BoardDetailResponse,
  CreateBootstrapPresetResponse,
  CreateGoalResponse,
  CreateAgentResponse,
  CreateMessageResponse,
  CreateProjectResponse,
  CreateRunResponse,
  CreateSessionResponse,
  CreateTaskResponse,
  CreateAgentMailFileLeaseResponse,
  CreateAgentMailThreadResponse,
  CreateAuthProfileResponse,
  CreateMemoryNoteResponse,
  DescribeConnectorToolResponse,
  ExportBootstrapPresetResponse,
  GetAgentMemoryStatusResponse,
  GetChannelRuntimeStatusResponse,
  GetConnectorHealthResponse,
  GetConnectorResponse,
  HealthResponse,
  ImportConnectorRequest,
  ImportConnectorResponse,
  JobStatusResponse,
  ListAgentMailFileLeasesResponse,
  ListAgentMailMessagesResponse,
  ListAgentMailThreadsResponse,
  ListBootstrapPresetsResponse,
  ListConnectorCatalogRequest,
  ListConnectorCatalogResponse,
  ListConnectorInteractionsResponse,
  ListConnectorsRequest,
  ListConnectorsResponse,
  ListMemoryNotesResponse,
  ListMessagesResponse,
  ListAuthProfilesResponse,
  ListApprovalsResponse,
  ListJobsResponse,
  ListPluginRuntimeStatusResponse,
  ListPluginsResponse,
  ListProviderCapabilitiesResponse,
  ListProviderModelsResponse,
  ListRunbooksResponse,
  ListSkillsResponse,
  ListAgentsResponse,
  ListBoardsResponse,
  ListGoalsResponse,
  ListProjectsResponse,
  ListTasksResponse,
  MissionControlCalendarWeekResponse,
  MissionControlFocusResponse,
  MissionControlUsageResponse,
  MoveBoardCardResponse,
  OpenAiOauthFinishResponse,
  OpenAiOauthStartResponse,
  PluginManifestResponse,
  ReleaseAgentMailFileLeaseResponse,
  ResumeConnectorInteractionRequest,
  ResumeConnectorInteractionResponse,
  RemoveAgentResponse,
  RevokeAuthProfileResponse,
  RollbackConnectorVersionRequest,
  RollbackConnectorVersionResponse,
  ResolveApprovalResponse,
  RuntimeConnectionSettings,
  RunJobNowResponse,
  RunBoardCardResponse,
  RunbookDetailResponse,
  RunConnectorConversionRequest,
  RunConnectorConversionResponse,
  SendAgentMailMessageResponse,
  SetConnectorAssignmentRequest,
  SetConnectorAssignmentResponse,
  SetConnectorStateRequest,
  SetConnectorStateResponse,
  StatusResponse,
  StrategySummaryResponse,
  TaskLinkMutationResponse,
  UnpublishConnectorToolsRequest,
  UnpublishConnectorToolsResponse,
  UpsertConnectorAuthBindingRequest,
  UpsertConnectorAuthBindingResponse,
  UpdateBootstrapPresetResponse,
  UpdateGoalResponse,
  UpdatePluginResponse,
  UpdateSkillStateResponse,
  UpdateJobResponse,
  UpdateBoardCardResponse,
  UpdateAgentResponse,
  UpdateAgentMemoryBindingRequest,
  UpdateProjectResponse,
  UpdateTaskResponse,
  UploadAgentMailAttachmentResponse,
  UploadBoardCardAssetResponse,
  ImportBootstrapPresetResponse,
  PublishConnectorToolsRequest,
  PublishConnectorToolsResponse,
} from "../types";

export type GatewayApiErrorKind = "config" | "timeout" | "network" | "http";

interface GatewayApiErrorOptions {
  kind: GatewayApiErrorKind;
  path?: string;
  status?: number;
  statusText?: string;
  responseBody?: string | null;
  cause?: unknown;
}

export class GatewayApiError extends Error {
  readonly kind: GatewayApiErrorKind;
  readonly path: string | null;
  readonly status: number | null;
  readonly statusText: string | null;
  readonly responseBody: string | null;

  constructor(message: string, options: GatewayApiErrorOptions) {
    super(message);
    this.name = "GatewayApiError";
    this.kind = options.kind;
    this.path = options.path ?? null;
    this.status = options.status ?? null;
    this.statusText = options.statusText ?? null;
    this.responseBody = options.responseBody ?? null;
    if (options.cause !== undefined) {
      this.cause = options.cause;
    }
  }
}

function createGatewayApiError(
  message: string,
  options: GatewayApiErrorOptions
): GatewayApiError {
  return new GatewayApiError(message, options);
}

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

function appendQuery(
  path: string,
  entries: Array<[string, string | number | boolean | null | undefined]>
): string {
  const params = new URLSearchParams();
  for (const [key, value] of entries) {
    if (value === undefined || value === null || value === "") {
      continue;
    }
    params.set(key, String(value));
  }
  const query = params.toString();
  return query ? `${path}?${query}` : path;
}

interface ApiRequestOptions {
  method?: "GET" | "POST";
  body?: unknown;
}

async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs = API_REQUEST_TIMEOUT_MS,
  path = url
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
      throw createGatewayApiError(`Gateway request timed out after ${timeoutMs}ms.`, {
        kind: "timeout",
        path,
        cause: error,
      });
    }
    if (error instanceof GatewayApiError) {
      throw error;
    }
    throw createGatewayApiError("Gateway request failed.", {
      kind: "network",
      path,
      cause: error,
    });
  } finally {
    globalThis.clearTimeout(timeoutId);
  }
}

async function buildHttpError(path: string, response: Response): Promise<GatewayApiError> {
  const responseBody = await response.text();
  const message = `${response.status} ${response.statusText}`.trim() || String(response.status);
  return createGatewayApiError(message, {
    kind: "http",
    path,
    status: response.status,
    statusText: response.statusText,
    responseBody,
  });
}

async function requestRaw(
  settings: RuntimeConnectionSettings,
  path: string,
  options: ApiRequestOptions = {}
): Promise<Response> {
  const token = await getGatewayToken();
  if (!token) {
    throw createGatewayApiError("Gateway token is not configured.", {
      kind: "config",
      path,
    });
  }

  return fetchWithTimeout(
    resolveApiUrl(settings.gateway_url, path),
    {
      method: options.method ?? "GET",
      headers: {
        Authorization: `Bearer ${token}`,
        ...(options.body === undefined ? {} : { "Content-Type": "application/json" }),
      },
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    },
    API_REQUEST_TIMEOUT_MS,
    path
  );
}

async function requestJson<T>(
  settings: RuntimeConnectionSettings,
  path: string,
  options: ApiRequestOptions = {}
): Promise<T> {
  const response = await requestRaw(settings, path, options);

  if (!response.ok) {
    throw await buildHttpError(path, response);
  }

  return (await response.json()) as T;
}

async function requestBlob(
  settings: RuntimeConnectionSettings,
  path: string
): Promise<Blob> {
  const response = await requestRaw(settings, path);
  if (!response.ok) {
    throw await buildHttpError(path, response);
  }
  return response.blob();
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

export async function listConnectorCatalog(
  settings: RuntimeConnectionSettings,
  query?: ListConnectorCatalogRequest
): Promise<ListConnectorCatalogResponse> {
  return requestJson<ListConnectorCatalogResponse>(
    settings,
    appendQuery("/api/v1/connectors/catalog", [
      ["source_kind", query?.source_kind],
      ["query", query?.query],
    ])
  );
}

export async function listConnectors(
  settings: RuntimeConnectionSettings,
  query?: ListConnectorsRequest
): Promise<ListConnectorsResponse> {
  return requestJson<ListConnectorsResponse>(
    settings,
    appendQuery("/api/v1/connectors", [
      ["source_kind", query?.source_kind],
      ["status", query?.status],
      ["trust_state", query?.trust_state],
      ["query", query?.query],
      ["include_disabled", query?.include_disabled],
    ])
  );
}

export async function getConnector(
  settings: RuntimeConnectionSettings,
  connectorId: string
): Promise<GetConnectorResponse> {
  return requestJson<GetConnectorResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}`
  );
}

export async function importConnector(
  settings: RuntimeConnectionSettings,
  payload: ImportConnectorRequest
): Promise<ImportConnectorResponse> {
  return requestJson<ImportConnectorResponse>(settings, "/api/v1/connectors/import", {
    method: "POST",
    body: payload,
  });
}

export async function runConnectorConversion(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  payload: RunConnectorConversionRequest
): Promise<RunConnectorConversionResponse> {
  return requestJson<RunConnectorConversionResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/convert`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function publishConnectorTools(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  payload: PublishConnectorToolsRequest
): Promise<PublishConnectorToolsResponse> {
  return requestJson<PublishConnectorToolsResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/publish`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function unpublishConnectorTools(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  payload: UnpublishConnectorToolsRequest
): Promise<UnpublishConnectorToolsResponse> {
  return requestJson<UnpublishConnectorToolsResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/unpublish`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function rollbackConnectorVersion(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  payload: RollbackConnectorVersionRequest
): Promise<RollbackConnectorVersionResponse> {
  return requestJson<RollbackConnectorVersionResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/rollback`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function setConnectorState(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  payload: SetConnectorStateRequest
): Promise<SetConnectorStateResponse> {
  return requestJson<SetConnectorStateResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/state`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function setConnectorAssignment(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  payload: SetConnectorAssignmentRequest
): Promise<SetConnectorAssignmentResponse> {
  return requestJson<SetConnectorAssignmentResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/assignments`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function upsertConnectorAuthBinding(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  payload: UpsertConnectorAuthBindingRequest
): Promise<UpsertConnectorAuthBindingResponse> {
  return requestJson<UpsertConnectorAuthBindingResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/auth-bindings`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function listConnectorInteractions(
  settings: RuntimeConnectionSettings
): Promise<ListConnectorInteractionsResponse> {
  return requestJson<ListConnectorInteractionsResponse>(
    settings,
    "/api/v1/connectors/interactions"
  );
}

export async function resumeConnectorInteraction(
  settings: RuntimeConnectionSettings,
  interactionId: string,
  payload: ResumeConnectorInteractionRequest = {}
): Promise<ResumeConnectorInteractionResponse> {
  return requestJson<ResumeConnectorInteractionResponse>(
    settings,
    `/api/v1/connectors/interactions/${encodeURIComponent(interactionId)}/resume`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function getConnectorHealth(
  settings: RuntimeConnectionSettings,
  connectorId: string
): Promise<GetConnectorHealthResponse["health"]> {
  const response = await requestJson<GetConnectorHealthResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/health`
  );
  return response.health;
}

export async function describeConnectorTool(
  settings: RuntimeConnectionSettings,
  connectorId: string,
  publishedToolId: string
): Promise<DescribeConnectorToolResponse> {
  return requestJson<DescribeConnectorToolResponse>(
    settings,
    `/api/v1/connectors/${encodeURIComponent(connectorId)}/tools/${encodeURIComponent(publishedToolId)}`
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
    reports_to_agent_id?: string | null;
    role_label?: string | null;
    memory_binding?: AgentMemoryBindingRequest | null;
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
    reports_to_agent_id?: string | null;
    role_label?: string | null;
    memory_binding?: UpdateAgentMemoryBindingRequest | null;
  }
): Promise<UpdateAgentResponse> {
  return requestJson<UpdateAgentResponse>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function removeAgent(
  settings: RuntimeConnectionSettings,
  agentId: string
): Promise<RemoveAgentResponse> {
  return requestJson<RemoveAgentResponse>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}/remove`,
    {
      method: "POST",
    }
  );
}

export async function getAgentMemoryStatus(
  settings: RuntimeConnectionSettings,
  agentId: string
): Promise<GetAgentMemoryStatusResponse["status"]> {
  const response = await requestJson<GetAgentMemoryStatusResponse>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}/memory/status`
  );
  return response.status;
}

export async function listAgentMemoryCards(
  settings: RuntimeConnectionSettings,
  agentId: string,
  query?: {
    kind?: string;
    status?: string;
    contradiction?: string;
    q?: string;
    offset?: number;
    limit?: number;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryCardsPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryCardsPayload>>(
    settings,
    appendQuery(`/api/v1/agents/${encodeURIComponent(agentId)}/memory/cards`, [
      ["kind", query?.kind],
      ["status", query?.status],
      ["contradiction", query?.contradiction],
      ["q", query?.q],
      ["offset", query?.offset],
      ["limit", query?.limit],
    ])
  );
}

export async function getAgentMemoryCard(
  settings: RuntimeConnectionSettings,
  agentId: string,
  cardId: string
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryCardDetailPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryCardDetailPayload>>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}/memory/cards/${encodeURIComponent(cardId)}`
  );
}

export async function getAgentMemoryAtom(
  settings: RuntimeConnectionSettings,
  agentId: string,
  atomId: string
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryAtomDetailPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryAtomDetailPayload>>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}/memory/atom/${encodeURIComponent(atomId)}`
  );
}

export async function listAgentMemoryEpisodes(
  settings: RuntimeConnectionSettings,
  agentId: string,
  query?: {
    status?: string;
    q?: string;
    run_id?: string;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryEpisodesPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryEpisodesPayload>>(
    settings,
    appendQuery(`/api/v1/agents/${encodeURIComponent(agentId)}/memory/episodes`, [
      ["status", query?.status],
      ["q", query?.q],
      ["run_id", query?.run_id],
    ])
  );
}

export async function getAgentMemoryGraphMap(
  settings: RuntimeConnectionSettings,
  agentId: string,
  query?: {
    status?: string;
    q?: string;
    limit?: number;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryGraphMapPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryGraphMapPayload>>(
    settings,
    appendQuery(`/api/v1/agents/${encodeURIComponent(agentId)}/memory/graph-map`, [
      ["status", query?.status],
      ["q", query?.q],
      ["limit", query?.limit],
    ])
  );
}

export async function getAgentMemoryGraphNeighbors(
  settings: RuntimeConnectionSettings,
  agentId: string,
  query: {
    atom_id: string;
    depth?: number;
    node_limit?: number;
    link_limit?: number;
    include_root_detail?: boolean;
    include_shared_language?: boolean;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryGraphNeighborsPayload>> {
  if (!query.atom_id.trim()) {
    throw new Error("atom_id is required");
  }
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryGraphNeighborsPayload>>(
    settings,
    appendQuery(`/api/v1/agents/${encodeURIComponent(agentId)}/memory/graph/neighbors`, [
      ["atom_id", query.atom_id],
      ["depth", query.depth],
      ["node_limit", query.node_limit],
      ["link_limit", query.link_limit],
      ["include_root_detail", query.include_root_detail],
      ["include_shared_language", query.include_shared_language],
    ])
  );
}

export async function getAgentMemoryTurnWhy(
  settings: RuntimeConnectionSettings,
  agentId: string,
  turnId: string,
  query?: {
    citations?: boolean;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryTurnWhyPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryTurnWhyPayload>>(
    settings,
    appendQuery(
      `/api/v1/agents/${encodeURIComponent(agentId)}/memory/turns/${encodeURIComponent(turnId)}/why`,
      [["citations", query?.citations]]
    )
  );
}

export async function getAgentMemoryCitation(
  settings: RuntimeConnectionSettings,
  agentId: string,
  citationToken: string,
  query?: {
    context_window?: number;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryCitationPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryCitationPayload>>(
    settings,
    appendQuery(
      `/api/v1/agents/${encodeURIComponent(agentId)}/memory/citations/${encodeURIComponent(citationToken)}`,
      [["context_window", query?.context_window]]
    )
  );
}

export async function getAgentMemoryRuntimeHealth(
  settings: RuntimeConnectionSettings,
  agentId: string
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryRuntimeHealthPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryRuntimeHealthPayload>>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}/memory/runtime/health`
  );
}

export async function getAgentMemoryTelemetrySummary(
  settings: RuntimeConnectionSettings,
  agentId: string,
  query?: {
    limit?: number;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryTelemetrySummaryPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryTelemetrySummaryPayload>>(
    settings,
    appendQuery(
      `/api/v1/agents/${encodeURIComponent(agentId)}/memory/runtime/telemetry/summary`,
      [["limit", query?.limit]]
    )
  );
}

export async function getAgentMemoryTelemetryTurns(
  settings: RuntimeConnectionSettings,
  agentId: string,
  query?: {
    limit?: number;
  }
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryTelemetryTurnsPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryTelemetryTurnsPayload>>(
    settings,
    appendQuery(
      `/api/v1/agents/${encodeURIComponent(agentId)}/memory/runtime/telemetry/turns`,
      [["limit", query?.limit]]
    )
  );
}

export async function getAgentMemoryDecisionReasons(
  settings: RuntimeConnectionSettings,
  agentId: string
): Promise<AgentMemoryJsonPayloadResponse<AgentMemoryDecisionReasonsPayload>> {
  return requestJson<AgentMemoryJsonPayloadResponse<AgentMemoryDecisionReasonsPayload>>(
    settings,
    `/api/v1/agents/${encodeURIComponent(agentId)}/memory/runtime/decision-reasons`
  );
}

export async function listGoals(
  settings: RuntimeConnectionSettings,
  query?: {
    limit?: number;
    cursor?: string;
    sort?: string;
    status?: string;
    owner_agent_id?: string;
    query?: string;
  }
): Promise<ListGoalsResponse> {
  return requestJson<ListGoalsResponse>(
    settings,
    appendQuery("/api/v1/goals", [
      ["limit", query?.limit],
      ["cursor", query?.cursor],
      ["sort", query?.sort],
      ["status", query?.status],
      ["owner_agent_id", query?.owner_agent_id],
      ["query", query?.query],
    ])
  );
}

export async function createGoal(
  settings: RuntimeConnectionSettings,
  payload: {
    slug: string;
    title: string;
    summary?: string | null;
    status?: string;
    owner_agent_id?: string | null;
    target_date?: number | null;
  }
): Promise<CreateGoalResponse> {
  return requestJson<CreateGoalResponse>(settings, "/api/v1/goals", {
    method: "POST",
    body: payload,
  });
}

export async function updateGoal(
  settings: RuntimeConnectionSettings,
  goalId: string,
  payload: {
    slug?: string;
    title?: string;
    summary?: string;
    status?: string;
    owner_agent_id?: string | null;
    target_date?: number | null;
  }
): Promise<UpdateGoalResponse> {
  return requestJson<UpdateGoalResponse>(
    settings,
    `/api/v1/goals/${encodeURIComponent(goalId)}`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function listProjects(
  settings: RuntimeConnectionSettings,
  query?: {
    limit?: number;
    cursor?: string;
    sort?: string;
    status?: string;
    owner_agent_id?: string;
    query?: string;
    goal_id?: string;
  }
): Promise<ListProjectsResponse> {
  return requestJson<ListProjectsResponse>(
    settings,
    appendQuery("/api/v1/projects", [
      ["limit", query?.limit],
      ["cursor", query?.cursor],
      ["sort", query?.sort],
      ["status", query?.status],
      ["owner_agent_id", query?.owner_agent_id],
      ["query", query?.query],
      ["goal_id", query?.goal_id],
    ])
  );
}

export async function createProject(
  settings: RuntimeConnectionSettings,
  payload: {
    goal_id: string;
    slug: string;
    name: string;
    summary?: string | null;
    status?: string;
    owner_agent_id?: string | null;
    workspace_root?: string | null;
    budget_month_usd?: number | null;
  }
): Promise<CreateProjectResponse> {
  return requestJson<CreateProjectResponse>(settings, "/api/v1/projects", {
    method: "POST",
    body: payload,
  });
}

export async function updateProject(
  settings: RuntimeConnectionSettings,
  projectId: string,
  payload: {
    goal_id?: string;
    slug?: string;
    name?: string;
    summary?: string;
    status?: string;
    owner_agent_id?: string | null;
    workspace_root?: string | null;
    budget_month_usd?: number | null;
  }
): Promise<UpdateProjectResponse> {
  return requestJson<UpdateProjectResponse>(
    settings,
    `/api/v1/projects/${encodeURIComponent(projectId)}`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function listTasks(
  settings: RuntimeConnectionSettings,
  query?: {
    limit?: number;
    cursor?: string;
    sort?: string;
    status?: string;
    owner_agent_id?: string;
    query?: string;
    goal_id?: string;
    project_id?: string;
    stale?: boolean;
    blocked?: boolean;
    unassigned?: boolean;
    hierarchy_root_agent_id?: string;
    hierarchy_scope?: string;
  }
): Promise<ListTasksResponse> {
  return requestJson<ListTasksResponse>(
    settings,
    appendQuery("/api/v1/tasks", [
      ["limit", query?.limit],
      ["cursor", query?.cursor],
      ["sort", query?.sort],
      ["status", query?.status],
      ["owner_agent_id", query?.owner_agent_id],
      ["query", query?.query],
      ["goal_id", query?.goal_id],
      ["project_id", query?.project_id],
      ["stale", query?.stale],
      ["blocked", query?.blocked],
      ["unassigned", query?.unassigned],
      ["hierarchy_root_agent_id", query?.hierarchy_root_agent_id],
      ["hierarchy_scope", query?.hierarchy_scope],
    ])
  );
}

export async function createTask(
  settings: RuntimeConnectionSettings,
  payload: {
    project_id: string;
    parent_task_id?: string | null;
    title: string;
    detail?: string | null;
    status?: string;
    priority?: string;
    owner_agent_id?: string | null;
    due_at?: number | null;
    blocked_reason?: string | null;
  }
): Promise<CreateTaskResponse> {
  return requestJson<CreateTaskResponse>(settings, "/api/v1/tasks", {
    method: "POST",
    body: payload,
  });
}

export async function updateTask(
  settings: RuntimeConnectionSettings,
  taskId: string,
  payload: {
    project_id?: string;
    parent_task_id?: string | null;
    title?: string;
    detail?: string;
    status?: string;
    priority?: string;
    owner_agent_id?: string | null;
    due_at?: number | null;
    blocked_reason?: string | null;
  }
): Promise<UpdateTaskResponse> {
  return requestJson<UpdateTaskResponse>(
    settings,
    `/api/v1/tasks/${encodeURIComponent(taskId)}`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function linkTaskBoardCard(
  settings: RuntimeConnectionSettings,
  taskId: string,
  payload: {
    board_card_id: string;
    force_reassign?: boolean;
  }
): Promise<TaskLinkMutationResponse> {
  return requestJson<TaskLinkMutationResponse>(
    settings,
    `/api/v1/tasks/${encodeURIComponent(taskId)}/links/board-card`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function linkTaskJob(
  settings: RuntimeConnectionSettings,
  taskId: string,
  payload: {
    job_id: string;
    force_reassign?: boolean;
  }
): Promise<TaskLinkMutationResponse> {
  return requestJson<TaskLinkMutationResponse>(
    settings,
    `/api/v1/tasks/${encodeURIComponent(taskId)}/links/job`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function clearTaskLinks(
  settings: RuntimeConnectionSettings,
  taskId: string,
  payload: {
    clear_board_card?: boolean;
    clear_job?: boolean;
  }
): Promise<TaskLinkMutationResponse> {
  return requestJson<TaskLinkMutationResponse>(
    settings,
    `/api/v1/tasks/${encodeURIComponent(taskId)}/links/clear`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function getStrategySummary(
  settings: RuntimeConnectionSettings,
  query?: {
    timezone?: string;
    tz_offset_minutes?: number;
  }
): Promise<StrategySummaryResponse> {
  return requestJson<StrategySummaryResponse>(
    settings,
    appendQuery("/api/v1/mission-control/strategy/summary", [
      ["timezone", query?.timezone],
      ["tz_offset_minutes", query?.tz_offset_minutes],
    ])
  );
}

export async function listRunbooks(
  settings: RuntimeConnectionSettings,
  query?: {
    kind?: string;
    status?: string;
    owner_agent_id?: string;
    query?: string;
    linked_task_id?: string;
    linked_project_id?: string;
    linked_goal_id?: string;
    limit?: number;
    cursor?: string;
  }
): Promise<ListRunbooksResponse> {
  return requestJson<ListRunbooksResponse>(
    settings,
    appendQuery("/api/v1/mission-control/runbooks", [
      ["kind", query?.kind],
      ["status", query?.status],
      ["owner_agent_id", query?.owner_agent_id],
      ["query", query?.query],
      ["linked_task_id", query?.linked_task_id],
      ["linked_project_id", query?.linked_project_id],
      ["linked_goal_id", query?.linked_goal_id],
      ["limit", query?.limit],
      ["cursor", query?.cursor],
    ])
  );
}

export async function getRunbookDetail(
  settings: RuntimeConnectionSettings,
  runbookKind: string,
  anchorId: string
): Promise<RunbookDetailResponse> {
  return requestJson<RunbookDetailResponse>(
    settings,
    `/api/v1/mission-control/runbooks/${encodeURIComponent(runbookKind)}/${encodeURIComponent(anchorId)}`
  );
}

export async function listBootstrapPresets(
  settings: RuntimeConnectionSettings,
  query?: {
    limit?: number;
    cursor?: string;
    sort?: string;
    query?: string;
  }
): Promise<ListBootstrapPresetsResponse> {
  return requestJson<ListBootstrapPresetsResponse>(
    settings,
    appendQuery("/api/v1/bootstrap-presets", [
      ["limit", query?.limit],
      ["cursor", query?.cursor],
      ["sort", query?.sort],
      ["query", query?.query],
    ])
  );
}

export async function createBootstrapPreset(
  settings: RuntimeConnectionSettings,
  payload: {
    preset_key: string;
    display_name: string;
    description?: string | null;
    role_label: string;
    provider_path: string;
    default_model_provider?: string | null;
    default_model_id?: string | null;
    default_tool_profile?: string | null;
    default_workspace_root?: string | null;
    default_reports_to_agent_id?: string | null;
    setup_notes?: string | null;
  }
): Promise<CreateBootstrapPresetResponse> {
  return requestJson<CreateBootstrapPresetResponse>(
    settings,
    "/api/v1/bootstrap-presets",
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function updateBootstrapPreset(
  settings: RuntimeConnectionSettings,
  presetKey: string,
  payload: {
    display_name?: string;
    description?: string | null;
    role_label?: string;
    provider_path?: string;
    default_model_provider?: string | null;
    default_model_id?: string | null;
    default_tool_profile?: string | null;
    default_workspace_root?: string | null;
    default_reports_to_agent_id?: string | null;
    setup_notes?: string | null;
  }
): Promise<UpdateBootstrapPresetResponse> {
  return requestJson<UpdateBootstrapPresetResponse>(
    settings,
    `/api/v1/bootstrap-presets/${encodeURIComponent(presetKey)}`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function exportBootstrapPreset(
  settings: RuntimeConnectionSettings,
  presetKey: string
): Promise<ExportBootstrapPresetResponse> {
  return requestJson<ExportBootstrapPresetResponse>(
    settings,
    `/api/v1/bootstrap-presets/${encodeURIComponent(presetKey)}/export`
  );
}

export async function importBootstrapPreset(
  settings: RuntimeConnectionSettings,
  payload: {
    payload: Record<string, unknown>;
    overwrite?: boolean;
  }
): Promise<ImportBootstrapPresetResponse> {
  return requestJson<ImportBootstrapPresetResponse>(
    settings,
    "/api/v1/bootstrap-presets/import",
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function createSession(
  settings: RuntimeConnectionSettings,
  payload: {
    session_key?: string;
    agent_id?: string;
    title?: string;
  }
): Promise<CreateSessionResponse> {
  return requestJson<CreateSessionResponse>(settings, "/api/v1/sessions", {
    method: "POST",
    body: payload,
  });
}

export async function createSessionMessage(
  settings: RuntimeConnectionSettings,
  sessionId: string,
  payload: {
    role: "system" | "user" | "assistant" | "tool";
    content_text: string;
    content_format?: string;
    source_channel?: string;
  }
): Promise<CreateMessageResponse> {
  return requestJson<CreateMessageResponse>(
    settings,
    `/api/v1/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      method: "POST",
      body: payload,
    }
  );
}

export async function listSessionMessages(
  settings: RuntimeConnectionSettings,
  sessionId: string,
  limit = 200
): Promise<ListMessagesResponse> {
  return requestJson<ListMessagesResponse>(
    settings,
    `/api/v1/sessions/${encodeURIComponent(sessionId)}/messages?limit=${encodeURIComponent(String(limit))}`
  );
}

export async function createSessionRun(
  settings: RuntimeConnectionSettings,
  sessionId: string,
  payload: {
    model_provider: string;
    model_id: string;
    auth_profile_id?: string;
  }
): Promise<CreateRunResponse> {
  return requestJson<CreateRunResponse>(
    settings,
    `/api/v1/sessions/${encodeURIComponent(sessionId)}/runs`,
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

export async function getMissionControlUsage(
  settings: RuntimeConnectionSettings,
  query: {
    window: "today" | "week";
    timezone?: string;
    tz_offset_minutes?: number;
    window_start_ms?: number;
    window_end_ms?: number;
  }
): Promise<MissionControlUsageResponse> {
  const params = new URLSearchParams();
  params.set("window", query.window);
  if (query.timezone?.trim()) {
    params.set("timezone", query.timezone.trim());
  }
  if (Number.isFinite(query.tz_offset_minutes)) {
    params.set("tz_offset_minutes", String(Math.trunc(query.tz_offset_minutes as number)));
  }
  if (Number.isFinite(query.window_start_ms) && Number.isFinite(query.window_end_ms)) {
    params.set("window_start_ms", String(Math.trunc(query.window_start_ms as number)));
    params.set("window_end_ms", String(Math.trunc(query.window_end_ms as number)));
  }
  return requestJson<MissionControlUsageResponse>(
    settings,
    `/api/v1/mission-control/usage?${params.toString()}`
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

export async function revokeAuthProfile(
  settings: RuntimeConnectionSettings,
  authProfileId: string,
  payload: {
    reason?: string;
    remove_secret?: boolean;
    disable_profile?: boolean;
    kill_switch_scope?: string;
  } = {}
): Promise<RevokeAuthProfileResponse> {
  return requestJson<RevokeAuthProfileResponse>(
    settings,
    `/api/v1/security/auth-profiles/${encodeURIComponent(authProfileId)}/revoke`,
    {
      method: "POST",
      body: payload,
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

export async function validateAnthropicSetupToken(
  settings: RuntimeConnectionSettings,
  payload: {
    setup_token: string;
    api_base_url?: string;
  }
): Promise<AnthropicSetupTokenValidateResponse> {
  return requestJson<AnthropicSetupTokenValidateResponse>(
    settings,
    "/api/v1/auth/anthropic/setup-token/validate",
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
  return requestBlob(
    settings,
    `/api/v1/agent-mail/messages/${encodeURIComponent(messageId)}/attachments/${encodeURIComponent(attachmentId)}`
  );
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
  return requestBlob(
    settings,
    `/api/v1/boards/${encodeURIComponent(boardId)}/cards/${encodeURIComponent(cardId)}/assets/${encodeURIComponent(cardAssetId)}`
  );
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
