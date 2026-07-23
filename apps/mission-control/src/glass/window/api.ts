import { getGatewayToken } from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";
import type {
  CreateOfficeChatterMessageResponse,
  FloorPresenceResponse,
  OfficeChatterResponse,
} from "./types";

export const OFFICE_REQUEST_TIMEOUT_MS = 10_000;

function gatewayUrl(settings: RuntimeConnectionSettings, path: string): string {
  const base = settings.gateway_url.trim().replace(/\/+$/, "");
  if (!base) {
    throw new Error("Gateway connection is not configured.");
  }
  return `${base}${path}`;
}

async function requestOfficeJson<T>(
  settings: RuntimeConnectionSettings,
  path: string,
  init?: RequestInit,
): Promise<T> {
  const token = await getGatewayToken();
  if (!token) {
    throw new Error("Gateway token is not configured.");
  }
  const controller = new AbortController();
  const abortFromCaller = () => controller.abort();
  if (init?.signal?.aborted) {
    abortFromCaller();
  } else {
    init?.signal?.addEventListener("abort", abortFromCaller, { once: true });
  }
  const timeout = globalThis.setTimeout(abortFromCaller, OFFICE_REQUEST_TIMEOUT_MS);
  let response: Response;
  try {
    response = await fetch(gatewayUrl(settings, path), {
      ...init,
      signal: controller.signal,
      headers: {
        Accept: "application/json",
        Authorization: `Bearer ${token}`,
        ...(init?.body ? { "Content-Type": "application/json" } : {}),
        ...init?.headers,
      },
    });
  } finally {
    globalThis.clearTimeout(timeout);
    init?.signal?.removeEventListener("abort", abortFromCaller);
  }
  const payload = (await response.json().catch(() => null)) as
    | { safe_human_message?: unknown }
    | T
    | null;
  if (!response.ok) {
    const safe =
      payload &&
      typeof payload === "object" &&
      "safe_human_message" in payload &&
      typeof payload.safe_human_message === "string"
        ? payload.safe_human_message
        : "The Office window is unavailable right now.";
    throw new Error(safe);
  }
  return payload as T;
}

export function getFloorPresence(
  settings: RuntimeConnectionSettings,
): Promise<FloorPresenceResponse> {
  return requestOfficeJson(settings, "/api/v1/office/floor-presence");
}

export function getOfficeChatter(
  settings: RuntimeConnectionSettings,
): Promise<OfficeChatterResponse> {
  return requestOfficeJson(
    settings,
    "/api/v1/office/chatter?limit_rooms=20&limit_messages=100",
  );
}

export function postOfficeChatterMessage(
  settings: RuntimeConnectionSettings,
  threadId: string,
  bodyText: string,
): Promise<CreateOfficeChatterMessageResponse> {
  return requestOfficeJson(
    settings,
    `/api/v1/office/chatter/rooms/${encodeURIComponent(threadId)}/messages`,
    {
      method: "POST",
      body: JSON.stringify({ body_text: bodyText }),
    },
  );
}
