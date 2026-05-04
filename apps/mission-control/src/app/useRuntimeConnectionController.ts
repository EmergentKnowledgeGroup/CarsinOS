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
import type { NotifyFn } from "./useAppController";

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
  setTokenConfiguredChecked: Dispatch<SetStateAction<boolean>>;
  setHealthState: Dispatch<SetStateAction<string>>;
  setWsState: Dispatch<SetStateAction<WsLifecycleState>>;
  setNotice: NotifyFn;
  setBoards: Dispatch<SetStateAction<BoardSummary[]>>;
  setAgents: Dispatch<SetStateAction<Agent[]>>;
  activeBoardId: string | null;
  setActiveBoardId: Dispatch<SetStateAction<string | null>>;
  refreshBoard: (boardId: string, runtimeSettings?: RuntimeConnectionSettings) => Promise<void>;
  setBoard: Dispatch<SetStateAction<BoardDetail | null>>;
  loadMissionControlReadModels: (
    runtimeSettings?: RuntimeConnectionSettings
  ) => Promise<void>;
  loadRunbookReadModels: (
    runtimeSettings?: RuntimeConnectionSettings
  ) => Promise<void>;
  loadAgentMailReadModels: (runtimeSettings?: RuntimeConnectionSettings) => Promise<void>;
}

function formatBaselineErrorDetail(error: unknown): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message.trim();
  }
  return String(error);
}

export function useRuntimeConnectionController(options: UseRuntimeConnectionControllerOptions) {
  const {
    settings,
    gatewayDraft,
    tokenDraft,
    setSettings,
    setTokenDraft,
    setTokenConfigured,
    setTokenConfiguredChecked,
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
    loadRunbookReadModels,
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
      const startupErrors: string[] = [];
      const [healthResult, boardListResult, agentListResult] = await Promise.allSettled([
        getGatewayHealth(runtimeSettings),
        listBoards(runtimeSettings),
        listAgents(runtimeSettings),
      ]);

      if (healthResult.status === "fulfilled") {
        setHealthState(healthResult.value.ok === true ? "up" : "down");
      } else {
        setHealthState("down");
        startupErrors.push(
          `Gateway health unavailable: ${formatBaselineErrorDetail(healthResult.reason)}`
        );
      }

      if (agentListResult.status === "fulfilled") {
        setAgents(agentListResult.value.items);
      } else {
        startupErrors.push(
          `Agent roster unavailable: ${formatBaselineErrorDetail(agentListResult.reason)}`
        );
      }

      if (boardListResult.status === "fulfilled") {
        const boardSummaries = boardListResult.value.items.map((item) => ({
          board_id: item.board_id,
          name: item.name,
        }));
        setBoards(boardSummaries);

        const targetBoardId =
          preferredBoardId ?? activeBoardId ?? boardListResult.value.items[0]?.board_id ?? null;
        setActiveBoardId(targetBoardId);
        if (targetBoardId) {
          try {
            await refreshBoard(targetBoardId, runtimeSettings);
          } catch (error: unknown) {
            startupErrors.push(
              `Board detail refresh failed: ${formatBaselineErrorDetail(error)}`
            );
          }
        } else {
          setBoard(null);
        }
      } else {
        startupErrors.push(
          `Board list unavailable: ${formatBaselineErrorDetail(boardListResult.reason)}`
        );
      }

      const readModelResults = await Promise.allSettled([
        loadMissionControlReadModels(runtimeSettings),
        loadRunbookReadModels(runtimeSettings),
        loadAgentMailReadModels(runtimeSettings),
      ]);
      for (const result of readModelResults) {
        if (result.status === "rejected") {
          startupErrors.push(formatBaselineErrorDetail(result.reason));
        }
      }

      if (startupErrors.length > 0) {
        throw new Error(startupErrors.join(" | "));
      }
    },
    [
      activeBoardId,
      loadAgentMailReadModels,
      loadMissionControlReadModels,
      loadRunbookReadModels,
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
    void isGatewayTokenConfigured().then((configured) => {
      setTokenConfigured(configured);
      setTokenConfiguredChecked(true);
    });
  }, [setTokenConfigured, setTokenConfiguredChecked]);

  const saveConnectionFromInputs = useCallback(
    async (gatewayUrl: string, tokenInput?: string) => {
      try {
        const nextSettings: RuntimeConnectionSettings = {
          gateway_url: gatewayUrl.trim(),
        };
        persistConnectionSettings(nextSettings);
        setSettings(nextSettings);

        const nextToken = tokenInput?.trim() ?? "";
        if (nextToken) {
          await setGatewayToken(nextToken);
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
        throw error;
      }
    },
    [loadBaseline, setNotice, setSettings, setTokenConfigured, setTokenDraft]
  );

  const saveConnection = useCallback(async () => {
    try {
      await saveConnectionFromInputs(gatewayDraft, tokenDraft);
    } catch (error: unknown) {
      void error;
    }
  }, [gatewayDraft, saveConnectionFromInputs, tokenDraft]);

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
    saveConnectionFromInputs,
    clearToken,
    reconnect,
  };
}
