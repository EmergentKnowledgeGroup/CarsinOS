import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  createBoardCard,
  createSession,
  createSessionMessage,
  createSessionRun,
  getSession,
  getBoard,
  getRuntimeConfig,
  listProviderCapabilities,
  listProviderModels,
  listSessionMessages,
} from "../../lib/api";
import type { BoardSummary } from "../../app/useRuntimeConnectionController";
import type { NotifyFn } from "../../app/useAppController";
import type {
  Agent,
  AuthProfileResponse,
  MessageResponse,
  ProviderCapabilityResponse,
  RuntimeConnectionSettings,
  RuntimeRoutingConfigResponse,
} from "../../types";
import {
  buildProviderOptions,
  enabledAuthProfilesForProvider,
  mergeCatalogModelOptions,
} from "../providers/providerModelCatalog";
import { DEFAULT_ASSISTANT_CORE_PROMPT } from "./corePrompt";

interface UseAssistantChatControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  agents: Agent[];
  authProfiles: AuthProfileResponse[];
  boards: BoardSummary[];
  setNotice: NotifyFn;
  corePrompt: string;
  corePromptSaved: string;
  corePromptLoading: boolean;
  corePromptSaving: boolean;
  corePromptError: string | null;
  corePromptDirty: boolean;
  setCorePrompt: (value: string) => void;
  saveCorePrompt: () => Promise<void>;
  resetCorePrompt: () => void;
  restoreDefaultCorePrompt: () => void;
}

type AssistantSessionMode = "canonical_lane" | "pinned_session";

function normalizeMessages(items: MessageResponse[]): MessageResponse[] {
  return [...items].sort((left, right) => left.created_at - right.created_at);
}

function mergeMessage(items: MessageResponse[], next: MessageResponse): MessageResponse[] {
  return normalizeMessages([
    ...items.filter((item) => item.message_id !== next.message_id),
    next,
  ]);
}

function assignedAgentIdsForHuman(
  routing: RuntimeRoutingConfigResponse | null,
  humanIdentityId: string
): string[] {
  if (!routing || !humanIdentityId) {
    return [];
  }
  const human = routing.human_identities.find(
    (item) => item.human_identity_id === humanIdentityId
  );
  if (!human?.enabled) {
    return [];
  }
  return Array.from(
    new Set(
      routing.assistant_assignments
        .filter(
          (assignment) =>
            assignment.enabled && assignment.human_identity_id === humanIdentityId
        )
        .map((assignment) => assignment.assistant_agent_id.trim())
        .filter(Boolean)
    )
  );
}

