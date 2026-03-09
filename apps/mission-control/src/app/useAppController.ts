import { useState } from "react";
import { loadConnectionSettings } from "../lib/runtime";
import type { WsLifecycleState } from "../lib/ws";
import type { RuntimeConnectionSettings } from "../types";

export interface Notice {
  tone: "info" | "error" | "critical";
  message: string;
}

/** Callback to push a toast notification (replaces legacy Dispatch<SetStateAction<Notice | null>>) */
export type NotifyFn = (notice: Notice | null) => void;

export type MissionControlTab =
  | "boards"
  | "calendar"
  | "focus"
  | "events"
  | "mail"
  | "chatrooms"
  | "assistant"
  | "team"
  | "cockpit"
  | "strategy"
  | "help";

export interface EventStreamItem {
  event_id: string;
  event_type: string;
  entity: string;
  ts_unix_ms: number;
  payload: Record<string, unknown>;
}

export function useAppController() {
  const [activeTab, setActiveTab] = useState<MissionControlTab>("boards");
  const [settings, setSettings] = useState<RuntimeConnectionSettings>(
    loadConnectionSettings()
  );
  const [gatewayDraft, setGatewayDraft] = useState(settings.gateway_url);
  const [tokenDraft, setTokenDraft] = useState("");
  const [tokenConfigured, setTokenConfigured] = useState(false);

  const [healthState, setHealthState] = useState("idle");
  const [wsState, setWsState] = useState<WsLifecycleState>("idle");
  const [eventStream, setEventStream] = useState<EventStreamItem[]>([]);
  const [showRawEvents, setShowRawEvents] = useState(false);

  return {
    activeTab,
    setActiveTab,
    settings,
    setSettings,
    gatewayDraft,
    setGatewayDraft,
    tokenDraft,
    setTokenDraft,
    tokenConfigured,
    setTokenConfigured,
    healthState,
    setHealthState,
    wsState,
    setWsState,
    eventStream,
    setEventStream,
    showRawEvents,
    setShowRawEvents,
  };
}
