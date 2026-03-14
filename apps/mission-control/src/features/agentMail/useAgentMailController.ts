import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ackAgentMailMessage,
  createAgentMailFileLease,
  createAgentMailThread,
  createMemoryNote,
  fetchAgentMailAttachmentBlob,
  getAgentMailThread,
  listAgentMailFileLeases,
  listAgentMailMessages,
  listAgentMailThreads,
  releaseAgentMailFileLease,
  sendAgentMailMessage,
  uploadAgentMailAttachment,
} from "../../lib/api";
import type { NotifyFn } from "../../app/useAppController";
import type {
  AgentMailFileLeaseResponse,
  AgentMailMessageResponse,
  AgentMailThreadDetailResponse,
  AgentMailThreadSummaryResponse,
  RuntimeConnectionSettings,
} from "../../types";
import { fileToBase64 } from "../../utils/files";
import { parsePrincipalCsv, truncateText } from "../../utils/text";
import { buildThreadSummaryNote } from "./agentMailSummary";

interface UseAgentMailControllerOptions {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
  setNotice: NotifyFn;
}

const REACTION_EMOJI_ALIASES: Record<string, string> = {
  ":+1:": "👍",
  ":-1:": "👎",
  ":eyes:": "👀",
  ":rocket:": "🚀",
  ":white_check_mark:": "✅",
  ":warning:": "⚠️",
  ":memo:": "📝",
  ":question:": "❓",
  ":sparkles:": "✨",
  ":fire:": "🔥",
  ":hourglass:": "⏳",
  ":beetle:": "🐞",
  ":heart:": "❤️",
  ":clap:": "👏",
  ":thinking:": "🤔",
  ":100:": "💯",
  ":raised_hands:": "🙌",
  ":x:": "❌",
};

export function normalizeReactionEmoji(emoji: string): string {
  const trimmed = emoji.trim();
  return REACTION_EMOJI_ALIASES[trimmed] ?? trimmed;
}

export function readThreadScopedFiles(
  filesByThreadId: Record<string, File[]>,
  threadId: string | null
): File[] {
  if (!threadId) {
    return [];
  }
  return filesByThreadId[threadId] ? [...filesByThreadId[threadId]] : [];
}

export function writeThreadScopedFiles(
  filesByThreadId: Record<string, File[]>,
  threadId: string | null,
  files: File[]
): Record<string, File[]> {
  if (!threadId) {
    return filesByThreadId;
  }
  if (files.length === 0) {
    if (!(threadId in filesByThreadId)) {
      return filesByThreadId;
    }
    const next = { ...filesByThreadId };
    delete next[threadId];
    return next;
  }
  return {
    ...filesByThreadId,
    [threadId]: [...files],
  };
}

