import { useEffect } from "react";
import { connectGatewayEvents, type WsLifecycleState } from "../lib/ws";
import type { RuntimeConnectionSettings, WsEventFrame } from "../types";

interface UseGatewayEventsOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  maxReconnectAttempts?: number;
  onState: (state: WsLifecycleState) => void;
  onEvent: (frame: WsEventFrame) => void;
}

export function useGatewayEvents(options: UseGatewayEventsOptions): void {
  const {
    settings,
    tokenConfigured,
    maxReconnectAttempts,
    onState,
    onEvent,
  } = options;

  useEffect(() => {
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      onState("idle");
      return;
    }

    const subscription = connectGatewayEvents({
      settings,
      maxReconnectAttempts: maxReconnectAttempts ?? 40,
      onState,
      onEvent,
    });

    return () => {
      subscription.close();
    };
  }, [maxReconnectAttempts, onEvent, onState, settings, tokenConfigured]);
}
