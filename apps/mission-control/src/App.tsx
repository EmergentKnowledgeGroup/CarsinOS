import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import clsx from "clsx";
import {
  createBoardCard,
  fetchBoardCardAssetBlob,
  getChannelRuntimeStatus,
  getBoard,
  getMissionControlCalendarWeek,
  getMissionControlFocus,
  getGatewayHealth,
  listAgents,
  listBoards,
  listJobs,
  listApprovals,
  moveBoardCard,
  reconnectChannelRuntime,
  resolveApproval,
  runJobNow,
  runBoardCard,
  setJobEnabledState,
  updateBoardCard,
  uploadBoardCardAsset,
} from "./lib/api";
import {
  clearGatewayToken,
  isGatewayTokenConfigured,
  loadConnectionSettings,
  persistConnectionSettings,
  setGatewayToken,
} from "./lib/runtime";
import { connectGatewayEvents, type WsLifecycleState } from "./lib/ws";
import type {
  Agent,
  BoardCard,
  BoardColumn,
  BoardDetail,
  ChannelRuntimeAdapterStatusResponse,
  MissionControlCalendarJob,
  MissionControlCalendarWeekResponse,
  MissionControlFocusItem,
  RuntimeConnectionSettings,
  WsEventFrame,
} from "./types";
import "./styles.css";

interface Notice {
  tone: "info" | "error" | "critical";
  message: string;
}

type MissionControlTab = "boards" | "calendar" | "focus" | "events";

interface EventStreamItem {
  event_id: string;
  event_type: string;
  entity: string;
  ts_unix_ms: number;
  payload: Record<string, unknown>;
}

interface CardEditorDraft {
  title: string;
  description: string;
  ownerKind: string;
  ownerAgentId: string;
  ownerHumanId: string;
  dueAt: string;
  tagsCsv: string;
  scriptMarkdown: string;
}

function emptyEditorDraft(): CardEditorDraft {
  return {
    title: "",
    description: "",
    ownerKind: "unassigned",
    ownerAgentId: "",
    ownerHumanId: "",
    dueAt: "",
    tagsCsv: "",
    scriptMarkdown: "",
  };
}

function toCardsByColumn(board: BoardDetail | null): Map<string, BoardCard[]> {
  const map = new Map<string, BoardCard[]>();
  if (!board) {
    return map;
  }
  for (const column of board.columns) {
    map.set(column.column_id, []);
  }
  for (const card of board.cards) {
    if (!map.has(card.column_id)) {
      map.set(card.column_id, []);
    }
    map.get(card.column_id)?.push(card);
  }
  for (const list of map.values()) {
    list.sort((a, b) => a.position - b.position);
  }
  return map;
}

function withUpsertCard(board: BoardDetail, nextCard: BoardCard): BoardDetail {
  const cards = board.cards.filter((card) => card.card_id !== nextCard.card_id);
  cards.push(nextCard);
  cards.sort((a, b) => a.position - b.position);
  return {
    ...board,
    cards,
  };
}

function withOptimisticMove(
  board: BoardDetail,
  cardId: string,
  targetColumnId: string,
  beforeCardId?: string
): BoardDetail {
  const columns = board.columns.map((column) => column.column_id);
  const grouped = toCardsByColumn(board);
  const movingCard = board.cards.find((card) => card.card_id === cardId);
  if (!movingCard) {
    return board;
  }

  for (const list of grouped.values()) {
    const index = list.findIndex((card) => card.card_id === cardId);
    if (index >= 0) {
      list.splice(index, 1);
      break;
    }
  }

  const targetList = grouped.get(targetColumnId) ?? [];
  const beforeIndex =
    beforeCardId === undefined
      ? -1
      : targetList.findIndex((card) => card.card_id === beforeCardId);
  const insertIndex =
    beforeCardId === undefined || beforeIndex < 0 ? targetList.length : beforeIndex;

  const nextCard: BoardCard = {
    ...movingCard,
    column_id: targetColumnId,
  };
  if (insertIndex >= targetList.length) {
    targetList.push(nextCard);
  } else {
    targetList.splice(insertIndex, 0, nextCard);
  }
  grouped.set(targetColumnId, targetList);

  const nextCards: BoardCard[] = [];
  for (const columnId of columns) {
    const list = grouped.get(columnId) ?? [];
    list.forEach((card, idx) => {
      nextCards.push({
        ...card,
        position: idx,
      });
    });
  }

  return {
    ...board,
    cards: nextCards,
  };
}

function toInputDateTimeValue(unixMs: number | null): string {
  if (unixMs === null || unixMs === undefined) {
    return "";
  }
  const date = new Date(unixMs);
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

function fromInputDateTimeValue(value: string): number | null {
  if (!value.trim()) {
    return null;
  }
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? null : parsed;
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string") {
        reject(new Error("failed to read file"));
        return;
      }
      const marker = "base64,";
      const index = result.indexOf(marker);
      if (index < 0) {
        reject(new Error("unexpected file encoding"));
        return;
      }
      resolve(result.slice(index + marker.length));
    };
    reader.onerror = () => reject(new Error("failed to read file"));
    reader.readAsDataURL(file);
  });
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes}B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)}KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
}

