import { useEffect, useRef } from "react";
import { connectGatewayEvents, type WsLifecycleState } from "../lib/ws";
import type { ExecassWsFrame } from "../glass/execass/types";
import type { RuntimeConnectionSettings, WsEventFrame } from "../types";
import { WS_MAX_RECONNECT_ATTEMPTS } from "../constants";

interface UseGatewayEventsOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  maxReconnectAttempts?: number;
  onState: (state: WsLifecycleState) => void;
  onEvent: (frame: WsEventFrame) => void;
  onExecassFrame?: (frame: ExecassWsFrame) => void;
  onOpen?: (send: (text: string) => void) => void;
}

export function useGatewayEvents(options: UseGatewayEventsOptions): void {
  const {
    settings,
    tokenConfigured,
    maxReconnectAttempts,
    onState,
    onEvent,
    onExecassFrame,
    onOpen,
  } = options;
  const onStateRef = useRef(onState);
  const onEventRef = useRef(onEvent);
  const onExecassFrameRef = useRef(onExecassFrame);
  const onOpenRef = useRef(onOpen);

  useEffect(() => {
    onStateRef.current = onState;
  }, [onState]);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useEffect(() => {
    onExecassFrameRef.current = onExecassFrame;
  }, [onExecassFrame]);

  useEffect(() => {
    onOpenRef.current = onOpen;
  }, [onOpen]);

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
      onExecassFrame: (frame) => onExecassFrameRef.current?.(frame),
      onOpen: (send) => onOpenRef.current?.(send),
    });

    return () => {
      subscription.close();
    };
  }, [maxReconnectAttempts, settings, tokenConfigured]);
}
