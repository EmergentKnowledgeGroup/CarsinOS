import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  createBoardCard,
  fetchBoardCardAssetBlob,
  getBoard,
  moveBoardCard,
  runBoardCard,
  updateBoardCard,
  uploadBoardCardAsset,
} from "../../lib/api";
import type { NotifyFn } from "../../app/useAppController";
import type { BoardDetail, RuntimeConnectionSettings, WsEventFrame } from "../../types";
import { toInputDateTimeValue, fromInputDateTimeValue } from "../../utils/datetime";
import { fileToBase64 } from "../../utils/files";
import {
  emptyEditorDraft,
  toCardsByColumn,
  withOptimisticMove,
  withUpsertCard,
  type CardEditorDraft,
} from "./boardModel";

interface UseBoardsControllerOptions {
  settings: RuntimeConnectionSettings;
  setNotice: NotifyFn;
}

export function useBoardsController(options: UseBoardsControllerOptions) {
  const { settings, setNotice } = options;

  const [activeBoardId, setActiveBoardId] = useState<string | null>(null);
  const [board, setBoard] = useState<BoardDetail | null>(null);
  const [selectedCardId, setSelectedCardId] = useState<string | null>(null);
  const [cardEditor, setCardEditor] = useState<CardEditorDraft>(emptyEditorDraft());
  const [selectedPreviewUrl, setSelectedPreviewUrl] = useState<string | null>(null);
  const [dragCardId, setDragCardId] = useState<string | null>(null);
  const boardRefreshTimer = useRef<number | null>(null);

  const cardsByColumn = useMemo(() => toCardsByColumn(board), [board]);

  const selectedCard = useMemo(() => {
    if (!board || !selectedCardId) {
      return null;
    }
    return board.cards.find((card) => card.card_id === selectedCardId) ?? null;
  }, [board, selectedCardId]);

  const columns = board?.columns ?? [];

  const selectCard = useCallback(
    (cardId: string | null) => {
      setSelectedCardId(cardId);
      if (!cardId || !board) {
        setCardEditor(emptyEditorDraft());
        return;
      }
      const card = board.cards.find((item) => item.card_id === cardId);
      if (!card) {
        setCardEditor(emptyEditorDraft());
        return;
      }
      setCardEditor({
        title: card.title,
        description: card.description ?? "",
        ownerKind: card.owner_kind,
        ownerAgentId: card.owner_agent_id ?? "",
        ownerHumanId: card.owner_human_id ?? "",
        dueAt: toInputDateTimeValue(card.due_at),
        tagsCsv: card.tags.join(", "),
        scriptMarkdown: card.script_markdown ?? "",
      });
    },
    [board]
  );

  useEffect(() => {
    return () => {
      if (boardRefreshTimer.current) {
        globalThis.clearTimeout(boardRefreshTimer.current);
      }
    };
  }, []);

  useEffect(() => {
    return () => {
      if (selectedPreviewUrl) {
        URL.revokeObjectURL(selectedPreviewUrl);
      }
    };
  }, [selectedPreviewUrl]);

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
    [refreshBoard, setNotice, settings]
  );

  const handleBoardChange = useCallback(
    async (boardId: string) => {
      try {
        setActiveBoardId(boardId);
        await refreshBoard(boardId, settings);
      } catch (error) {
        setNotice({
          tone: "critical",
          message: `Board load failed: ${String(error)}`,
        });
      }
    },
    [refreshBoard, setNotice, settings]
  );

  const handleDropCard = useCallback(
    async (cardId: string, columnId: string, beforeCardId?: string) => {
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
        setBoard((previous) => (previous ? withUpsertCard(previous, moved.card) : previous));
      } catch (error) {
        setBoard(snapshot);
        setNotice({ tone: "error", message: `Move failed: ${String(error)}` });
      }
    },
    [activeBoardId, board, setNotice, settings]
  );

  const handleCreateCard = useCallback(
    async (columnId: string, title: string, opts?: { owner_kind?: string; owner_agent_id?: string; owner_human_id?: string }) => {
      if (!activeBoardId) {
        return;
      }
      try {
        const created = await createBoardCard(settings, activeBoardId, {
          column_id: columnId,
          title,
          ...opts,
        });
        setBoard((previous) => (previous ? withUpsertCard(previous, created.card) : previous));
        setNotice({ tone: "info", message: `Card created: ${created.card.title}` });
      } catch (error) {
        setNotice({ tone: "error", message: `Card create failed: ${String(error)}` });
      }
    },
    [activeBoardId, setNotice, settings]
  );

  const saveCardDraft = useCallback(async () => {
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
      setBoard((previous) => (previous ? withUpsertCard(previous, response.card) : previous));
      setNotice({ tone: "info", message: "Card updated." });
    } catch (error) {
      setNotice({ tone: "error", message: `Card update failed: ${String(error)}` });
    }
  }, [activeBoardId, cardEditor, selectedCardId, setNotice, settings]);

  const runCard = useCallback(async () => {
    if (!activeBoardId || !selectedCardId) {
      return;
    }
    try {
      const response = await runBoardCard(settings, activeBoardId, selectedCardId);
      setBoard((previous) => (previous ? withUpsertCard(previous, response.card) : previous));
      setNotice({
        tone: "info",
        message: `Run queued: ${response.run.run_id} (${response.run.status})`,
      });
    } catch (error) {
      setNotice({ tone: "error", message: `Run failed: ${String(error)}` });
    }
  }, [activeBoardId, selectedCardId, setNotice, settings]);

  const uploadAsset = useCallback(
    async (file: File) => {
      if (!activeBoardId || !selectedCardId) {
        return;
      }
      try {
        const contentBase64 = await fileToBase64(file);
        const response = await uploadBoardCardAsset(settings, activeBoardId, selectedCardId, {
          filename: file.name,
          mime: file.type || "application/octet-stream",
          content_base64: contentBase64,
        });
        setBoard((previous) => (previous ? withUpsertCard(previous, response.card) : previous));
        setNotice({ tone: "info", message: `Asset uploaded: ${response.asset.filename}` });
      } catch (error) {
        setNotice({ tone: "error", message: `Asset upload failed: ${String(error)}` });
      }
    },
    [activeBoardId, selectedCardId, setNotice, settings]
  );

  const previewAsset = useCallback(
    async (cardId: string, cardAssetId: string) => {
      if (!activeBoardId) {
        return;
      }
      try {
        const blob = await fetchBoardCardAssetBlob(settings, activeBoardId, cardId, cardAssetId);
        if (selectedPreviewUrl) {
          URL.revokeObjectURL(selectedPreviewUrl);
        }
        const url = URL.createObjectURL(blob);
        setSelectedPreviewUrl(url);
      } catch (error) {
        setNotice({ tone: "error", message: `Asset preview failed: ${String(error)}` });
      }
    },
    [activeBoardId, selectedPreviewUrl, setNotice, settings]
  );

  const applyGatewayBoardEvent = useCallback(
    (frame: WsEventFrame, runtimeSettings: RuntimeConnectionSettings = settings) => {
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
            queueBoardRefresh(activeBoardId, runtimeSettings);
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
          queueBoardRefresh(activeBoardId, runtimeSettings);
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
            queueBoardRefresh(activeBoardId, runtimeSettings);
            return previous;
          }
          return withUpsertCard(previous, {
            ...target,
            updated_at: updatedAt ?? target.updated_at,
          });
        }

        if (frame.event_type === "board.asset.uploaded") {
          queueBoardRefresh(activeBoardId, runtimeSettings);
        }
        return previous;
      });
    },
    [activeBoardId, queueBoardRefresh, settings]
  );

  return {
    activeBoardId,
    setActiveBoardId,
    board,
    setBoard,
    selectedCardId,
    setSelectedCardId,
    selectCard,
    cardEditor,
    setCardEditor,
    selectedPreviewUrl,
    setSelectedPreviewUrl,
    dragCardId,
    setDragCardId,
    cardsByColumn,
    selectedCard,
    columns,
    refreshBoard,
    queueBoardRefresh,
    handleBoardChange,
    handleDropCard,
    handleCreateCard,
    saveCardDraft,
    runCard,
    uploadAsset,
    previewAsset,
    applyGatewayBoardEvent,
  };
}
