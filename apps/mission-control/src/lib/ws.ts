import { getGatewayToken } from "./runtime";
import { websocketUrlFromGateway } from "./api";
import type { RuntimeConnectionSettings, WsEventFrame } from "../types";
import { WS_RECONNECT_INITIAL_MS, WS_RECONNECT_MAX_MS } from "../constants";

export type WsLifecycleState =
  | "idle"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

interface ConnectOptions {
  settings: RuntimeConnectionSettings;
  onEvent: (frame: WsEventFrame) => void;
  onState: (state: WsLifecycleState) => void;
  maxReconnectAttempts?: number;
}

export interface WsSubscription {
  close: () => void;
}

export function connectGatewayEvents(options: ConnectOptions): WsSubscription {
  let closed = false;
  let socket: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
  let reconnectDelayMs = WS_RECONNECT_INITIAL_MS;
  let reconnectAttempts = 0;

  const parseWsEventFrame = (raw: string): WsEventFrame | null => {
    try {
      const parsed = JSON.parse(raw) as Partial<WsEventFrame>;
      if (
        typeof parsed.schema_version !== "string" ||
        typeof parsed.event_id !== "string" ||
        typeof parsed.event_type !== "string" ||
        typeof parsed.ts_unix_ms !== "number" ||
        typeof parsed.entity !== "string" ||
        parsed.payload === undefined ||
        parsed.payload === null ||
        typeof parsed.payload !== "object"
      ) {
        return null;
      }
      return {
        schema_version: parsed.schema_version,
        event_id: parsed.event_id,
        event_type: parsed.event_type,
        ts_unix_ms: parsed.ts_unix_ms,
        request_id:
          typeof parsed.request_id === "string" || parsed.request_id === null
            ? parsed.request_id
            : undefined,
        entity: parsed.entity,
        payload: parsed.payload as Record<string, unknown>,
      };
    } catch {
      return null;
    }
  };

  const connect = async () => {
    const token = await getGatewayToken();
    if (closed) {
      options.onState("idle");
      return;
    }
    if (!token || !options.settings.gateway_url.trim()) {
      options.onState("idle");
      return;
    }

    options.onState(
      reconnectDelayMs > WS_RECONNECT_INITIAL_MS ? "reconnecting" : "connecting"
    );

    try {
      const wsUrl = websocketUrlFromGateway(options.settings, token);
      socket = new WebSocket(wsUrl);
    } catch {
      options.onState("error");
      scheduleReconnect();
      return;
    }

    socket.onopen = () => {
      if (closed) {
        socket?.close();
        return;
      }
      reconnectDelayMs = WS_RECONNECT_INITIAL_MS;
      reconnectAttempts = 0;
      options.onState("connected");
    };

    socket.onmessage = (message) => {
      if (closed) {
        return;
      }
      if (typeof message.data !== "string") {
        return;
      }
      const parsed = parseWsEventFrame(message.data);
      if (!parsed) {
        // Keep stream alive on malformed events.
        return;
      }
      options.onEvent(parsed);
    };

    socket.onclose = () => {
      if (closed) {
        return;
      }
      options.onState("reconnecting");
      scheduleReconnect();
    };

    socket.onerror = () => {
      if (closed) {
        return;
      }
      options.onState("error");
    };
  };

  const scheduleReconnect = () => {
    if (closed) {
      return;
    }
    reconnectAttempts += 1;
    if (
      options.maxReconnectAttempts !== undefined &&
      reconnectAttempts > options.maxReconnectAttempts
    ) {
      options.onState("error");
      return;
    }
    const nextDelay = reconnectDelayMs;
    reconnectDelayMs = Math.min(reconnectDelayMs * 2, WS_RECONNECT_MAX_MS);
    if (reconnectTimer !== null) {
      globalThis.clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    reconnectTimer = globalThis.setTimeout(() => {
      reconnectTimer = null;
      if (!closed) {
        void connect();
      }
    }, nextDelay);
  };

  void connect();

  return {
    close() {
      closed = true;
      if (reconnectTimer !== null) {
        globalThis.clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      if (socket) {
        socket.close();
      }
      options.onState("idle");
    },
  };
}
