import { getGatewayToken } from "./runtime";
import { websocketUrlFromGateway } from "./api";
import type { RuntimeConnectionSettings, WsEventFrame } from "../types";

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
}

export interface WsSubscription {
  close: () => void;
}

export function connectGatewayEvents(options: ConnectOptions): WsSubscription {
  let closed = false;
  let socket: WebSocket | null = null;
  let reconnectDelayMs = 750;

  const connect = async () => {
    const token = await getGatewayToken();
    if (!token || !options.settings.gateway_url.trim()) {
      options.onState("idle");
      return;
    }

    options.onState(reconnectDelayMs > 750 ? "reconnecting" : "connecting");

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
      reconnectDelayMs = 750;
      options.onState("connected");
    };

    socket.onmessage = (message) => {
      if (closed) {
        return;
      }
      if (typeof message.data !== "string") {
        return;
      }
      try {
        const parsed = JSON.parse(message.data) as Partial<WsEventFrame>;
        if (typeof parsed.event_type !== "string") {
          return;
        }
        options.onEvent(parsed as WsEventFrame);
      } catch {
        // Keep stream alive on malformed events.
      }
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
    const nextDelay = reconnectDelayMs;
    reconnectDelayMs = Math.min(reconnectDelayMs * 2, 5000);
    window.setTimeout(() => {
      if (!closed) {
        void connect();
      }
    }, nextDelay);
  };

  void connect();

  return {
    close() {
      closed = true;
      if (socket) {
        socket.close();
      }
      options.onState("idle");
    },
  };
}