export function useAssistantChatController(options: UseAssistantChatControllerOptions) {
  const {
    settings,
    tokenConfigured,
    agents,
    authProfiles,
    setNotice,
    corePrompt,
    corePromptSaved,
    corePromptLoading,
    corePromptSaving,
    corePromptError,
    corePromptDirty,
    setCorePrompt,
    saveCorePrompt,
    resetCorePrompt,
    restoreDefaultCorePrompt,
  } = options;

  const [selectedAgentId, setSelectedAgentId] = useState(agents[0]?.agent_id ?? "");
  const [pinnedSessionAgentId, setPinnedSessionAgentId] = useState<string | null>(null);
  const [modelProvider, setModelProvider] = useState(agents[0]?.model_provider ?? "");
  const [modelId, setModelId] = useState(agents[0]?.model_id ?? "");
  const [authProfileId, setAuthProfileId] = useState("");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<MessageResponse[]>([]);
  const [draft, setDraft] = useState("");
  const [targetBoardId, setTargetBoardId] = useState(options.boards[0]?.board_id ?? "");
  const [busy, setBusy] = useState(false);
  const [sendStatus, setSendStatus] = useState<string | null>(null);
  const [optimisticUserMessage, setOptimisticUserMessage] =
    useState<MessageResponse | null>(null);
  const [lastRunId, setLastRunId] = useState<string | null>(null);
  const [lastRunStatus, setLastRunStatus] = useState<string | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [providerCapabilities, setProviderCapabilities] = useState<
    ProviderCapabilityResponse[]
  >([]);
  const [providerCapabilitiesLoading, setProviderCapabilitiesLoading] = useState(false);
  const [providerCapabilitiesError, setProviderCapabilitiesError] = useState<string | null>(null);
  const [catalogModels, setCatalogModels] = useState<string[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [runtimeRouting, setRuntimeRouting] = useState<RuntimeRoutingConfigResponse | null>(null);
  const [runtimeRoutingLoaded, setRuntimeRoutingLoaded] = useState(false);
  const [runtimeRoutingError, setRuntimeRoutingError] = useState<string | null>(null);
  const [sessionMode, setSessionMode] = useState<AssistantSessionMode>("canonical_lane");
  const suppressSessionResetRef = useRef(false);
  const runtimeRoutingRef = useRef<RuntimeRoutingConfigResponse | null>(null);

  const applyRuntimeRouting = useCallback((routing: RuntimeRoutingConfigResponse) => {
    runtimeRoutingRef.current = routing;
    setRuntimeRouting(routing);
    setRuntimeRoutingLoaded(true);
    setRuntimeRoutingError(null);
  }, []);
  const refreshRoutingState = useCallback(async () => {
    try {
      const response = await getRuntimeConfig(settings);
      applyRuntimeRouting(response.config.routing);
      return response.config.routing;
    } catch (error: unknown) {
      setRuntimeRoutingLoaded(true);
      setRuntimeRoutingError(String(error));
      return runtimeRoutingRef.current;
    }
  }, [applyRuntimeRouting, settings]);

  const activeAgentId =
    sessionMode === "pinned_session" ? pinnedSessionAgentId ?? selectedAgentId : selectedAgentId;
  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.agent_id === activeAgentId) ?? null,
    [activeAgentId, agents]
  );
  const lastAssistantMessage = useMemo(
    () => [...messages].reverse().find((item) => item.role === "assistant") ?? null,
    [messages]
  );
  const displayMessages = useMemo(
    () =>
      optimisticUserMessage
        ? mergeMessage(messages, optimisticUserMessage)
        : messages,
    [messages, optimisticUserMessage]
  );
  const providerOptions = useMemo(
    () => buildProviderOptions(providerCapabilities, providerCapabilitiesError, modelProvider),
    [modelProvider, providerCapabilities, providerCapabilitiesError]
  );
  const availableAuthProfiles = useMemo(
    () => enabledAuthProfilesForProvider(authProfiles, modelProvider),
    [authProfiles, modelProvider]
  );
  const catalogModelOptions = useMemo(
    () => mergeCatalogModelOptions(modelId, catalogModels),
    [catalogModels, modelId]
  );
  const localOperatorHumanIdentityId = useMemo(
    () => runtimeRouting?.local_operator_human_identity_id?.trim() ?? "",
    [runtimeRouting]
  );
  const localOperatorHuman = useMemo(() => {
    if (!runtimeRouting || !localOperatorHumanIdentityId) {
      return null;
    }
    return (
      runtimeRouting.human_identities.find(
        (item) => item.human_identity_id === localOperatorHumanIdentityId
      ) ?? null
    );
  }, [localOperatorHumanIdentityId, runtimeRouting]);
  const locallyAssignedAgentIds = useMemo(() => {
    return assignedAgentIdsForHuman(runtimeRouting, localOperatorHumanIdentityId);
  }, [localOperatorHumanIdentityId, runtimeRouting]);
  const availableAgents = useMemo(() => {
    if (!runtimeRoutingLoaded) {
      return [];
    }
    if (!localOperatorHumanIdentityId || locallyAssignedAgentIds.length === 0) {
      return [];
    }
    const allowedAgentIds = new Set(locallyAssignedAgentIds);
    return agents.filter((agent) => allowedAgentIds.has(agent.agent_id));
  }, [
    agents,
    localOperatorHumanIdentityId,
    locallyAssignedAgentIds,
    runtimeRoutingLoaded,
  ]);
  const assistantAvailabilityMessage = useMemo(() => {
    if (!runtimeRoutingLoaded) {
      return "Loading the local operator route from Team.";
    }
    if (runtimeRoutingError && runtimeRouting) {
      return "Team routing could not refresh cleanly. Showing the last known local route.";
    }
    if (runtimeRoutingError) {
      return "Team routing could not load cleanly. Assistant is staying locked until it can confirm your route.";
    }
    if (!localOperatorHumanIdentityId) {
      return "Pick the local app operator in Team before using Assistant.";
    }
    if (!localOperatorHuman?.enabled) {
      return "The local app operator is disabled in Team. Re-enable that person or choose a different local operator first.";
    }
    if (availableAgents.length === 0) {
      return "No assistant is assigned to the local operator yet. Assign one in Team.";
    }
    return "This list only shows assistants routed to the local operator.";
  }, [
    availableAgents.length,
    localOperatorHuman,
    localOperatorHumanIdentityId,
    runtimeRouting,
    runtimeRoutingError,
    runtimeRoutingLoaded,
  ]);

  useEffect(() => {
    if (sessionMode === "pinned_session") {
      return;
    }
    if (availableAgents.length > 0) {
      if (
        !selectedAgentId ||
        !availableAgents.some((agent) => agent.agent_id === selectedAgentId)
      ) {
        setSelectedAgentId(availableAgents[0].agent_id);
      }
      return;
    }
    if (agents.length === 0) {
      if (selectedAgentId) {
        setSelectedAgentId("");
      }
      return;
    }
    if (runtimeRoutingLoaded) {
      setSelectedAgentId("");
      return;
    }
    if (!selectedAgentId || !agents.some((agent) => agent.agent_id === selectedAgentId)) {
      setSelectedAgentId(agents[0]?.agent_id ?? "");
    }
  }, [agents, availableAgents, runtimeRoutingLoaded, selectedAgentId, sessionMode]);

  useEffect(() => {
    if (options.boards.length === 0) {
      setTargetBoardId("");
      return;
    }
    if (!targetBoardId || !options.boards.some((board) => board.board_id === targetBoardId)) {
      setTargetBoardId(options.boards[0].board_id);
    }
  }, [options.boards, targetBoardId]);

  useEffect(() => {
    if (!selectedAgent) {
      return;
    }
    setModelProvider(selectedAgent.model_provider || "");
    setModelId(selectedAgent.model_id || "");
    setAuthProfileId("");
    if (suppressSessionResetRef.current) {
      suppressSessionResetRef.current = false;
      return;
    }
    setSessionMode("canonical_lane");
    setPinnedSessionAgentId(null);
    setSessionId(null);
    setMessages([]);
    setOptimisticUserMessage(null);
    setSendStatus(null);
    setLastRunId(null);
    setLastRunStatus(null);
    setLastError(null);
  }, [selectedAgent]);

  useEffect(() => {
    let cancelled = false;
    void refreshRoutingState().then((routing) => {
      if (cancelled || routing) {
        return;
      }
    });
    return () => {
      cancelled = true;
    };
  }, [refreshRoutingState]);

  useEffect(() => {
    let cancelled = false;
    setProviderCapabilitiesLoading(true);
    setProviderCapabilitiesError(null);
    void listProviderCapabilities(settings)
      .then((response) => {
        if (!cancelled) {
          setProviderCapabilities(response.items);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setProviderCapabilitiesError(String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setProviderCapabilitiesLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [settings]);

  const refreshModelCatalog = useCallback(async () => {
    const provider = modelProvider.trim().toLowerCase();
    if (!provider) {
      setCatalogModels([]);
      setCatalogError("Choose a provider first.");
      return;
    }

    setCatalogLoading(true);
    setCatalogError(null);
    try {
      const response = await listProviderModels(settings, {
        provider,
        agent_id: activeAgentId.trim() || undefined,
        auth_profile_id: authProfileId.trim() || undefined,
        refresh: true,
      });
      const modelIds = response.items.map((item) => item.model_id);
      setCatalogModels(modelIds);
      setModelId((current) => {
        if (current.trim() && modelIds.includes(current.trim())) {
          return current.trim();
        }
        return modelIds[0] ?? current.trim();
      });
    } catch (error: unknown) {
      setCatalogModels([]);
      setCatalogError(String(error));
    } finally {
      setCatalogLoading(false);
    }
  }, [activeAgentId, authProfileId, modelProvider, settings]);

  useEffect(() => {
    if (!modelProvider.trim()) {
      setCatalogModels([]);
      setCatalogError(null);
      return;
    }
    void refreshModelCatalog();
  }, [modelProvider, authProfileId, activeAgentId, refreshModelCatalog]);

  useEffect(() => {
    if (
      authProfileId &&
      !availableAuthProfiles.some((profile) => profile.auth_profile_id === authProfileId)
    ) {
      setAuthProfileId("");
    }
  }, [authProfileId, availableAuthProfiles]);

  const refreshMessages = useCallback(
    async (id: string) => {
      const response = await listSessionMessages(settings, id, 300);
      setMessages(normalizeMessages(response.items));
    },
    [settings]
  );

  const resolveLocalOperatorRouting = useCallback(async (options?: { preferCached?: boolean }) => {
    let routing =
      options?.preferCached === true ? runtimeRoutingRef.current : null;
    if (!routing) {
      routing = (await refreshRoutingState()) ?? runtimeRoutingRef.current;
    }
    if (!routing) {
      throw new Error(
        "Team routing could not load cleanly. Retry once the local operator route is available."
      );
    }
    const localOperatorId = routing.local_operator_human_identity_id?.trim() ?? "";
    if (!localOperatorId) {
      throw new Error("Choose the local app operator in Team before starting Assistant chat.");
    }
    const localOperator = routing.human_identities.find(
      (item) => item.human_identity_id === localOperatorId
    );
    if (!localOperator?.enabled) {
      throw new Error(
        "The local app operator is disabled in Team. Re-enable that person or pick a different local operator first."
      );
    }
    return {
      routing,
      localOperatorId,
      localOperatorAgentIds: assignedAgentIdsForHuman(routing, localOperatorId),
    };
  }, [refreshRoutingState]);

  const ensureCanonicalLaneSession = useCallback(async () => {
    const selectedAssistantId =
      selectedAgentId.trim() || availableAgents[0]?.agent_id.trim() || "";
    if (!selectedAssistantId) {
      throw new Error("Select an agent first.");
    }
    if (!settings.gateway_url.trim()) {
      throw new Error("Connect Mission Control to the gateway first.");
    }
    const { localOperatorAgentIds, localOperatorId } = await resolveLocalOperatorRouting({
      preferCached: true,
    });
    if (!localOperatorAgentIds.includes(selectedAssistantId)) {
      throw new Error(
        "That assistant is not routed to the local operator. Pick the assigned assistant in Team first."
      );
    }
    const response = await createSession(settings, {
      agent_id: selectedAssistantId,
      human_identity_id: localOperatorId,
    });
    const id = response.session.session_id;
    if (sessionId !== id) {
      setSessionId(id);
      await refreshMessages(id);
    } else if (messages.length === 0) {
      await refreshMessages(id);
    }
    return id;
  }, [
    messages.length,
    availableAgents,
    refreshMessages,
    resolveLocalOperatorRouting,
    selectedAgentId,
    sessionId,
    settings,
  ]);

  const ensureSession = useCallback(async () => {
    if (sessionMode === "pinned_session") {
      if (!sessionId) {
        throw new Error("Open a transcript first, or return to your lane before sending.");
      }
      const pinnedAgentId = pinnedSessionAgentId?.trim() || selectedAgentId.trim();
      const { localOperatorAgentIds } = await resolveLocalOperatorRouting();
      if (!pinnedAgentId || !localOperatorAgentIds.includes(pinnedAgentId)) {
        setSessionMode("canonical_lane");
        setPinnedSessionAgentId(null);
        setSessionId(null);
      setMessages([]);
      setOptimisticUserMessage(null);
      setSendStatus(null);
      setLastRunId(null);
        setLastRunStatus(null);
        throw new Error(
          "That pinned transcript is not routed to the local operator anymore. Return to your lane or reassign it in Team first."
        );
      }
      if (messages.length === 0) {
        await refreshMessages(sessionId);
      }
      return sessionId;
    }
    return ensureCanonicalLaneSession();
  }, [
    ensureCanonicalLaneSession,
    messages.length,
    pinnedSessionAgentId,
    refreshMessages,
    resolveLocalOperatorRouting,
    selectedAgentId,
    sessionId,
    sessionMode,
  ]);

  const openSession = useCallback(
    async (id: string, options?: { runId?: string | null }) => {
      const normalizedId = id.trim();
      if (!normalizedId) {
        return false;
      }
      setBusy(true);
      setLastError(null);
      try {
        const { localOperatorAgentIds } = await resolveLocalOperatorRouting();
        const sessionResponse = await getSession(settings, normalizedId);
        const sessionAgentId = sessionResponse.session.agent_id.trim();
        if (!sessionAgentId) {
          throw new Error("That session is missing its assistant binding.");
        }
        if (!agents.some((agent) => agent.agent_id === sessionAgentId)) {
          throw new Error(
            "That session belongs to an assistant that is not available in this workspace."
          );
        }
        if (
          !localOperatorAgentIds.includes(sessionAgentId)
        ) {
          throw new Error(
            "That transcript is not routed to the local operator anymore. Reassign it in Team first."
          );
        }
        await refreshMessages(normalizedId);
        setSessionId(normalizedId);
        setSessionMode("pinned_session");
        setPinnedSessionAgentId(sessionAgentId);
        if (sessionAgentId !== activeAgentId) {
          suppressSessionResetRef.current = true;
        }
        setLastRunId(options?.runId ?? null);
        setLastRunStatus(null);
        return true;
      } catch (error: unknown) {
        const text = String(error);
        setLastError(text);
        setNotice({ tone: "error", message: `Assistant session load failed: ${text}` });
        return false;
      } finally {
        setBusy(false);
      }
    },
    [
      agents,
      refreshMessages,
      resolveLocalOperatorRouting,
      activeAgentId,
      setNotice,
      settings,
    ]
  );

  const resetToCanonicalLane = useCallback(() => {
    setSessionMode("canonical_lane");
    setPinnedSessionAgentId(null);
    setSessionId(null);
    setMessages([]);
    setOptimisticUserMessage(null);
    setSendStatus(null);
    setLastRunId(null);
    setLastRunStatus(null);
    setLastError(null);
  }, []);

  const injectCorePrompt = useCallback(async () => {
    if (corePromptDirty) {
      setBusy(true);
      setLastError(null);
      try {
        await saveCorePrompt();
        setNotice({
          tone: "info",
          message: "Shared prompt saved. Assistant, Discord, and Telegram all use it automatically.",
        });
      } catch (error: unknown) {
        const text = String(error);
        setLastError(text);
        setNotice({ tone: "error", message: `Saving shared prompt failed: ${text}` });
      } finally {
        setBusy(false);
      }
      return;
    }
    setNotice({
      tone: "info",
      message:
        "Shared prompt is already saved. New Assistant, Discord, and Telegram runs use it automatically.",
    });
  }, [corePromptDirty, saveCorePrompt, setNotice]);

  const send = useCallback(async () => {
    const body = draft.trim();
    if (!body) {
      return;
    }
    if (!tokenConfigured) {
      setNotice({ tone: "error", message: "Gateway token is missing." });
      return;
    }
    const selectedRunAgent = selectedAgent ?? availableAgents[0] ?? null;
    const resolvedModelProvider = selectedRunAgent?.model_provider?.trim() ?? "";
    const resolvedModelId = selectedRunAgent?.model_id?.trim() ?? "";
    if (!resolvedModelProvider || !resolvedModelId) {
      setNotice({
        tone: "error",
        message: "This assistant does not have a provider and model yet. Fix it in Team first.",
      });
      return;
    }

    setBusy(true);
    setLastError(null);
    setSendStatus("Sending message to the lane...");
    const pendingMessage: MessageResponse = {
      message_id: `local-pending-${Date.now()}`,
      session_id: sessionId ?? "pending",
      source_channel: "assistant-chat",
      source_peer_id: null,
      source_message_id: null,
      role: "user",
      content_text: body,
      content_format: "markdown",
      created_at: Date.now(),
    };
    setOptimisticUserMessage(pendingMessage);
    setDraft("");
    setLastRunStatus(null);
    setLastRunId(null);
    try {
      const id = await ensureSession();
      setOptimisticUserMessage({ ...pendingMessage, session_id: id });
      const createdMessage = await createSessionMessage(settings, id, {
        role: "user",
        content_text: body,
        content_format: "markdown",
        source_channel: "assistant-chat",
      });
      setMessages((current) => mergeMessage(current, createdMessage.message));
      setOptimisticUserMessage(null);
      setSendStatus("Waiting for the model response...");
      setLastRunStatus("running");
      const run = await createSessionRun(
        settings,
        id,
        sessionMode === "canonical_lane"
          ? {}
          : {
              model_provider: resolvedModelProvider,
              model_id: resolvedModelId,
            }
      );
      setLastRunId(run.run.run_id);
      setLastRunStatus(run.run.status);
      setSendStatus("Refreshing the transcript...");
      await refreshMessages(id);
      if (run.run.status !== "succeeded") {
        setLastError(run.run.error_text ?? `Run ended with status ${run.run.status}`);
      }
    } catch (error: unknown) {
      const text = String(error);
      setLastError(text);
      setDraft((current) => (current.trim() ? current : body));
      setNotice({ tone: "error", message: `Assistant run failed: ${text}` });
    } finally {
      setOptimisticUserMessage(null);
      setSendStatus(null);
      setBusy(false);
    }
  }, [
    draft,
    ensureSession,
    availableAgents,
    refreshMessages,
    selectedAgent,
    setNotice,
    settings,
    sessionId,
    sessionMode,
    tokenConfigured,
  ]);

  const sendLastAssistantToBoard = useCallback(async () => {
    if (!lastAssistantMessage) {
      setNotice({ tone: "error", message: "No assistant reply available to send to board." });
      return false;
    }
    if (!targetBoardId.trim()) {
      setNotice({ tone: "error", message: "Select a board target first." });
      return false;
    }
    setBusy(true);
    try {
      const board = await getBoard(settings, targetBoardId);
      const firstColumn = board.columns[0];
      if (!firstColumn) {
        throw new Error("Board has no columns.");
      }
      const normalized = lastAssistantMessage.content_text.trim();
      const firstLine =
        normalized.split(/\r?\n/).map((line) => line.trim()).find((line) => line.length > 0) ??
        "Assistant follow-up";
      const title = firstLine.slice(0, 120);
      await createBoardCard(settings, targetBoardId, {
        column_id: firstColumn.column_id,
        title,
        owner_kind: activeAgentId ? "agent" : "unassigned",
        owner_agent_id: activeAgentId || undefined,
      });
      setNotice({ tone: "info", message: `Created board card from assistant reply: ${title}` });
      return true;
    } catch (error: unknown) {
      const text = String(error);
      setNotice({ tone: "error", message: `Send to board failed: ${text}` });
      return false;
    } finally {
      setBusy(false);
    }
  }, [activeAgentId, lastAssistantMessage, setNotice, settings, targetBoardId]);

  return {
    selectedAgentId: activeAgentId,
    setSelectedAgentId: (value: string) => {
      setSessionMode("canonical_lane");
      setPinnedSessionAgentId(null);
      setSelectedAgentId(value);
    },
    selectedAgent,
    availableAgents,
    assistantAvailabilityMessage,
    runtimeRoutingLoaded,
    runtimeRoutingError,
    refreshRoutingState,
    providerOptions,
    providerCapabilitiesLoading,
    providerCapabilitiesError,
    modelProvider,
    setModelProvider,
    availableAuthProfiles,
    modelId,
    setModelId,
    authProfileId,
    setAuthProfileId,
    catalogModelOptions,
    catalogLoading,
    catalogError,
    refreshModelCatalog,
    sessionId,
    sessionMode,
    messages: displayMessages,
    draft,
    setDraft,
    corePrompt,
    corePromptDirty,
    corePromptError,
    corePromptLoading,
    corePromptSaved,
    corePromptSaving,
    setCorePrompt,
    saveCorePrompt,
    resetCorePrompt,
    restoreDefaultCorePrompt,
    defaultCorePrompt: DEFAULT_ASSISTANT_CORE_PROMPT,
    targetBoardId,
    setTargetBoardId,
    lastAssistantMessage,
    busy,
    sendStatus,
    lastRunId,
    lastRunStatus,
    lastError,
    send,
    openSession,
    resetToCanonicalLane,
    injectCorePrompt,
    sendLastAssistantToBoard,
  };
}
