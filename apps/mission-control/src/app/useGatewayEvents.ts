import { useEffect, useRef } from "react";
import { connectGatewayEvents, type WsLifecycleState } from "../lib/ws";
import type { RuntimeConnectionSettings, WsEventFrame } from "../types";
import { WS_MAX_RECONNECT_ATTEMPTS } from "../constants";

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
  const onStateRef = useRef(onState);
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onStateRef.current = onState;
  }, [onState]);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useEffect(() => {
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      onStateRef.current("idle");
      return;
    }

    const subscription = connectGatewayEvents({
      settings,
      maxReconnectAttempts: maxReconnectAttempts ?? WS_MAX_RECONNECT_ATTEMPTS,
      onState: (state) => onStateRef.current(state),
      onEvent: (frame) => onEventRef.current(frame),
    });

    return () => {
      subscription.close();
    };
  }, [maxReconnectAttempts, settings, tokenConfigured]);
}
