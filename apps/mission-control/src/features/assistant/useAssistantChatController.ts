import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createBoardCard,
  createSession,
  createSessionMessage,
  createSessionRun,
  getAgentProviderProfileOrder,
  getBoard,
  listAuthProfiles,
  listProviderCapabilities,
  listProviderModels,
  listSessionMessages,
  revokeAuthProfile,
  setAgentProviderProfileOrder,
} from "../../lib/api";
import type { BoardSummary } from "../../app/useRuntimeConnectionController";
import { STORAGE_KEYS } from "../../storageKeys";
import type { NotifyFn } from "../../app/useAppController";
import { providerLabel } from "../../lib/providerCatalog";
import type {
  Agent,
  AuthProfileResponse,
  MessageResponse,
  RuntimeConnectionSettings,
} from "../../types";

interface UseAssistantChatControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  agents: Agent[];
  authProfiles: AuthProfileResponse[];
  boards: BoardSummary[];
  refreshAuthProfiles?: () => Promise<void>;
  setNotice: NotifyFn;
}

const DEFAULT_CORE_PROMPT = `You are the CarsinOS assistant.\n\nGoals:\n1) Help the operator complete tasks safely and quickly.\n2) Prefer clear plans and explicit next actions.\n3) Keep execution grounded in current system state.\n\nOperational rules:\n- Ask concise clarifying questions only when required.\n- Prefer reversible actions before risky actions.\n- When uncertain, state assumptions briefly.\n- Use Mission Control tabs intentionally: Boards for execution, Calendar for scheduling, Focus for incidents, Mail/Rooms for communication, Team for agent config.\n`;

function loadCorePrompt(): string {
  try {
    const value = localStorage.getItem(STORAGE_KEYS.assistantCorePromptV1);
    return value && value.trim().length > 0 ? value : DEFAULT_CORE_PROMPT;
  } catch {
    return DEFAULT_CORE_PROMPT;
  }
}

function normalizeMessages(items: MessageResponse[]): MessageResponse[] {
  return [...items].sort((left, right) => left.created_at - right.created_at);
}

function providerRequiresAuth(provider: string): boolean {
  const normalized = provider.trim().toLowerCase();
  return normalized === "openai" || normalized === "anthropic" || normalized === "openrouter";
}

function parseApiError(error: unknown): { code: string | null; message: string } {
  const raw = String(error);
  const jsonStart = raw.indexOf("{");
  if (jsonStart === -1) {
    return { code: null, message: raw };
  }
  try {
    const payload = JSON.parse(raw.slice(jsonStart)) as {
      error?: unknown;
      error_code?: unknown;
    };
    const code = typeof payload.error_code === "string" ? payload.error_code : null;
    const message =
      typeof payload.error === "string" && payload.error.trim().length > 0
        ? payload.error
        : raw;
    return { code, message };
  } catch {
    return { code: null, message: raw };
  }
}

