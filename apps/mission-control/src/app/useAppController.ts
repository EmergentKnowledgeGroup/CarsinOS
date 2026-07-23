import { useCallback, useMemo, useState } from "react";
import { DEFAULT_FLOORS, findRoom, roomForTab } from "../glass/floors";
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
  | "window"
  | "team"
  | "cockpit"
  | "strategy"
  | "runbook"
  | "memory"
  | "connectors"
  | "help";

export interface EventStreamItem {
  event_id: string;
  event_type: string;
  entity: string;
  ts_unix_ms: number;
  payload: Record<string, unknown>;
}

export function useAppController() {
  const [activeTab, setActiveTabState] = useState<MissionControlTab>("boards");
  /**
   * The last room chosen by an explicit room selection. Stable room ids own
   * navigation identity: a selection stays lit only while the active tab is
   * still the surface that room owns; tab-only navigations elsewhere clear it
   * so shared surfaces resolve honestly instead of remembering stale picks.
   */
  const [selectedRoomId, setSelectedRoomId] = useState<string | null>(null);

  const setActiveTab = useCallback((tab: MissionControlTab) => {
    setActiveTabState(tab);
    setSelectedRoomId((current) => {
      if (!current) return current;
      const found = findRoom(DEFAULT_FLOORS, current);
      return found && found.room.route === tab ? current : null;
    });
  }, []);

  /** Navigate by stable room id; unknown ids fail closed. */
  const selectRoom = useCallback((roomId: string) => {
    const found = findRoom(DEFAULT_FLOORS, roomId);
    if (!found) return;
    setSelectedRoomId(roomId);
    setActiveTabState(found.room.route);
  }, []);

  const activeRoomId = useMemo(
    () =>
      roomForTab(DEFAULT_FLOORS, activeTab, selectedRoomId ?? undefined)?.room
        .id ?? null,
    [activeTab, selectedRoomId],
  );
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
    activeRoomId,
    selectRoom,
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
