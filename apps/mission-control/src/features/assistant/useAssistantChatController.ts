import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createBoardCard,
  createSession,
  createSessionMessage,
  createSessionRun,
  getBoard,
  listSessionMessages,
} from "../../lib/api";
import type { BoardSummary } from "../../app/useRuntimeConnectionController";
import { STORAGE_KEYS } from "../../storageKeys";
import type { NotifyFn } from "../../app/useAppController";
import type { Agent, MessageResponse, RuntimeConnectionSettings } from "../../types";

interface UseAssistantChatControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  agents: Agent[];
  boards: BoardSummary[];
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

export function useAssistantChatController(options: UseAssistantChatControllerOptions) {
  const { settings, tokenConfigured, agents, setNotice } = options;

  const [selectedAgentId, setSelectedAgentId] = useState(agents[0]?.agent_id ?? "");
  const [modelProvider, setModelProvider] = useState(agents[0]?.model_provider ?? "");
  const [modelId, setModelId] = useState(agents[0]?.model_id ?? "");
  const [authProfileId, setAuthProfileId] = useState("");
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
      const firstColumn = [...board.columns].sort((left, right) => left.position - right.position)[0];
      if (!firstColumn) {
        throw new Error("Board has no columns.");
      }
      const normalized = lastAssistantMessage.content_text.trim();
      const firstLine =
        normalized.split(/\\r?\\n/).map((line) => line.trim()).find((line) => line.length > 0) ??
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
    modelId,
    setModelId,
    authProfileId,
    setAuthProfileId,
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
    startNewChat,
    injectCorePrompt,
    sendLastAssistantToBoard,
  };
}