export function useAssistantChatController(options: UseAssistantChatControllerOptions) {
  const { settings, tokenConfigured, agents, authProfiles, refreshAuthProfiles, setNotice } =
    options;

  const [selectedAgentId, setSelectedAgentId] = useState(agents[0]?.agent_id ?? "");
  const [modelProvider, setModelProvider] = useState(agents[0]?.model_provider ?? "");
  const [modelId, setModelId] = useState(agents[0]?.model_id ?? "");
  const [authProfileId, setAuthProfileId] = useState("");
  const [providerOptions, setProviderOptions] = useState<Array<{ value: string; label: string }>>(
    []
  );
  const [authProfileOrder, setAuthProfileOrder] = useState<string[]>([]);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [savingAuthProfileRoute, setSavingAuthProfileRoute] = useState(false);
  const [clearingAllProfiles, setClearingAllProfiles] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<MessageResponse[]>([]);
  const [draft, setDraft] = useState("");
  const [corePrompt, setCorePrompt] = useState(loadCorePrompt);
  const [targetBoardId, setTargetBoardId] = useState(options.boards[0]?.board_id ?? "");
  const [busy, setBusy] = useState(false);
  const [lastRunStatus, setLastRunStatus] = useState<string | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);

  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.agent_id === selectedAgentId) ?? null,
    [agents, selectedAgentId]
  );
  const lastAssistantMessage = useMemo(
    () => [...messages].reverse().find((item) => item.role === "assistant") ?? null,
    [messages]
  );
  const authProfileOptions = useMemo(() => {
    if (!modelProvider.trim()) {
      return [] as AuthProfileResponse[];
    }
    const provider = modelProvider.trim().toLowerCase();
    const orderIndex = new Map(authProfileOrder.map((profileId, index) => [profileId, index]));
    return authProfiles
      .filter((profile) => profile.provider.toLowerCase() === provider && profile.enabled)
      .sort((left, right) => {
        const leftOrder = orderIndex.get(left.auth_profile_id);
        const rightOrder = orderIndex.get(right.auth_profile_id);
        if (leftOrder !== undefined && rightOrder !== undefined) {
          return leftOrder - rightOrder;
        }
        if (leftOrder !== undefined) {
          return -1;
        }
        if (rightOrder !== undefined) {
          return 1;
        }
        return left.display_name.localeCompare(right.display_name);
      });
  }, [authProfiles, authProfileOrder, modelProvider]);

  useEffect(() => {
    if (agents.length === 0) {
      setSelectedAgentId("");
      return;
    }
    if (!selectedAgentId || !agents.some((agent) => agent.agent_id === selectedAgentId)) {
      setSelectedAgentId(agents[0].agent_id);
    }
  }, [agents, selectedAgentId]);

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
    setSessionId(null);
    setMessages([]);
    setLastRunStatus(null);
    setLastError(null);
  }, [selectedAgent]);

  useEffect(() => {
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      setProviderOptions((previous) => {
        if (previous.length > 0) {
          return previous;
        }
        const fallback = [...new Set(agents.map((item) => item.model_provider).filter(Boolean))]
          .sort((left, right) => left.localeCompare(right))
          .map((provider) => ({ value: provider, label: providerLabel(provider) }));
        return fallback;
      });
      return;
    }
    let cancelled = false;
    void listProviderCapabilities(settings)
      .then((response) => {
        if (cancelled) {
          return;
        }
        const discovered = response.items
          .map((item) => item.provider.trim())
          .filter((provider) => provider.length > 0);
        const mergedProviders = new Set([
          ...discovered,
          ...agents.map((item) => item.model_provider).filter(Boolean),
        ]);
        const options = [...mergedProviders]
          .sort((left, right) => left.localeCompare(right))
          .map((provider) => ({
            value: provider,
            label: providerLabel(provider),
          }));
        setProviderOptions(options);
      })
      .catch(() => {
        if (cancelled) {
          return;
        }
        const fallback = [...new Set(agents.map((item) => item.model_provider).filter(Boolean))]
          .sort((left, right) => left.localeCompare(right))
          .map((provider) => ({ value: provider, label: providerLabel(provider) }));
        setProviderOptions(fallback);
      });
    return () => {
      cancelled = true;
    };
  }, [agents, settings, tokenConfigured]);

  useEffect(() => {
    if (providerOptions.length === 0) {
      return;
    }
    if (providerOptions.some((option) => option.value === modelProvider)) {
      return;
    }
    const preferred =
      selectedAgent?.model_provider &&
      providerOptions.some((option) => option.value === selectedAgent.model_provider)
        ? selectedAgent.model_provider
        : providerOptions[0].value;
    setModelProvider(preferred);
  }, [modelProvider, providerOptions, selectedAgent]);

  useEffect(() => {
    const provider = modelProvider.trim().toLowerCase();
    const agentId = selectedAgentId.trim();
    if (!provider || !agentId || !tokenConfigured || !settings.gateway_url.trim()) {
      setAuthProfileOrder([]);
      return;
    }
    let cancelled = false;
    void getAgentProviderProfileOrder(settings, agentId, provider)
      .then((response) => {
        if (cancelled) {
          return;
        }
        setAuthProfileOrder(response.profile_ids);
      })
      .catch(() => {
        if (cancelled) {
          return;
        }
        setAuthProfileOrder([]);
      });
    return () => {
      cancelled = true;
    };
  }, [modelProvider, selectedAgentId, settings, tokenConfigured]);

  useEffect(() => {
    const provider = modelProvider.trim();
    if (authProfileOptions.length === 0) {
      setAuthProfileId("");
      return;
    }
    if (authProfileId && authProfileOptions.some((item) => item.auth_profile_id === authProfileId)) {
      return;
    }
    if (providerRequiresAuth(provider)) {
      setAuthProfileId(authProfileOptions[0].auth_profile_id);
      return;
    }
    if (authProfileId) {
      setAuthProfileId("");
    }
  }, [authProfileId, authProfileOptions, modelProvider]);

  useEffect(() => {
    if (!selectedAgent) {
      return;
    }
    if (!selectedAgent.model_provider || selectedAgent.model_provider !== modelProvider) {
      return;
    }
    if (!selectedAgent.model_id) {
      return;
    }
    if (modelId || modelOptions.length === 0) {
      return;
    }
    if (modelOptions.includes(selectedAgent.model_id)) {
      setModelId(selectedAgent.model_id);
    }
  }, [modelId, modelOptions, modelProvider, selectedAgent]);

  useEffect(() => {
    if (!modelProvider.trim() || modelId) {
      return;
    }
    if (modelOptions.length > 0) {
      setModelId(modelOptions[0]);
    }
  }, [modelId, modelOptions, modelProvider]);

  useEffect(() => {
    const provider = modelProvider.trim();
    const providerName = providerLabel(provider);
    const providerNeedsAuth = providerRequiresAuth(provider);
    if (!provider) {
      setModelOptions([]);
      setModelsError("Select a model provider.");
      setModelsLoading(false);
      return;
    }
    if (providerNeedsAuth && authProfileOptions.length === 0) {
      setModelOptions([]);
      setModelId("");
      setModelsError(
        `No eligible ${providerName} auth profiles are available. Add one, then save it for this agent.`
      );
      setModelsLoading(false);
      return;
    }
    if (providerNeedsAuth && !authProfileId.trim()) {
      setModelOptions([]);
      setModelId("");
      setModelsError(`Select a ${providerName} auth profile to load model choices.`);
      setModelsLoading(false);
      return;
    }
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      const fallback = selectedAgent?.model_provider === provider && selectedAgent.model_id
        ? [selectedAgent.model_id]
        : [];
      setModelOptions(fallback);
      setModelsError(null);
      setModelsLoading(false);
      if (fallback.length > 0) {
        setModelId((current) => (current ? current : fallback[0]));
      }
      return;
    }

    let cancelled = false;
    setModelsLoading(true);
    setModelsError(null);
    void listProviderModels(settings, {
      provider,
      agent_id: selectedAgentId || undefined,
      auth_profile_id: authProfileId || undefined,
    })
      .then((response) => {
        if (cancelled) {
          return;
        }
        const models = response.items.map((item) => item.model_id);
        setModelOptions(models);
        if (models.length === 0) {
          setModelId("");
          setModelsError(`No models reported for ${providerName}.`);
          return;
        }
        setModelId((current) => {
          if (current && models.includes(current)) {
            return current;
          }
          if (
            selectedAgent?.model_provider === provider &&
            selectedAgent.model_id &&
            models.includes(selectedAgent.model_id)
          ) {
            return selectedAgent.model_id;
          }
          return models[0];
        });
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        const parsed = parseApiError(error);
        const fallback = selectedAgent?.model_provider === provider && selectedAgent.model_id
          ? [selectedAgent.model_id]
          : [];
        setModelOptions(fallback);
        if (fallback.length > 0) {
          setModelId((current) => (current && fallback.includes(current) ? current : fallback[0]));
        }
        if (parsed.code === "AUTH_FORBIDDEN") {
          setModelsError(
            authProfileId
              ? `Selected ${providerName} profile is blocked by policy. Pick another profile or un-revoke it in Security.`
              : `${providerName} auth policy blocked model discovery for this agent.`
          );
          return;
        }
        if (parsed.code === "AUTH_REQUIRED") {
          setModelsError(
            authProfileOptions.length > 0
              ? `Choose an eligible ${providerName} auth profile, then retry model discovery.`
              : `No eligible ${providerName} auth profiles are available.`
          );
          return;
        }
        if (fallback.length > 0) {
          setModelsError(null);
        } else {
          setModelsError(parsed.message);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setModelsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    authProfileId,
    authProfileOptions.length,
    modelProvider,
    selectedAgent,
    selectedAgentId,
    settings,
    tokenConfigured,
  ]);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEYS.assistantCorePromptV1, corePrompt);
    } catch {
      // ignore storage failures
    }
  }, [corePrompt]);

  const refreshMessages = useCallback(
    async (id: string) => {
      const response = await listSessionMessages(settings, id, 300);
      setMessages(normalizeMessages(response.items));
    },
    [settings]
  );

  const ensureSession = useCallback(async () => {
    if (!selectedAgentId.trim()) {
      throw new Error("Select an agent first.");
    }
    if (sessionId) {
      return sessionId;
    }
    const response = await createSession(settings, {
      agent_id: selectedAgentId,
      title: `Assistant chat (${selectedAgentId})`,
    });
    const id = response.session.session_id;
    setSessionId(id);

    if (corePrompt.trim()) {
      await createSessionMessage(settings, id, {
        role: "system",
        content_text: corePrompt.trim(),
        content_format: "markdown",
        source_channel: "assistant-chat",
      });
    }

    await refreshMessages(id);
    return id;
  }, [corePrompt, refreshMessages, selectedAgentId, sessionId, settings]);

  const startNewChat = useCallback(() => {
    setSessionId(null);
    setMessages([]);
    setLastRunStatus(null);
    setLastError(null);
  }, []);

  const injectCorePrompt = useCallback(async () => {
    const prompt = corePrompt.trim();
    if (!prompt) {
      setNotice({ tone: "error", message: "Core prompt is empty." });
      return;
    }
    setBusy(true);
    setLastError(null);
    try {
      const id = await ensureSession();
      await createSessionMessage(settings, id, {
        role: "system",
        content_text: prompt,
        content_format: "markdown",
        source_channel: "assistant-chat",
      });
      await refreshMessages(id);
      setNotice({ tone: "info", message: "Core prompt inserted into current session." });
    } catch (error: unknown) {
      const text = String(error);
      setLastError(text);
      setNotice({ tone: "error", message: `Prompt injection failed: ${text}` });
    } finally {
      setBusy(false);
    }
  }, [corePrompt, ensureSession, refreshMessages, setNotice, settings]);

  const saveAuthProfileSelection = useCallback(async () => {
    const agentId = selectedAgentId.trim();
    const provider = modelProvider.trim().toLowerCase();
    const profileId = authProfileId.trim();
    if (!agentId || !provider || !profileId) {
      setNotice({ tone: "error", message: "Select agent, provider, and auth profile first." });
      return;
    }
    setSavingAuthProfileRoute(true);
    try {
      const existing = await getAgentProviderProfileOrder(settings, agentId, provider);
      const nextOrder = [
        profileId,
        ...existing.profile_ids.filter((candidate) => candidate !== profileId),
      ];
      await setAgentProviderProfileOrder(settings, agentId, provider, nextOrder);
      setAuthProfileOrder(nextOrder);
      setNotice({
        tone: "info",
        message: `Saved ${providerLabel(provider)} profile routing for agent ${agentId}.`,
      });
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Profile save failed: ${String(error)}`,
      });
    } finally {
      setSavingAuthProfileRoute(false);
    }
  }, [authProfileId, modelProvider, selectedAgentId, setNotice, settings]);

  const clearAllAssistantProfiles = useCallback(async () => {
    if (!tokenConfigured) {
      setNotice({ tone: "error", message: "Gateway token is missing." });
      return;
    }
    const shouldProceed =
      typeof window === "undefined"
        ? true
        : window.confirm(
            "Clear all assistant auth profiles? This revokes tokens and disables every profile."
          );
    if (!shouldProceed) {
      return;
    }
    setClearingAllProfiles(true);
    try {
      const profiles = await listAuthProfiles(settings, { includeDisabled: true });
      if (profiles.length === 0) {
        setNotice({ tone: "info", message: "No assistant profiles found to clear." });
        return;
      }
      const results = await Promise.allSettled(
        profiles.map((profile) =>
          revokeAuthProfile(settings, profile.auth_profile_id, {
            reason: "assistant profile reset",
            remove_secret: true,
            disable_profile: true,
            kill_switch_scope: "profile",
          })
        )
      );
      const failed = results.filter((result) => result.status === "rejected");
      if (failed.length > 0) {
        const firstFailure = String((failed[0] as PromiseRejectedResult).reason);
        setNotice({
          tone: "error",
          message: `Cleared ${profiles.length - failed.length}/${profiles.length} profiles. First error: ${firstFailure}`,
        });
      } else {
        setNotice({
          tone: "info",
          message: `Cleared ${profiles.length} assistant profile${
            profiles.length === 1 ? "" : "s"
          }.`,
        });
      }
      setAuthProfileId("");
      setAuthProfileOrder([]);
      setModelId("");
      setModelOptions([]);
      setModelsError(null);
      if (refreshAuthProfiles) {
        await refreshAuthProfiles();
      }
    } catch (error: unknown) {
      setNotice({
        tone: "error",
        message: `Clear profiles failed: ${String(error)}`,
      });
    } finally {
      setClearingAllProfiles(false);
    }
  }, [refreshAuthProfiles, setNotice, settings, tokenConfigured]);

  const send = useCallback(async () => {
    const body = draft.trim();
    if (!body) {
      return;
    }
    if (!tokenConfigured) {
      setNotice({ tone: "error", message: "Gateway token is missing." });
      return;
    }
    if (!modelProvider.trim() || !modelId.trim()) {
      setNotice({ tone: "error", message: "Model provider and model are required." });
      return;
    }
    if (providerRequiresAuth(modelProvider) && !authProfileId.trim()) {
      setNotice({
        tone: "error",
        message: `${providerLabel(modelProvider)} requires an auth profile. Select one first.`,
      });
      return;
    }

    setBusy(true);
    setLastError(null);
    setLastRunStatus(null);
    try {
      const id = await ensureSession();
      await createSessionMessage(settings, id, {
        role: "user",
        content_text: body,
        content_format: "markdown",
        source_channel: "assistant-chat",
      });
      const run = await createSessionRun(settings, id, {
        model_provider: modelProvider.trim(),
        model_id: modelId.trim(),
        auth_profile_id: authProfileId.trim() || undefined,
      });
      setLastRunStatus(run.run.status);
      await refreshMessages(id);
      setDraft("");
      if (run.run.status !== "succeeded") {
        setLastError(run.run.error_text ?? `Run ended with status ${run.run.status}`);
      }
    } catch (error: unknown) {
      const text = String(error);
      setLastError(text);
      setNotice({ tone: "error", message: `Assistant run failed: ${text}` });
    } finally {
      setBusy(false);
    }
  }, [
    authProfileId,
    draft,
    ensureSession,
    modelId,
    modelProvider,
    refreshMessages,
    setNotice,
    settings,
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
        owner_kind: selectedAgentId ? "agent" : "unassigned",
        owner_agent_id: selectedAgentId || undefined,
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
  }, [lastAssistantMessage, selectedAgentId, setNotice, settings, targetBoardId]);

  return {
    selectedAgentId,
    setSelectedAgentId,
    selectedAgent,
    modelProvider,
    setModelProvider,
    providerOptions,
    modelId,
    setModelId,
    modelOptions,
    modelsLoading,
    modelsError,
    authProfileId,
    setAuthProfileId,
    authProfileOptions,
    savingAuthProfileRoute,
    clearingAllProfiles,
    sessionId,
    messages,
    draft,
    setDraft,
    corePrompt,
    setCorePrompt,
    targetBoardId,
    setTargetBoardId,
    lastAssistantMessage,
    busy,
    lastRunStatus,
    lastError,
    send,
    saveAuthProfileSelection,
    clearAllAssistantProfiles,
    startNewChat,
    injectCorePrompt,
    sendLastAssistantToBoard,
  };
}