function formatDateTime(unixMs: number | null | undefined): string {
  if (!unixMs) {
    return "n/a";
  }
  return new Date(unixMs).toLocaleString();
}

function BoardLane(props: {
  column: BoardColumn;
  cards: BoardCard[];
  selectedCardId: string | null;
  dragCardId: string | null;
  setDragCardId: (value: string | null) => void;
  onSelectCard: (cardId: string) => void;
  onDropCard: (cardId: string, columnId: string, beforeCardId?: string) => void;
  onCreateCard: (columnId: string, title: string) => Promise<void>;
}) {
  const [newCardTitle, setNewCardTitle] = useState("");
  const listRef = useRef<HTMLDivElement | null>(null);

  const cardVirtualizer = useVirtualizer({
    count: props.cards.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => 132,
    overscan: 5,
  });

  const submitCreate = async () => {
    const title = newCardTitle.trim();
    if (!title) {
      return;
    }
    await props.onCreateCard(props.column.column_id, title);
    setNewCardTitle("");
  };

  return (
    <section className="mc-lane">
      <header className="mc-lane-header">
        <h3>{props.column.name}</h3>
        <span>{props.cards.length}</span>
      </header>

      <div
        className="mc-lane-body"
        ref={listRef}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault();
          const cardId = event.dataTransfer.getData("text/plain") || props.dragCardId;
          if (!cardId) {
            return;
          }
          props.onDropCard(cardId, props.column.column_id);
          props.setDragCardId(null);
        }}
      >
        <div
          style={{
            height: `${cardVirtualizer.getTotalSize()}px`,
            position: "relative",
          }}
        >
          {cardVirtualizer.getVirtualItems().map((virtualRow) => {
            const card = props.cards[virtualRow.index];
            return (
              <article
                key={card.card_id}
                className={clsx("mc-card", {
                  "mc-card-selected": props.selectedCardId === card.card_id,
                })}
                style={{
                  transform: `translateY(${virtualRow.start}px)`,
                  height: `${virtualRow.size}px`,
                  position: "absolute",
                  width: "100%",
                }}
                draggable
                onClick={() => props.onSelectCard(card.card_id)}
                onDragStart={(event) => {
                  props.setDragCardId(card.card_id);
                  event.dataTransfer.setData("text/plain", card.card_id);
                  event.dataTransfer.effectAllowed = "move";
                }}
                onDragEnd={() => props.setDragCardId(null)}
                onDragOver={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  const cardId =
                    event.dataTransfer.getData("text/plain") || props.dragCardId;
                  if (!cardId || cardId === card.card_id) {
                    return;
                  }
                  props.onDropCard(cardId, props.column.column_id, card.card_id);
                  props.setDragCardId(null);
                }}
              >
                <div className="mc-card-title">{card.title}</div>
                <div className="mc-card-meta">
                  <span>{card.owner_kind}</span>
                  {card.latest_run_id ? <span>run: {card.latest_run_id}</span> : null}
                </div>
              </article>
            );
          })}
        </div>
      </div>

      <div className="mc-lane-create">
        <input
          value={newCardTitle}
          onChange={(event) => setNewCardTitle(event.target.value)}
          placeholder="Add card"
        />
        <button type="button" onClick={submitCreate}>
          Add
        </button>
      </div>
    </section>
  );
}

