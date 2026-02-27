import { getGatewayToken } from "./runtime";
import type {
  BoardDetail,
  BoardDetailResponse,
  GetChannelRuntimeStatusResponse,
  HealthResponse,
  ListApprovalsResponse,
  ListJobsResponse,
  ListAgentsResponse,
  ListBoardsResponse,
  MissionControlCalendarWeekResponse,
  MissionControlFocusResponse,
  MoveBoardCardResponse,
  ResolveApprovalResponse,
  RuntimeConnectionSettings,
  RunJobNowResponse,
  RunBoardCardResponse,
  UpdateJobResponse,
  UpdateBoardCardResponse,
  UploadBoardCardAssetResponse,
} from "../types";

function normalizeGatewayUrl(gatewayUrl: string): string {
  const trimmed = gatewayUrl.trim();
  if (!trimmed) {
    throw new Error("Gateway URL is required.");
  }
  return trimmed.endsWith("/") ? trimmed : `${trimmed}/`;
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

const DEFAULT_GATEWAY_TIMEOUT_MS = 15_000;

async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs = DEFAULT_GATEWAY_TIMEOUT_MS
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

export async function listJobs(
  settings: RuntimeConnectionSettings,
  limit = 100
): Promise<ListJobsResponse> {
  return requestJson<ListJobsResponse>(
    settings,
    `/api/v1/jobs?limit=${encodeURIComponent(String(limit))}&include_disabled=true`
  );
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
