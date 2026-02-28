import { useCallback, useEffect, type Dispatch, type SetStateAction } from "react";
import { getGatewayHealth, listAgents, listBoards } from "../lib/api";
import {
  clearGatewayToken,
  isGatewayTokenConfigured,
  persistConnectionSettings,
  setGatewayToken,
} from "../lib/runtime";
import type { WsLifecycleState } from "../lib/ws";
import type { RuntimeConnectionSettings, Agent, BoardDetail } from "../types";
import type { Notice } from "./useAppController";

export interface BoardSummary {
  board_id: string;
  name: string;
}

interface UseRuntimeConnectionControllerOptions {
  settings: RuntimeConnectionSettings;
  gatewayDraft: string;
  tokenDraft: string;
  setSettings: Dispatch<SetStateAction<RuntimeConnectionSettings>>;
  setTokenDraft: Dispatch<SetStateAction<string>>;
  setTokenConfigured: Dispatch<SetStateAction<boolean>>;
  setHealthState: Dispatch<SetStateAction<string>>;
  setWsState: Dispatch<SetStateAction<WsLifecycleState>>;
  setNotice: Dispatch<SetStateAction<Notice | null>>;
  setBoards: Dispatch<SetStateAction<BoardSummary[]>>;
  setAgents: Dispatch<SetStateAction<Agent[]>>;
  activeBoardId: string | null;
  setActiveBoardId: Dispatch<SetStateAction<string | null>>;
  refreshBoard: (boardId: string, runtimeSettings?: RuntimeConnectionSettings) => Promise<void>;
  setBoard: Dispatch<SetStateAction<BoardDetail | null>>;
  loadMissionControlReadModels: (
    runtimeSettings?: RuntimeConnectionSettings
  ) => Promise<void>;
  loadAgentMailReadModels: (runtimeSettings?: RuntimeConnectionSettings) => Promise<void>;
}

export function useRuntimeConnectionController(options: UseRuntimeConnectionControllerOptions) {
  const {
    settings,
    gatewayDraft,
    tokenDraft,
    setSettings,
    setTokenDraft,
    setTokenConfigured,
    setHealthState,
    setWsState,
    setNotice,
    setBoards,
    setAgents,
    activeBoardId,
    setActiveBoardId,
    refreshBoard,
    setBoard,
    loadMissionControlReadModels,
    loadAgentMailReadModels,
  } = options;

  const loadBaseline = useCallback(
    async (
      runtimeSettings: RuntimeConnectionSettings = settings,
      preferredBoardId?: string | null
    ) => {
      if (!runtimeSettings.gateway_url.trim()) {
        return;
      }

      setHealthState("checking");
      const [health, boardList, agentList] = await Promise.all([
        getGatewayHealth(runtimeSettings),
        listBoards(runtimeSettings),
        listAgents(runtimeSettings),
      ]);

      setHealthState(health.ok === false ? "down" : "up");
      setBoards(
        boardList.items.map((item) => ({
          board_id: item.board_id,
          name: item.name,
        }))
      );
      setAgents(agentList.items);

      const targetBoardId =
        preferredBoardId ?? activeBoardId ?? boardList.items[0]?.board_id ?? null;
      setActiveBoardId(targetBoardId);
      if (targetBoardId) {
        await refreshBoard(targetBoardId, runtimeSettings);
      } else {
        setBoard(null);
      }

      await Promise.all([
        loadMissionControlReadModels(runtimeSettings),
        loadAgentMailReadModels(runtimeSettings),
      ]);
    },
    [
      activeBoardId,
      loadAgentMailReadModels,
      loadMissionControlReadModels,
      refreshBoard,
      setActiveBoardId,
      setAgents,
      setBoard,
      setBoards,
      setHealthState,
      settings,
    ]
  );

  useEffect(() => {
    void isGatewayTokenConfigured().then(setTokenConfigured);
  }, [setTokenConfigured]);

  const saveConnection = useCallback(async () => {
    try {
      const nextSettings: RuntimeConnectionSettings = {
        gateway_url: gatewayDraft.trim(),
      };
      persistConnectionSettings(nextSettings);
      setSettings(nextSettings);

      if (tokenDraft.trim()) {
        await setGatewayToken(tokenDraft.trim());
        setTokenDraft("");
      }

      const hasToken = await isGatewayTokenConfigured();
      setTokenConfigured(hasToken);

      if (hasToken && nextSettings.gateway_url.trim()) {
        await loadBaseline(nextSettings);
        setNotice({ tone: "info", message: "Connection settings saved." });
      }
    } catch (error: unknown) {
      setNotice({
        tone: "critical",
        message: `Connection save failed: ${String(error)}`,
      });
    }
  }, [
    gatewayDraft,
    loadBaseline,
    setNotice,
    setSettings,
    setTokenConfigured,
    setTokenDraft,
    tokenDraft,
  ]);

  const clearToken = useCallback(async () => {
    await clearGatewayToken();
    setTokenConfigured(false);
    setWsState("idle");
    setNotice({ tone: "info", message: "Gateway token cleared." });
  }, [setNotice, setTokenConfigured, setWsState]);

  const reconnect = useCallback(async () => {
    try {
      await loadBaseline(settings);
      setNotice({ tone: "info", message: "Connection refreshed." });
    } catch (error: unknown) {
      setNotice({
        tone: "critical",
        message: `Reconnect failed: ${String(error)}`,
      });
    }
  }, [loadBaseline, setNotice, settings]);

  return {
    loadBaseline,
    saveConnection,
    clearToken,
    reconnect,
  };
}