export default function App() {
  const [activeTab, setActiveTab] = useState<MissionControlTab>("boards");
  const [settings, setSettings] = useState<RuntimeConnectionSettings>(
    loadConnectionSettings()
  );
  const [gatewayDraft, setGatewayDraft] = useState(settings.gateway_url);
  const [tokenDraft, setTokenDraft] = useState("");
  const [tokenConfigured, setTokenConfigured] = useState(false);

  const [healthState, setHealthState] = useState("idle");
  const [wsState, setWsState] = useState<WsLifecycleState>("idle");
  const [notice, setNotice] = useState<Notice | null>(null);
  const [eventStream, setEventStream] = useState<EventStreamItem[]>([]);
  const [showRawEvents, setShowRawEvents] = useState(false);

  const [boards, setBoards] = useState<{ board_id: string; name: string }[]>([]);
  const [activeBoardId, setActiveBoardId] = useState<string | null>(null);
  const [board, setBoard] = useState<BoardDetail | null>(null);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [calendarWeek, setCalendarWeek] = useState<MissionControlCalendarWeekResponse | null>(
    null
  );
  const [focusItems, setFocusItems] = useState<MissionControlFocusItem[]>([]);
  const [channelStatuses, setChannelStatuses] = useState<ChannelRuntimeAdapterStatusResponse[]>(
    []
  );
  const [jobsById, setJobsById] = useState<Map<string, MissionControlCalendarJob>>(new Map());
  const [approvalsById, setApprovalsById] = useState<Set<string>>(new Set());

  const [selectedCardId, setSelectedCardId] = useState<string | null>(null);
  const [cardEditor, setCardEditor] = useState<CardEditorDraft>(emptyEditorDraft());
  const [selectedPreviewUrl, setSelectedPreviewUrl] = useState<string | null>(null);
  const [dragCardId, setDragCardId] = useState<string | null>(null);

  const boardRefreshTimer = useRef<number | null>(null);
  const missionControlRefreshTimer = useRef<number | null>(null);

  const cardsByColumn = useMemo(() => toCardsByColumn(board), [board]);

  const selectedCard = useMemo(() => {
    if (!board || !selectedCardId) {
      return null;
    }
    return board.cards.find((card) => card.card_id === selectedCardId) ?? null;
  }, [board, selectedCardId]);

  useEffect(() => {
    if (!selectedCard) {
      setCardEditor(emptyEditorDraft());
      return;
    }
    setCardEditor({
      title: selectedCard.title,
      description: selectedCard.description ?? "",
      ownerKind: selectedCard.owner_kind,
      ownerAgentId: selectedCard.owner_agent_id ?? "",
      ownerHumanId: selectedCard.owner_human_id ?? "",
      dueAt: toInputDateTimeValue(selectedCard.due_at),
      tagsCsv: selectedCard.tags.join(", "),
      scriptMarkdown: selectedCard.script_markdown ?? "",
    });
  }, [selectedCard]);

  const refreshBoard = useCallback(
    async (boardId: string, runtimeSettings: RuntimeConnectionSettings = settings) => {
      const detail = await getBoard(runtimeSettings, boardId);
      setBoard(detail);
    },
    [settings]
  );

  const queueBoardRefresh = useCallback(
    (boardId: string, runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (boardRefreshTimer.current) {
        globalThis.clearTimeout(boardRefreshTimer.current);
      }
      boardRefreshTimer.current = globalThis.setTimeout(() => {
        void refreshBoard(boardId, runtimeSettings).catch((error: unknown) => {
          setNotice({
            tone: "error",
            message: `Board refresh failed: ${String(error)}`,
          });
        });
      }, 250);
    },
    [refreshBoard, settings]
  );

  const loadMissionControlReadModels = useCallback(
    async (runtimeSettings: RuntimeConnectionSettings = settings) => {
      const [calendar, focus, jobs, approvals, channelRuntime] = await Promise.all([
        getMissionControlCalendarWeek(runtimeSettings),
        getMissionControlFocus(runtimeSettings, 100),
        listJobs(runtimeSettings, 200),
        listApprovals(runtimeSettings, "requested", 200),
        getChannelRuntimeStatus(runtimeSettings),
      ]);
      setCalendarWeek(calendar);
      setFocusItems(focus.items);
      setJobsById(
        new Map(
          jobs.items.map((item) => [
            item.job_id,
            {
              job_id: item.job_id,
              name: item.name,
              agent_id: item.agent_id,
              enabled: item.enabled,
              schedule_kind: item.schedule_kind,
              interval_seconds: item.interval_seconds,
              cron_expr: item.cron_expr,
              next_run_at: item.next_run_at,
              last_run_at: item.last_run_at,
              last_error: item.last_error,
              lane:
                item.enabled &&
                item.schedule_kind === "interval" &&
                (item.interval_seconds ?? 0) <= 300 &&
                (item.interval_seconds ?? 0) > 0
                  ? "always_running"
                  : "scheduled",
              primary_action: item.enabled ? "pause" : "resume",
            } satisfies MissionControlCalendarJob,
          ])
        )
      );
      setApprovalsById(new Set(approvals.items.map((item) => item.approval_id)));
      setChannelStatuses(channelRuntime.items);
    },
    [settings]
  );

  const queueMissionControlRefresh = useCallback(
    (runtimeSettings: RuntimeConnectionSettings = settings) => {
      if (missionControlRefreshTimer.current) {
        globalThis.clearTimeout(missionControlRefreshTimer.current);
      }
      missionControlRefreshTimer.current = globalThis.setTimeout(() => {
        void loadMissionControlReadModels(runtimeSettings).catch((error: unknown) => {
          setNotice({
            tone: "error",
            message: `Mission Control refresh failed: ${String(error)}`,
          });
        });
      }, 300);
    },
    [loadMissionControlReadModels, settings]
  );

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
      setBoards(boardList.items.map((item) => ({ board_id: item.board_id, name: item.name })));
      setAgents(agentList.items);

      const targetBoardId =
        preferredBoardId ?? activeBoardId ?? boardList.items[0]?.board_id ?? null;
      setActiveBoardId(targetBoardId);
      if (targetBoardId) {
        await refreshBoard(targetBoardId, runtimeSettings);
      } else {
        setBoard(null);
      }
      await loadMissionControlReadModels(runtimeSettings);
    },
    [activeBoardId, loadMissionControlReadModels, refreshBoard, settings]
  );

  useEffect(() => {
    void isGatewayTokenConfigured().then(setTokenConfigured);
  }, []);

  useEffect(() => {
    if (!tokenConfigured || !settings.gateway_url.trim()) {
      setWsState("idle");
      return;
    }

    const subscription = connectGatewayEvents({
      settings,
      maxReconnectAttempts: 40,
      onState: setWsState,
      onEvent: (frame: WsEventFrame) => {
        setEventStream((previous) => {
          const next: EventStreamItem = {
            event_id: frame.event_id,
            event_type: frame.event_type,
            entity: frame.entity,
            ts_unix_ms: frame.ts_unix_ms,
            payload: frame.payload,
          };
          return [next, ...previous].slice(0, 400);
        });

        if (
          frame.event_type.startsWith("job.") ||
          frame.event_type.startsWith("approval.") ||
          frame.event_type.startsWith("channel.")
        ) {
          queueMissionControlRefresh(settings);
        }

        if (!activeBoardId) {
          return;
        }
        const payloadBoardId =
          typeof frame.payload.board_id === "string" ? frame.payload.board_id : null;
        if (payloadBoardId !== activeBoardId) {
          return;
        }

        setBoard((previous) => {
          if (!previous) {
            return previous;
          }
          if (frame.event_type === "board.card.moved") {
            const cardId =
              typeof frame.payload.card_id === "string" ? frame.payload.card_id : null;
            const columnId =
              typeof frame.payload.column_id === "string" ? frame.payload.column_id : null;
            const position =
              typeof frame.payload.position === "number" ? frame.payload.position : null;
            if (!cardId || !columnId) {
              return previous;
            }
            const target = previous.cards.find((item) => item.card_id === cardId);
            if (!target) {
              queueBoardRefresh(activeBoardId, settings);
              return previous;
            }
            return withUpsertCard(previous, {
              ...target,
              column_id: columnId,
              position: position ?? target.position,
            });
          }

          if (frame.event_type === "board.card.run") {
            const cardId =
              typeof frame.payload.card_id === "string" ? frame.payload.card_id : null;
            const runId =
              typeof frame.payload.run_id === "string" ? frame.payload.run_id : null;
            if (!cardId) {
              return previous;
            }
            const target = previous.cards.find((item) => item.card_id === cardId);
            if (!target) {
              return previous;
            }
            return withUpsertCard(previous, {
              ...target,
              latest_run_id: runId ?? target.latest_run_id,
            });
          }

          if (frame.event_type === "board.card.created") {
            queueBoardRefresh(activeBoardId, settings);
            return previous;
          }

          if (frame.event_type === "board.card.updated") {
            const cardId =
              typeof frame.payload.card_id === "string" ? frame.payload.card_id : null;
            const updatedAt =
              typeof frame.payload.updated_at === "number"
                ? frame.payload.updated_at
                : null;
            if (!cardId) {
              return previous;
            }
            const target = previous.cards.find((item) => item.card_id === cardId);
            if (!target) {
              queueBoardRefresh(activeBoardId, settings);
              return previous;
            }
            return withUpsertCard(previous, {
              ...target,
              updated_at: updatedAt ?? target.updated_at,
            });
          }

          if (frame.event_type === "board.asset.uploaded") {
            queueBoardRefresh(activeBoardId, settings);
          }
          return previous;
        });
      },
    });

    return () => {
      subscription.close();
    };
  }, [
    activeBoardId,
    queueBoardRefresh,
    queueMissionControlRefresh,
    settings,
    tokenConfigured,
  ]);

  const saveConnection = async () => {
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
    } catch (error) {
      setNotice({
        tone: "critical",
        message: `Connection save failed: ${String(error)}`,
      });
    }
  };

  const clearToken = async () => {
    await clearGatewayToken();
    setTokenConfigured(false);
    setWsState("idle");
    setNotice({ tone: "info", message: "Gateway token cleared." });
  };

  const reconnect = async () => {
    try {
      await loadBaseline(settings);
      setNotice({ tone: "info", message: "Connection refreshed." });
    } catch (error) {
      setNotice({
        tone: "critical",
        message: `Reconnect failed: ${String(error)}`,
      });
    }
  };

  const handleBoardChange = async (boardId: string) => {
    try {
      setActiveBoardId(boardId);
      await refreshBoard(boardId, settings);
    } catch (error) {
      setNotice({
        tone: "critical",
        message: `Board load failed: ${String(error)}`,
      });
    }
  };

  const handleDropCard = async (
    cardId: string,
    columnId: string,
    beforeCardId?: string
  ) => {
    if (!board || !activeBoardId) {
      return;
    }
    const snapshot = board;
    setBoard((previous) =>
      previous ? withOptimisticMove(previous, cardId, columnId, beforeCardId) : previous
    );
    try {
      const moved = await moveBoardCard(settings, activeBoardId, cardId, {
        column_id: columnId,
        before_card_id: beforeCardId,
      });
      setBoard((previous) =>
        previous ? withUpsertCard(previous, moved.card) : previous
      );
    } catch (error) {
      setBoard(snapshot);
      setNotice({ tone: "error", message: `Move failed: ${String(error)}` });
    }
  };

  const handleCreateCard = async (columnId: string, title: string) => {
    if (!activeBoardId) {
      return;
    }
    try {
      const created = await createBoardCard(settings, activeBoardId, {
        column_id: columnId,
        title,
      });
      setBoard((previous) =>
        previous ? withUpsertCard(previous, created.card) : previous
      );
      setNotice({ tone: "info", message: `Card created: ${created.card.title}` });
    } catch (error) {
      setNotice({ tone: "error", message: `Card create failed: ${String(error)}` });
    }
  };

  const saveCardDraft = async () => {
    if (!activeBoardId || !selectedCardId) {
      return;
    }
    try {
      const response = await updateBoardCard(settings, activeBoardId, selectedCardId, {
        title: cardEditor.title.trim(),
        description: cardEditor.description.trim() || null,
        owner_kind: cardEditor.ownerKind,
        owner_agent_id: cardEditor.ownerAgentId.trim() || null,
        owner_human_id: cardEditor.ownerHumanId.trim() || null,
        due_at: fromInputDateTimeValue(cardEditor.dueAt),
        tags: cardEditor.tagsCsv.trim()
          ? cardEditor.tagsCsv
              .split(",")
              .map((tag) => tag.trim())
              .filter(Boolean)
          : null,
        script_markdown: cardEditor.scriptMarkdown.trim() || null,
      });
      setBoard((previous) =>
        previous ? withUpsertCard(previous, response.card) : previous
      );
      setNotice({ tone: "info", message: "Card updated." });
    } catch (error) {
      setNotice({ tone: "error", message: `Card update failed: ${String(error)}` });
    }
  };

  const runCard = async () => {
    if (!activeBoardId || !selectedCardId) {
      return;
    }
    try {
      const response = await runBoardCard(settings, activeBoardId, selectedCardId);
      setBoard((previous) =>
        previous ? withUpsertCard(previous, response.card) : previous
      );
      setNotice({
        tone: "info",
        message: `Run queued: ${response.run.run_id} (${response.run.status})`,
      });
    } catch (error) {
      setNotice({ tone: "error", message: `Run failed: ${String(error)}` });
    }
  };

  const uploadAsset = async (file: File) => {
    if (!activeBoardId || !selectedCardId) {
      return;
    }
    try {
      const contentBase64 = await fileToBase64(file);
      const response = await uploadBoardCardAsset(
        settings,
        activeBoardId,
        selectedCardId,
        {
          filename: file.name,
          mime: file.type || "application/octet-stream",
          content_base64: contentBase64,
        }
      );
      setBoard((previous) =>
        previous ? withUpsertCard(previous, response.card) : previous
      );
      setNotice({ tone: "info", message: `Asset uploaded: ${response.asset.filename}` });
    } catch (error) {
      setNotice({ tone: "error", message: `Asset upload failed: ${String(error)}` });
    }
  };

  const previewAsset = async (cardId: string, cardAssetId: string) => {
    if (!activeBoardId) {
      return;
    }
    try {
      const blob = await fetchBoardCardAssetBlob(
        settings,
        activeBoardId,
        cardId,
        cardAssetId
      );
      if (selectedPreviewUrl) {
        URL.revokeObjectURL(selectedPreviewUrl);
      }
      const url = URL.createObjectURL(blob);
      setSelectedPreviewUrl(url);
    } catch (error) {
      setNotice({ tone: "error", message: `Asset preview failed: ${String(error)}` });
    }
  };

  const runCalendarJobNow = async (jobId: string) => {
    try {
      const response = await runJobNow(settings, jobId);
      setNotice({
        tone: "info",
        message: `Job run started (${response.job_run.status})`,
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Run-now failed: ${String(error)}`,
      });
    }
  };

  const toggleCalendarJob = async (jobId: string, enabled: boolean) => {
    try {
      const response = await setJobEnabledState(settings, jobId, enabled);
      setJobsById((previous) => {
        const next = new Map(previous);
        const existing = next.get(jobId);
        if (existing) {
          next.set(jobId, {
            ...existing,
            enabled: response.job.enabled,
            primary_action: response.job.enabled ? "pause" : "resume",
            next_run_at: response.job.next_run_at,
            last_error: response.job.last_error,
          });
        }
        return next;
      });
      setNotice({
        tone: "info",
        message: enabled ? "Job resumed." : "Job paused.",
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Job state update failed: ${String(error)}`,
      });
    }
  };

  const resolveFocusApproval = async (
    approvalId: string,
    decision: "approve" | "deny"
  ) => {
    try {
      const response = await resolveApproval(settings, approvalId, decision);
      setApprovalsById((previous) => {
        const next = new Set(previous);
        if (response.approval.status !== "requested") {
          next.delete(approvalId);
        }
        return next;
      });
      setNotice({
        tone: "info",
        message: `Approval ${decision}d.`,
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Approval ${decision} failed: ${String(error)}`,
      });
    }
  };

  const reconnectFocusChannel = async (provider: string) => {
    try {
      await reconnectChannelRuntime(settings, provider);
      setNotice({
        tone: "info",
        message: `Channel reconnect requested for ${provider}.`,
      });
      queueMissionControlRefresh(settings);
    } catch (error) {
      setNotice({
        tone: "error",
        message: `Channel reconnect failed: ${String(error)}`,
      });
    }
  };

  const visibleEvents = useMemo(() => {
    if (showRawEvents) {
      return eventStream;
    }
    return eventStream.filter((event) => !event.event_type.startsWith("heartbeat."));
  }, [eventStream, showRawEvents]);

  const columns = board?.columns ?? [];
  const boardScrollerRef = useRef<HTMLDivElement | null>(null);
  const columnVirtualizer = useVirtualizer({
    count: columns.length,
    horizontal: true,
    getScrollElement: () => boardScrollerRef.current,
    estimateSize: () => 320,
    overscan: 2,
  });

  const calendarAlwaysRunning = calendarWeek?.always_running ?? [];
  const calendarNextUp = calendarWeek?.next_up ?? [];
  const calendarJobs = calendarWeek?.jobs ?? Array.from(jobsById.values());

  useEffect(() => {
    return () => {
      if (boardRefreshTimer.current) {
        globalThis.clearTimeout(boardRefreshTimer.current);
      }
      if (missionControlRefreshTimer.current) {
        globalThis.clearTimeout(missionControlRefreshTimer.current);
      }
      if (selectedPreviewUrl) {
        URL.revokeObjectURL(selectedPreviewUrl);
      }
    };
  }, [selectedPreviewUrl]);

  return (
    <main className="mc-shell">
      <header className="mc-topbar">
        <div className="mc-brand-block">
          <p className="mc-overline">CarsinOS</p>
          <h1>Mission Control Slick</h1>
        </div>
        <div className="mc-status-strip">
          <span className={clsx("chip", `chip-${healthState}`)}>health: {healthState}</span>
          <span className={clsx("chip", `chip-${wsState}`)}>ws: {wsState}</span>
          <span className="chip">token: {tokenConfigured ? "set" : "missing"}</span>
        </div>
      </header>

      <section className="mc-connection">
        <label>
          Gateway URL
          <input
            value={gatewayDraft}
            onChange={(event) => setGatewayDraft(event.target.value)}
            placeholder="http://127.0.0.1:8080"
          />
        </label>
        <label>
          Gateway Token
          <input
            value={tokenDraft}
            onChange={(event) => setTokenDraft(event.target.value)}
            placeholder={tokenConfigured ? "token stored in keychain" : "paste token"}
            type="password"
          />
        </label>
        <div className="mc-connection-actions">
          <button type="button" onClick={() => void saveConnection()}>
            Save + Connect
          </button>
          <button type="button" onClick={() => void reconnect()}>
            Reconnect
          </button>
          <button type="button" className="danger" onClick={() => void clearToken()}>
            Clear Token
          </button>
        </div>
      </section>

      {notice ? (
        <div className={clsx("mc-notice", `mc-notice-${notice.tone}`)}>{notice.message}</div>
      ) : null}

      <section className="mc-tabs">
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "boards" && "mc-tab-active")}
          onClick={() => setActiveTab("boards")}
        >
          Boards
        </button>
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "calendar" && "mc-tab-active")}
          onClick={() => setActiveTab("calendar")}
        >
          Calendar
        </button>
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "focus" && "mc-tab-active")}
          onClick={() => setActiveTab("focus")}
        >
          Operator Focus
        </button>
        <button
          type="button"
          className={clsx("mc-tab", activeTab === "events" && "mc-tab-active")}
          onClick={() => setActiveTab("events")}
        >
          Event Stream
        </button>
      </section>

      {activeTab === "boards" ? (
        <section className="mc-main-grid">
          <section className="mc-board-panel">
            <div className="mc-board-toolbar">
              <label>
                Board
                <select
                  value={activeBoardId ?? ""}
                  onChange={(event) => void handleBoardChange(event.target.value)}
                >
                  {boards.map((item) => (
                    <option key={item.board_id} value={item.board_id}>
                      {item.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <div className="mc-board-scroll" ref={boardScrollerRef}>
              <div
                className="mc-board-canvas"
                style={{ width: `${columnVirtualizer.getTotalSize()}px` }}
              >
                {columnVirtualizer.getVirtualItems().map((virtualColumn) => {
                  const column = columns[virtualColumn.index];
                  const cards = cardsByColumn.get(column.column_id) ?? [];
                  return (
                    <div
                      key={column.column_id}
                      className="mc-board-column-wrap"
                      style={{ transform: `translateX(${virtualColumn.start}px)` }}
                    >
                      <BoardLane
                        column={column}
                        cards={cards}
                        selectedCardId={selectedCardId}
                        dragCardId={dragCardId}
                        setDragCardId={setDragCardId}
                        onSelectCard={setSelectedCardId}
                        onDropCard={handleDropCard}
                        onCreateCard={handleCreateCard}
                      />
                    </div>
                  );
                })}
              </div>
            </div>
          </section>

          <aside className="mc-drawer">
            {!selectedCard ? (
              <div className="mc-empty-drawer">Select a card to edit and run.</div>
            ) : (
              <>
                <header className="mc-drawer-header">
                  <h2>Card Drawer</h2>
                  {selectedCard.latest_run_id ? (
                    <span className="run-pill">latest run: {selectedCard.latest_run_id}</span>
                  ) : null}
                </header>

                <label>
                  Title
                  <input
                    value={cardEditor.title}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        title: event.target.value,
                      }))
                    }
                  />
                </label>

                <label>
                  Description
                  <textarea
                    value={cardEditor.description}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        description: event.target.value,
                      }))
                    }
                  />
                </label>

                <div className="mc-field-grid">
                  <label>
                    Owner Kind
                    <select
                      value={cardEditor.ownerKind}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          ownerKind: event.target.value,
                        }))
                      }
                    >
                      <option value="unassigned">unassigned</option>
                      <option value="agent">agent</option>
                      <option value="human">human</option>
                    </select>
                  </label>

                  <label>
                    Owner Agent
                    <select
                      value={cardEditor.ownerAgentId}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          ownerAgentId: event.target.value,
                        }))
                      }
                    >
                      <option value="">none</option>
                      {agents.map((agent) => (
                        <option key={agent.agent_id} value={agent.agent_id}>
                          {agent.name} ({agent.agent_id})
                        </option>
                      ))}
                    </select>
                  </label>
                </div>

                <div className="mc-field-grid">
                  <label>
                    Owner Human
                    <input
                      value={cardEditor.ownerHumanId}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          ownerHumanId: event.target.value,
                        }))
                      }
                    />
                  </label>

                  <label>
                    Due
                    <input
                      type="datetime-local"
                      value={cardEditor.dueAt}
                      onChange={(event) =>
                        setCardEditor((previous) => ({
                          ...previous,
                          dueAt: event.target.value,
                        }))
                      }
                    />
                  </label>
                </div>

                <label>
                  Tags (comma separated)
                  <input
                    value={cardEditor.tagsCsv}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        tagsCsv: event.target.value,
                      }))
                    }
                  />
                </label>

                <label>
                  Script Markdown
                  <textarea
                    className="script-area"
                    value={cardEditor.scriptMarkdown}
                    onChange={(event) =>
                      setCardEditor((previous) => ({
                        ...previous,
                        scriptMarkdown: event.target.value,
                      }))
                    }
                  />
                </label>

                <div className="mc-drawer-actions">
                  <button type="button" onClick={() => void saveCardDraft()}>
                    Save Card
                  </button>
                  <button type="button" onClick={() => void runCard()}>
                    Run Card
                  </button>
                </div>

                <section className="mc-assets">
                  <h3>Assets</h3>
                  <label className="upload-pill">
                    <input
                      type="file"
                      onChange={(event) => {
                        const file = event.target.files?.[0];
                        if (!file) {
                          return;
                        }
                        void uploadAsset(file);
                        event.currentTarget.value = "";
                      }}
                    />
                    Upload
                  </label>
                  <ul>
                    {selectedCard.assets.map((asset) => (
                      <li key={asset.card_asset_id}>
                        <button
                          type="button"
                          onClick={() =>
                            void previewAsset(selectedCard.card_id, asset.card_asset_id)
                          }
                        >
                          {asset.filename}
                        </button>
                        <span>{formatBytes(asset.bytes)}</span>
                      </li>
                    ))}
                  </ul>
                  {selectedPreviewUrl ? (
                    <div className="mc-preview-wrap">
                      <img src={selectedPreviewUrl} alt="asset preview" />
                    </div>
                  ) : null}
                </section>
              </>
            )}
          </aside>
        </section>
      ) : null}

      {activeTab === "calendar" ? (
        <section className="mc-alt-grid">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Week Planning</h2>
              <p>
                {calendarWeek
                  ? `${formatDateTime(calendarWeek.week_start_ms)} - ${formatDateTime(
                      calendarWeek.week_end_ms
                    )}`
                  : "No week data loaded"}
              </p>
            </header>
            <div className="mc-lane-grid">
              <section className="mc-lane-panel">
                <h3>Always Running</h3>
                <ul>
                  {calendarAlwaysRunning.map((job) => (
                    <li key={job.job_id}>
                      <div>
                        <strong>{job.name}</strong>
                        <p>{job.agent_id}</p>
                      </div>
                      <div className="mc-inline-actions">
                        <button type="button" onClick={() => void runCalendarJobNow(job.job_id)}>
                          Run now
                        </button>
                        <button
                          type="button"
                          className={job.enabled ? "danger" : ""}
                          onClick={() => void toggleCalendarJob(job.job_id, !job.enabled)}
                        >
                          {job.enabled ? "Pause" : "Resume"}
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
              <section className="mc-lane-panel">
                <h3>Next Up</h3>
                <ul>
                  {calendarNextUp.map((job) => (
                    <li key={job.job_id}>
                      <div>
                        <strong>{job.name}</strong>
                        <p>{formatDateTime(job.next_run_at)}</p>
                      </div>
                      <button type="button" onClick={() => void runCalendarJobNow(job.job_id)}>
                        Run now
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            </div>
          </article>
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Scheduler Matrix</h2>
              <p>{calendarJobs.length} jobs</p>
            </header>
            <div className="mc-table-wrap">
              <table className="mc-table">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Schedule</th>
                    <th>Next Run</th>
                    <th>Status</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {calendarJobs.map((job) => (
                    <tr key={job.job_id}>
                      <td>
                        <strong>{job.name}</strong>
                        <p>{job.agent_id}</p>
                      </td>
                      <td>
                        {job.schedule_kind}
                        {job.interval_seconds ? ` / ${job.interval_seconds}s` : ""}
                        {job.cron_expr ? ` / ${job.cron_expr}` : ""}
                      </td>
                      <td>{formatDateTime(job.next_run_at)}</td>
                      <td>
                        <span
                          className={clsx("chip", job.enabled ? "chip-up" : "chip-down")}
                        >
                          {job.enabled ? "enabled" : "paused"}
                        </span>
                      </td>
                      <td>
                        <div className="mc-inline-actions">
                          <button type="button" onClick={() => void runCalendarJobNow(job.job_id)}>
                            Run
                          </button>
                          <button
                            type="button"
                            className={job.enabled ? "danger" : ""}
                            onClick={() => void toggleCalendarJob(job.job_id, !job.enabled)}
                          >
                            {job.enabled ? "Pause" : "Resume"}
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </article>
        </section>
      ) : null}

      {activeTab === "focus" ? (
        <section className="mc-alt-grid">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Operator Focus Queue</h2>
              <p>{focusItems.length} open attention items</p>
            </header>
            <div className="mc-focus-list">
              {focusItems.map((item) => (
                <article key={item.item_id} className={clsx("mc-focus-item", item.severity)}>
                  <div className="mc-focus-head">
                    <span className={clsx("chip", `chip-${item.severity}`)}>
                      {item.severity}
                    </span>
                    <span>{item.category}</span>
                    <span>{formatDateTime(item.created_at)}</span>
                  </div>
                  <h3>{item.title}</h3>
                  <p>{item.detail}</p>
                  <div className="mc-inline-actions">
                    {item.category === "approval" ? (
                      <>
                        <button
                          type="button"
                          onClick={() =>
                            void resolveFocusApproval(
                              String(item.action_payload.approval_id ?? ""),
                              "approve"
                            )
                          }
                        >
                          Approve
                        </button>
                        <button
                          type="button"
                          className="danger"
                          onClick={() =>
                            void resolveFocusApproval(
                              String(item.action_payload.approval_id ?? ""),
                              "deny"
                            )
                          }
                        >
                          Deny
                        </button>
                      </>
                    ) : null}
                    {item.category === "run_failure" ? (
                      <button
                        type="button"
                        onClick={() =>
                          void runCalendarJobNow(String(item.action_payload.job_id ?? ""))
                        }
                      >
                        Retry Job
                      </button>
                    ) : null}
                    {item.category === "channel_health" ? (
                      <button
                        type="button"
                        onClick={() =>
                          void reconnectFocusChannel(
                            String(item.action_payload.provider ?? "")
                          )
                        }
                      >
                        Reconnect Channel
                      </button>
                    ) : null}
                  </div>
                </article>
              ))}
            </div>
          </article>
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Ops Snapshot</h2>
              <p>Live queue and channel posture</p>
            </header>
            <ul className="mc-stat-list">
              <li>
                <strong>Pending approvals</strong>
                <span>{approvalsById.size}</span>
              </li>
              <li>
                <strong>Channel adapters</strong>
                <span>{channelStatuses.length}</span>
              </li>
              <li>
                <strong>Degraded channels</strong>
                <span>
                  {
                    channelStatuses.filter(
                      (item) => !item.healthy || item.lifecycle_state !== "running"
                    ).length
                  }
                </span>
              </li>
            </ul>
            <div className="mc-channel-grid">
              {channelStatuses.map((item) => (
                <article key={item.provider} className="mc-channel-card">
                  <h3>{item.provider}</h3>
                  <p>{item.lifecycle_state}</p>
                  <p>{item.last_error ?? item.detail ?? "healthy"}</p>
                  <button
                    type="button"
                    onClick={() => void reconnectFocusChannel(item.provider)}
                  >
                    Reconnect
                  </button>
                </article>
              ))}
            </div>
          </article>
        </section>
      ) : null}

      {activeTab === "events" ? (
        <section className="mc-alt-grid">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Realtime Event Stream</h2>
              <label className="mc-checkbox">
                <input
                  type="checkbox"
                  checked={showRawEvents}
                  onChange={(event) => setShowRawEvents(event.target.checked)}
                />
                Show raw heartbeat events
              </label>
            </header>
            <div className="mc-events">
              {visibleEvents.map((event) => (
                <article key={event.event_id} className="mc-event-item">
                  <div className="mc-event-head">
                    <span>{event.event_type}</span>
                    <span>{event.entity}</span>
                    <span>{formatDateTime(event.ts_unix_ms)}</span>
                  </div>
                  <pre>{JSON.stringify(event.payload, null, 2)}</pre>
                </article>
              ))}
              {visibleEvents.length === 0 ? (
                <p className="mc-empty-events">No events captured yet.</p>
              ) : null}
            </div>
          </article>
        </section>
      ) : null}
    </main>
  );
}
