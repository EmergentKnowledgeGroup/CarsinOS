import { getGatewayToken } from "./runtime";
import type {
  BoardDetail,
  BoardDetailResponse,
  HealthResponse,
  ListAgentsResponse,
  ListBoardsResponse,
  MoveBoardCardResponse,
  RuntimeConnectionSettings,
  RunBoardCardResponse,
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
