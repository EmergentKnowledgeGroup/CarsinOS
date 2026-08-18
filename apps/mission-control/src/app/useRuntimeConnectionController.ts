import { useCallback, useEffect, useRef, type Dispatch, type SetStateAction } from "react";
import { listen } from "@tauri-apps/api/event";
import { getGatewayHealth, listAgents, listBoards } from "../lib/api";
import {
  clearGatewayToken,
  getDesktopBootstrap,
  isGatewayTokenConfigured,
  isTauriRuntime,
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
  setGatewayDraft: Dispatch<SetStateAction<string>>;
  setTokenDraft: Dispatch<SetStateAction<string>>;
  setTokenConfigured: Dispatch<SetStateAction<boolean>>;
  setTokenConfiguredChecked: Dispatch<SetStateAction<boolean>>;
  /** Invalidates consumers whose truth is scoped to the secure token identity. */
  onAuthIdentityChanged: () => void;
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
    setGatewayDraft,
    setTokenDraft,
    setTokenConfigured,
    setTokenConfiguredChecked,
    onAuthIdentityChanged,
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

  // Mount-time identity reads must never overwrite a newer operator edit or
  // connection mutation. Draft changes and mutations synchronously invalidate
  // the generation captured by token/bootstrap promises.
  const desktopBootstrapGenerationRef = useRef(0);
  const tokenTruthGenerationRef = useRef(0);
  const lastGatewayDraftRef = useRef(gatewayDraft);
  useEffect(() => {
    if (lastGatewayDraftRef.current !== gatewayDraft) {
      lastGatewayDraftRef.current = gatewayDraft;
      desktopBootstrapGenerationRef.current += 1;
    }
  }, [gatewayDraft]);

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
    const generation = tokenTruthGenerationRef.current;
    let disposed = false;
    void isGatewayTokenConfigured().then((configured) => {
      if (
        disposed ||
        generation !== tokenTruthGenerationRef.current
      ) {
        return;
      }
      setTokenConfigured(configured);
      setTokenConfiguredChecked(true);
    });
    return () => {
      disposed = true;
    };
  }, [setTokenConfigured, setTokenConfiguredChecked]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | undefined;
    const generation = desktopBootstrapGenerationRef.current;
    void getDesktopBootstrap()
      .then((bootstrap) => {
        if (
          disposed ||
          !bootstrap ||
          generation !== desktopBootstrapGenerationRef.current
        ) {
          return;
        }
        const nextSettings = { gateway_url: bootstrap.gateway_url };
        setSettings(nextSettings);
        setGatewayDraft(bootstrap.gateway_url);
        if (bootstrap.startup_error) {
          setHealthState("down");
          setWsState("idle");
          setNotice({ tone: "critical", message: bootstrap.startup_error });
        }
      })
      .catch((error: unknown) => {
        if (
          !disposed &&
          generation === desktopBootstrapGenerationRef.current
        ) {
          setHealthState("down");
          setWsState("idle");
          setNotice({
            tone: "critical",
            message: `Desktop gateway bootstrap failed: ${formatBaselineErrorDetail(error)}`,
          });
        }
      });

    void listen<{ message?: string }>("gateway-terminated", (event) => {
      if (disposed) {
        return;
      }
      setHealthState("down");
      setWsState("idle");
      setNotice({
        tone: "critical",
        message:
          event.payload?.message?.trim() ||
          "The managed gateway stopped. Restart CarsinOS to reconnect.",
      });
    })
      .then((disposeListener) => {
        if (disposed) {
          disposeListener();
        } else {
          unlisten = disposeListener;
        }
      })
      .catch((error: unknown) => {
        if (!disposed) {
          setNotice({
            tone: "critical",
            message: `Desktop gateway monitoring failed: ${formatBaselineErrorDetail(error)}`,
          });
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [setGatewayDraft, setHealthState, setNotice, setSettings, setWsState]);

  // One synchronous lock guards every sibling mutation on the single
  // connection authority (save, reconnect, clear). React render state is not
  // a lock: same-tick duplicates must be refused before the first await.
  const connectionActionLockRef = useRef(false);

  const saveConnectionFromInputs = useCallback(
    async (gatewayUrl: string, tokenInput?: string) => {
      if (connectionActionLockRef.current) {
        // Reject instead of resolving: a caller awaiting this save (the
        // onboarding wizard) must never mistake a refused duplicate for a
        // successful save. No notice here - the surviving call owns feedback.
        throw new Error("Another connection operation is already in progress.");
      }
      connectionActionLockRef.current = true;
      desktopBootstrapGenerationRef.current += 1;
      try {
        const nextSettings: RuntimeConnectionSettings = {
          gateway_url: gatewayUrl.trim(),
        };
        persistConnectionSettings(nextSettings);
        setSettings(nextSettings);
        // Every save shares one draft truth: a wizard save must land in the
        // Settings/Setup gateway field instead of leaving a stale blank
        // draft that a blind re-save would persist as an empty connection.
        setGatewayDraft(nextSettings.gateway_url);

        const nextToken = tokenInput?.trim() ?? "";
        if (nextToken) {
          await setGatewayToken(nextToken);
          tokenTruthGenerationRef.current += 1;
          onAuthIdentityChanged();
        }

        const hasToken = await isGatewayTokenConfigured();
        setTokenConfigured(hasToken);
        setTokenConfiguredChecked(true);

        if (hasToken && nextSettings.gateway_url.trim()) {
          await loadBaseline(nextSettings);
          if (nextToken) {
            setTokenDraft("");
          }
          setNotice({ tone: "info", message: "Connection settings saved." });
        }
      } catch (error: unknown) {
        setNotice({
          tone: "critical",
          message: `Connection save failed: ${String(error)}`,
        });
        throw error;
      } finally {
        connectionActionLockRef.current = false;
      }
    },
    [
      loadBaseline,
      onAuthIdentityChanged,
      setGatewayDraft,
      setNotice,
      setSettings,
      setTokenConfigured,
      setTokenConfiguredChecked,
      setTokenDraft,
    ]
  );

  const saveConnection = useCallback(async () => {
    try {
      await saveConnectionFromInputs(gatewayDraft, tokenDraft);
    } catch (error: unknown) {
      void error;
    }
  }, [gatewayDraft, saveConnectionFromInputs, tokenDraft]);

  const clearToken = useCallback(async () => {
    if (connectionActionLockRef.current) {
      return;
    }
    connectionActionLockRef.current = true;
    try {
      await clearGatewayToken();
    } catch (error: unknown) {
      // The secure store still holds the token: keep the configured truth
      // and the live connection state exactly as they were.
      setNotice({
        tone: "critical",
        message: `Forget token failed: ${String(error)}`,
      });
      return;
    } finally {
      connectionActionLockRef.current = false;
    }
    tokenTruthGenerationRef.current += 1;
    onAuthIdentityChanged();
    setTokenConfigured(false);
    setTokenConfiguredChecked(true);
    setWsState("idle");
    setNotice({ tone: "info", message: "Gateway token cleared." });
  }, [
    onAuthIdentityChanged,
    setNotice,
    setTokenConfigured,
    setTokenConfiguredChecked,
    setWsState,
  ]);

  const reconnect = useCallback(async () => {
    if (connectionActionLockRef.current) {
      return;
    }
    connectionActionLockRef.current = true;
    try {
      await loadBaseline(settings);
      setNotice({ tone: "info", message: "Connection refreshed." });
    } catch (error: unknown) {
      setNotice({
        tone: "critical",
        message: `Reconnect failed: ${String(error)}`,
      });
    } finally {
      connectionActionLockRef.current = false;
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