export function useAgentMailController(options: UseAgentMailControllerOptions) {
  const { settings, tokenConfigured, setNotice } = options;

  const [mailboxFilter, setMailboxFilter] = useState<"all" | "inbox" | "outbox">("inbox");
  const [mailSearch, setMailSearch] = useState("");
  const [mailPrincipalOverride, setMailPrincipalOverride] = useState("");
  const [mailThreads, setMailThreads] = useState<AgentMailThreadSummaryResponse[]>([]);
  const [roomThreads, setRoomThreads] = useState<AgentMailThreadSummaryResponse[]>([]);
  const [selectedMailThreadId, setSelectedMailThreadIdState] = useState<string | null>(null);
  const [selectedRoomThreadId, setSelectedRoomThreadIdState] = useState<string | null>(null);
  const [mailThreadDetail, setMailThreadDetail] = useState<AgentMailThreadDetailResponse | null>(
    null
  );
  const [roomThreadDetail, setRoomThreadDetail] = useState<AgentMailThreadDetailResponse | null>(
    null
  );
  const [mailMessages, setMailMessages] = useState<AgentMailMessageResponse[]>([]);
  const [roomMessages, setRoomMessages] = useState<AgentMailMessageResponse[]>([]);
  const [newMailThreadSubject, setNewMailThreadSubject] = useState("");
  const [newMailThreadParticipants, setNewMailThreadParticipants] = useState("");
  const [newRoomName, setNewRoomName] = useState("");
  const [newRoomParticipants, setNewRoomParticipants] = useState("");
  const [mailComposeBody, setMailComposeBody] = useState("");
  const [mailComposeRecipients, setMailComposeRecipients] = useState("");
  const [mailComposeSender, setMailComposeSender] = useState("");
  const [chatComposeBody, setChatComposeBody] = useState("");
  const [chatComposeRecipients, setChatComposeRecipients] = useState("");
  const [chatComposeSender, setChatComposeSender] = useState("");
  const [mailAttachmentFilesByThreadId, setMailAttachmentFilesByThreadId] = useState<
    Record<string, File[]>
  >({});
  const [chatAttachmentFilesByThreadId, setChatAttachmentFilesByThreadId] = useState<
    Record<string, File[]>
  >({});
  const [leases, setLeases] = useState<AgentMailFileLeaseResponse[]>([]);
  const [leaseHolderPrincipal, setLeaseHolderPrincipal] = useState("");
  const [leaseGlobPattern, setLeaseGlobPattern] = useState("**/*");
  const [leaseTtlMs, setLeaseTtlMs] = useState("900000");
  const [leaseExclusive, setLeaseExclusive] = useState(false);
  const [leaseNote, setLeaseNote] = useState("");

  const agentMailRefreshTimer = useRef<number | null>(null);
  const mailThreadLoadSeq = useRef(0);
  const roomThreadLoadSeq = useRef(0);

  const loadMailThreadById = useCallback(
    async (
      threadId: string,
      runtimeSettings: RuntimeConnectionSettings = settings
    ): Promise<{
      detail: AgentMailThreadDetailResponse;
      messages: AgentMailMessageResponse[];
    }> => {
      const [detail, messages] = await Promise.all([
        getAgentMailThread(runtimeSettings, threadId),
        listAgentMailMessages(runtimeSettings, threadId, 500),
      ]);
      return {
        detail,
        messages: messages.items,
      };
    },
    [settings]
  );

  const loadAgentMailReadModels = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      const principalId = mailPrincipalOverride.trim() || undefined;
      const search = mailSearch.trim() || undefined;
      const [directThreads, roomThreadItems, activeLeases] = await Promise.all([
        listAgentMailThreads(runtimeSettings, {
          kind: "direct",
          mailbox: mailboxFilter,
          principalId,
          search,
          limit: 300,
        }),
        listAgentMailThreads(runtimeSettings, {
          kind: "room",
          mailbox: "all",
          principalId,
          search,
          limit: 300,
        }),
        listAgentMailFileLeases(runtimeSettings, {
          holderPrincipal: principalId,
          includeReleased: false,
        }),
      ]);
      setMailThreads(directThreads.items);
      setRoomThreads(roomThreadItems.items);
      setLeases(activeLeases);
    },
    [mailPrincipalOverride, mailSearch, mailboxFilter, settings]
  );

  const queueAgentMailRefresh = useCallback(
    (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (agentMailRefreshTimer.current) {
        globalThis.clearTimeout(agentMailRefreshTimer.current);
      }
      agentMailRefreshTimer.current = globalThis.setTimeout(() => {
        void loadAgentMailReadModels(runtimeSettings).catch((error: unknown) => {
          setNotice({
            tone: "error",
            message: `Agent Mail refresh failed: ${String(error)}`,
          });
        });
      }, 280);
    },
    [loadAgentMailReadModels, setNotice, settings]
  );

  const selectedMailThreadIdEffective = useMemo(() => {
    if (mailThreads.length === 0) {
      return null;
    }
    if (
      selectedMailThreadId &&
      mailThreads.some((item) => item.thread_id === selectedMailThreadId)
    ) {
      return selectedMailThreadId;
    }
    return mailThreads[0].thread_id;
  }, [mailThreads, selectedMailThreadId]);

  const selectedRoomThreadIdEffective = useMemo(() => {
    if (roomThreads.length === 0) {
      return null;
    }
    if (
      selectedRoomThreadId &&
      roomThreads.some((item) => item.thread_id === selectedRoomThreadId)
    ) {
      return selectedRoomThreadId;
    }
    return roomThreads[0].thread_id;
  }, [roomThreads, selectedRoomThreadId]);

  const setSelectedMailThreadId = useCallback((threadId: string | null) => {
    setSelectedMailThreadIdState(threadId);
  }, []);

  const setSelectedRoomThreadId = useCallback((threadId: string | null) => {
    setSelectedRoomThreadIdState(threadId);
  }, []);

  const mailAttachmentFiles = useMemo(
    () => readThreadScopedFiles(mailAttachmentFilesByThreadId, selectedMailThreadIdEffective),
    [mailAttachmentFilesByThreadId, selectedMailThreadIdEffective]
  );

  const setMailAttachmentFilesForThread = useCallback(
    (threadId: string | null, files: File[]) => {
      setMailAttachmentFilesByThreadId((current) =>
        writeThreadScopedFiles(current, threadId, files)
      );
    },
    []
  );

  const setMailAttachmentFiles = useCallback(
    (files: File[]) => {
      setMailAttachmentFilesForThread(selectedMailThreadIdEffective, files);
    },
    [selectedMailThreadIdEffective, setMailAttachmentFilesForThread]
  );

  const chatAttachmentFiles = useMemo(
    () => readThreadScopedFiles(chatAttachmentFilesByThreadId, selectedRoomThreadIdEffective),
    [chatAttachmentFilesByThreadId, selectedRoomThreadIdEffective]
  );

  const setChatAttachmentFilesForThread = useCallback(
    (threadId: string | null, files: File[]) => {
      setChatAttachmentFilesByThreadId((current) =>
        writeThreadScopedFiles(current, threadId, files)
      );
    },
    []
  );

  const setChatAttachmentFiles = useCallback(
    (files: File[]) => {
      setChatAttachmentFilesForThread(selectedRoomThreadIdEffective, files);
    },
    [selectedRoomThreadIdEffective, setChatAttachmentFilesForThread]
  );

  useEffect(() => {
    if (!settings.gateway_url.trim() || !tokenConfigured) {
      return;
    }
    queueAgentMailRefresh(settings);
  }, [
    mailboxFilter,
    mailPrincipalOverride,
    mailSearch,
    queueAgentMailRefresh,
    settings,
    tokenConfigured,
  ]);

  useEffect(() => {
    if (!selectedMailThreadIdEffective || !settings.gateway_url.trim() || !tokenConfigured) {
      mailThreadLoadSeq.current += 1;
      return;
    }
    const requestSeq = ++mailThreadLoadSeq.current;
    void loadMailThreadById(selectedMailThreadIdEffective, settings)
      .then(({ detail, messages }) => {
        if (requestSeq !== mailThreadLoadSeq.current) {
          return;
        }
        setMailThreadDetail(detail);
        setMailMessages(messages);
      })
      .catch((error: unknown) => {
        if (requestSeq !== mailThreadLoadSeq.current) {
          return;
        }
        setNotice({
          tone: "error",
          message: `Mail thread load failed: ${String(error)}`,
        });
      });
    return () => {
      mailThreadLoadSeq.current += 1;
    };
  }, [
    loadMailThreadById,
    selectedMailThreadIdEffective,
    setNotice,
    settings,
    tokenConfigured,
  ]);

  useEffect(() => {
    if (!selectedRoomThreadIdEffective || !settings.gateway_url.trim() || !tokenConfigured) {
      roomThreadLoadSeq.current += 1;
      return;
    }
    const requestSeq = ++roomThreadLoadSeq.current;
    void loadMailThreadById(selectedRoomThreadIdEffective, settings)
      .then(({ detail, messages }) => {
        if (requestSeq !== roomThreadLoadSeq.current) {
          return;
        }
        setRoomThreadDetail(detail);
        setRoomMessages(messages);
      })
      .catch((error: unknown) => {
        if (requestSeq !== roomThreadLoadSeq.current) {
          return;
        }
        setNotice({
          tone: "error",
          message: `Room thread load failed: ${String(error)}`,
        });
      });
    return () => {
      roomThreadLoadSeq.current += 1;
    };
  }, [
    loadMailThreadById,
    selectedRoomThreadIdEffective,
    setNotice,
    settings,
    tokenConfigured,
  ]);

  useEffect(() => {
    return () => {
      if (agentMailRefreshTimer.current) {
        globalThis.clearTimeout(agentMailRefreshTimer.current);
      }
    };
  }, []);

  const createMailThread = useCallback(
    async (kind: "direct" | "room") => {
      const subject = (kind === "room" ? newRoomName : newMailThreadSubject).trim();
      const participants = parsePrincipalCsv(
        kind === "room" ? newRoomParticipants : newMailThreadParticipants
      );
      if (!subject) {
        setNotice({
          tone: "error",
          message: kind === "room" ? "Room name is required." : "Thread subject is required.",
        });
        return false;
      }
      try {
        const created = await createAgentMailThread(settings, {
          kind,
          subject,
          participants,
        });
        if (kind === "room") {
          setNewRoomName("");
          setNewRoomParticipants("");
          setSelectedRoomThreadId(created.thread.thread_id);
        } else {
          setNewMailThreadSubject("");
          setNewMailThreadParticipants("");
          setSelectedMailThreadId(created.thread.thread_id);
        }
        setNotice({
          tone: "info",
          message: `${kind === "room" ? "Room" : "Thread"} created: ${created.thread.subject}`,
        });
        queueAgentMailRefresh(settings);
        return true;
      } catch (error) {
        setNotice({
          tone: "error",
          message: `${kind === "room" ? "Room" : "Thread"} create failed: ${String(error)}`,
        });
        return false;
      }
    },
    [
      newMailThreadParticipants,
      newMailThreadSubject,
      newRoomName,
      newRoomParticipants,
      queueAgentMailRefresh,
      setNotice,
      settings,
      setSelectedMailThreadId,
      setSelectedRoomThreadId,
    ]
  );

  const sendThreadMessage = useCallback(
    async (
      threadId: string,
      options: {
        body: string;
        recipientsCsv: string;
        senderPrincipal: string;
        files: File[];
        context: "mail" | "chat";
      }
    ) => {
      const body = options.body.trim();
      if (!body) {
        setNotice({ tone: "error", message: "Message body cannot be empty." });
        return;
      }
      try {
        const sent = await sendAgentMailMessage(settings, threadId, {
          body_text: body,
          sender_principal: options.senderPrincipal.trim() || undefined,
          sender_kind: "agent",
          recipients: parsePrincipalCsv(options.recipientsCsv),
        });
        const uploadResults = await Promise.allSettled(
          options.files.map(async (file) => {
            const contentBase64 = await fileToBase64(file);
            await uploadAgentMailAttachment(settings, sent.message.message_id, {
              filename: file.name,
              mime: file.type || "application/octet-stream",
              content_base64: contentBase64,
            });
          })
        );
        const failedUploads = uploadResults.filter(
          (result) => result.status === "rejected"
        ).length;
        if (options.context === "mail") {
          setMailComposeBody("");
          setMailComposeRecipients("");
          setMailAttachmentFilesForThread(threadId, []);
        } else {
          setChatComposeBody("");
          setChatComposeRecipients("");
          setChatAttachmentFilesForThread(threadId, []);
        }
        setNotice({
          tone: failedUploads > 0 ? "error" : "info",
          message:
            failedUploads > 0
              ? `Message sent, but ${failedUploads} attachment(s) failed to upload.`
              : options.files.length > 0
                ? "Message + attachments sent."
                : "Message sent.",
        });
        queueAgentMailRefresh(settings);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Send failed: ${String(error)}`,
        });
      }
    },
    [
      queueAgentMailRefresh,
      setChatAttachmentFilesForThread,
      setMailAttachmentFilesForThread,
      setNotice,
      settings,
    ]
  );

  const acknowledgeMessage = useCallback(
    async (messageId: string, recipientPrincipal?: string) => {
      try {
        await ackAgentMailMessage(settings, messageId, recipientPrincipal?.trim() || undefined);
        setNotice({
          tone: "info",
          message: "Message acknowledged.",
        });
        queueAgentMailRefresh(settings);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Acknowledge failed: ${String(error)}`,
        });
      }
    },
    [queueAgentMailRefresh, setNotice, settings]
  );

  const acknowledgeRoomUnread = useCallback(async () => {
    const principal = mailPrincipalOverride.trim();
    if (!principal) {
      setNotice({
        tone: "error",
        message: "Principal override is required to bulk-ack a room.",
      });
      return;
    }
    const pending = roomMessages.filter((message) =>
      message.recipients.some(
        (recipient) =>
          recipient.recipient_principal === principal && recipient.acked_at === null
      )
    );
    if (pending.length === 0) {
      setNotice({
        tone: "info",
        message: "No unread room messages for that principal.",
      });
      return;
    }
    const results = await Promise.allSettled(
      pending.map((message) => ackAgentMailMessage(settings, message.message_id, principal))
    );
    const failedCount = results.filter((result) => result.status === "rejected").length;
    const successCount = pending.length - failedCount;
    setNotice({
      tone: failedCount > 0 ? "error" : "info",
      message:
        failedCount > 0
          ? `Acknowledged ${successCount}/${pending.length} room message(s).`
          : `Acknowledged ${pending.length} room message(s).`,
    });
    if (successCount > 0) {
      queueAgentMailRefresh(settings);
    }
  }, [mailPrincipalOverride, queueAgentMailRefresh, roomMessages, setNotice, settings]);

  const postRoomReaction = useCallback(
    async (emoji: string) => {
      if (!selectedRoomThreadIdEffective) {
        return;
      }
      await sendThreadMessage(selectedRoomThreadIdEffective, {
        body: `reaction ${normalizeReactionEmoji(emoji)}`,
        recipientsCsv: "",
        senderPrincipal: chatComposeSender,
        files: [],
        context: "chat",
      });
    },
    [chatComposeSender, selectedRoomThreadIdEffective, sendThreadMessage]
  );

  const summarizeSelectedMailThread = useCallback(async () => {
    if (!mailThreadDetail || mailMessages.length === 0) {
      setNotice({
        tone: "error",
        message: "Select a populated thread before summarizing.",
      });
      return;
    }
    try {
      const summaryBody = buildThreadSummaryNote(mailThreadDetail, mailMessages);
      const created = await createMemoryNote(settings, {
        title: `Agent Mail Summary: ${truncateText(mailThreadDetail.thread.subject, 80)}`,
        body: summaryBody,
        tags: ["agent_mail", "mission_control", "thread_summary"],
      });
      setNotice({
        tone: "info",
        message: `Thread summary stored as note ${created.note.note_id}.`,
      });
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Summarize failed: ${String(error)}`,
      });
    }
  }, [mailMessages, mailThreadDetail, setNotice, settings]);

  const downloadMailAttachment = useCallback(
    async (messageId: string, attachmentId: string, filename: string) => {
      try {
        const blob = await fetchAgentMailAttachmentBlob(settings, messageId, attachmentId);
        const objectUrl = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = objectUrl;
        anchor.download = filename;
        document.body.appendChild(anchor);
        anchor.click();
        document.body.removeChild(anchor);
        URL.revokeObjectURL(objectUrl);
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Attachment download failed: ${String(error)}`,
        });
      }
    },
    [setNotice, settings]
  );

  const createFileLease = useCallback(async () => {
    const ttl = Number(leaseTtlMs);
    if (!Number.isFinite(ttl) || ttl <= 0 || !Number.isInteger(ttl)) {
      setNotice({
        tone: "error",
        message: "Lease TTL must be a positive integer number of milliseconds.",
      });
      return false;
    }
    const globPattern = leaseGlobPattern.trim();
    if (!globPattern) {
      setNotice({
        tone: "error",
        message: "Lease glob pattern is required.",
      });
      return false;
    }
    try {
      const created = await createAgentMailFileLease(settings, {
        holder_principal: leaseHolderPrincipal.trim() || undefined,
        glob_pattern: globPattern,
        exclusive: leaseExclusive,
        ttl_ms: ttl,
        note: leaseNote.trim() || undefined,
      });
      setNotice({
        tone: "info",
        message: `Lease created: ${created.lease.glob_pattern}`,
      });
      setLeaseNote("");
      queueAgentMailRefresh(settings);
      return true;
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Lease create failed: ${String(error)}`,
      });
      return false;
    }
  }, [
    leaseExclusive,
    leaseGlobPattern,
    leaseHolderPrincipal,
    leaseNote,
    leaseTtlMs,
    queueAgentMailRefresh,
    setNotice,
    settings,
  ]);

  const releaseFileLease = useCallback(
    async (leaseId: string) => {
      try {
        await releaseAgentMailFileLease(
          settings,
          leaseId,
          leaseHolderPrincipal.trim() || undefined
        );
        setNotice({
          tone: "info",
          message: "Lease released.",
        });
        queueAgentMailRefresh(settings);
        return true;
      } catch (error) {
        setNotice({
          tone: "error",
          message: `Lease release failed: ${String(error)}`,
        });
        return false;
      }
    },
    [leaseHolderPrincipal, queueAgentMailRefresh, setNotice, settings]
  );

  const reserveSelectedRoomWorkspace = useCallback(async () => {
    if (!selectedRoomThreadIdEffective) {
      return;
    }
    try {
      await createAgentMailFileLease(settings, {
        holder_principal: leaseHolderPrincipal.trim() || undefined,
        glob_pattern: `chatrooms/${selectedRoomThreadIdEffective}/**`,
        exclusive: true,
        ttl_ms: 900_000,
        note: "room moderation reserve",
      });
      setNotice({
        tone: "info",
        message: "Room workspace lease reserved for 15m.",
      });
      queueAgentMailRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Room lease reserve failed: ${String(error)}`,
      });
    }
  }, [
    leaseHolderPrincipal,
    queueAgentMailRefresh,
    selectedRoomThreadIdEffective,
    setNotice,
    settings,
  ]);

  const effectiveMailThreadDetail = useMemo(() => {
    if (!selectedMailThreadIdEffective || !settings.gateway_url.trim() || !tokenConfigured) {
      return null;
    }
    return mailThreadDetail;
  }, [
    mailThreadDetail,
    selectedMailThreadIdEffective,
    settings,
    tokenConfigured,
  ]);

  const effectiveMailMessages = useMemo(() => {
    if (!selectedMailThreadIdEffective || !settings.gateway_url.trim() || !tokenConfigured) {
      return [] as AgentMailMessageResponse[];
    }
    return mailMessages;
  }, [mailMessages, selectedMailThreadIdEffective, settings, tokenConfigured]);

  const effectiveRoomThreadDetail = useMemo(() => {
    if (!selectedRoomThreadIdEffective || !settings.gateway_url.trim() || !tokenConfigured) {
      return null;
    }
    return roomThreadDetail;
  }, [
    roomThreadDetail,
    selectedRoomThreadIdEffective,
    settings,
    tokenConfigured,
  ]);

  const effectiveRoomMessages = useMemo(() => {
    if (!selectedRoomThreadIdEffective || !settings.gateway_url.trim() || !tokenConfigured) {
      return [] as AgentMailMessageResponse[];
    }
    return roomMessages;
  }, [roomMessages, selectedRoomThreadIdEffective, settings, tokenConfigured]);

  return {
    mailboxFilter,
    setMailboxFilter,
    mailSearch,
    setMailSearch,
    mailPrincipalOverride,
    setMailPrincipalOverride,
    mailThreads,
    roomThreads,
    selectedMailThreadId: selectedMailThreadIdEffective,
    setSelectedMailThreadId,
    selectedRoomThreadId: selectedRoomThreadIdEffective,
    setSelectedRoomThreadId,
    mailThreadDetail: effectiveMailThreadDetail,
    roomThreadDetail: effectiveRoomThreadDetail,
    mailMessages: effectiveMailMessages,
    roomMessages: effectiveRoomMessages,
    newMailThreadSubject,
    setNewMailThreadSubject,
    newMailThreadParticipants,
    setNewMailThreadParticipants,
    newRoomName,
    setNewRoomName,
    newRoomParticipants,
    setNewRoomParticipants,
    mailComposeBody,
    setMailComposeBody,
    mailComposeRecipients,
    setMailComposeRecipients,
    mailComposeSender,
    setMailComposeSender,
    chatComposeBody,
    setChatComposeBody,
    chatComposeRecipients,
    setChatComposeRecipients,
    chatComposeSender,
    setChatComposeSender,
    mailAttachmentFiles,
    setMailAttachmentFiles,
    chatAttachmentFiles,
    setChatAttachmentFiles,
    leases,
    leaseHolderPrincipal,
    setLeaseHolderPrincipal,
    leaseGlobPattern,
    setLeaseGlobPattern,
    leaseTtlMs,
    setLeaseTtlMs,
    leaseExclusive,
    setLeaseExclusive,
    leaseNote,
    setLeaseNote,
    loadAgentMailReadModels,
    queueAgentMailRefresh,
    createMailThread,
    sendThreadMessage,
    acknowledgeMessage,
    acknowledgeRoomUnread,
    postRoomReaction,
    summarizeSelectedMailThread,
    downloadMailAttachment,
    createFileLease,
    releaseFileLease,
    reserveSelectedRoomWorkspace,
  };
}
