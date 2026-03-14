import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type {
  Agent,
  BoardCard,
  BoardColumn,
  RunbookSummaryItemResponse,
  TaskResponse,
} from "../../types";
import { formatBytes } from "../../utils/files";
import { Modal } from "../../ui/Modal";
import { Pagination } from "../../ui/Pagination";
import { Tabs } from "../../ui/Tabs";
import { TagPicker } from "../../ui/TagPicker";
import { usePagination } from "../../ui/usePagination";
import { RunbookLinkPanel } from "../runbook/RunbookLinkPanel";
import { StrategyTaskContextPanel } from "../strategy/StrategyTaskContextPanel";
import type { StrategyTaskContextSnapshot } from "../strategy/useStrategyController";
import { BoardLane } from "./BoardLane";
import type { CardEditorDraft } from "./boardModel";

const ASSETS_PAGE_SIZE = 6;

interface BoardsPageProps {
  boards: Array<{ board_id: string; name: string }>;
  activeBoardId: string | null;
  loading?: boolean;
  onBoardChange: (boardId: string) => Promise<void>;
  columns: BoardColumn[];
  cardsByColumn: Map<string, BoardCard[]>;
  selectedCardId: string | null;
  dragCardId: string | null;
  setDragCardId: (value: string | null) => void;
  onSelectCard: (cardId: string | null) => void;
  onDropCard: (cardId: string, columnId: string, beforeCardId?: string) => void;
  onCreateCard: (columnId: string, title: string, opts?: { owner_kind?: string; owner_agent_id?: string; owner_human_id?: string }) => Promise<boolean>;
  selectedCard: BoardCard | null;
  cardEditor: CardEditorDraft;
  setCardEditor: Dispatch<SetStateAction<CardEditorDraft>>;
  agents: Agent[];
  onSaveCardDraft: () => Promise<void>;
  onRunCard: () => Promise<void>;
  onMoveCardToColumn: (columnId: string) => Promise<void>;
  onUploadAsset: (file: File) => Promise<void>;
  onPreviewAsset: (cardId: string, cardAssetId: string) => Promise<void>;
  selectedPreviewUrl: string | null;
  editorBusy: boolean;
  editorBusyAction: "save" | "run" | "upload" | "move" | null;
  strategyReady: boolean;
  linkedTaskByCardId: Map<string, TaskResponse>;
  describeStrategyTask: (taskId: string) => StrategyTaskContextSnapshot | null;
  onOpenStrategyTask: (taskId: string) => boolean;
  runbookEnabled: boolean;
  runbookByCardId: Map<string, RunbookSummaryItemResponse>;
  onOpenBoardCardRunbook: (cardId: string) => boolean;
}

type OwnerFilter = "all" | "unassigned" | string;

export function BoardsPage({
  boards,
  activeBoardId,
  loading,
  onBoardChange,
  columns,
  cardsByColumn,
  selectedCardId,
  dragCardId,
  setDragCardId,
  onSelectCard,
  onDropCard,
  onCreateCard,
  selectedCard,
  cardEditor,
  setCardEditor,
  agents,
  onSaveCardDraft,
  onRunCard,
  onMoveCardToColumn,
  onUploadAsset,
  onPreviewAsset,
  selectedPreviewUrl,
  editorBusy,
  editorBusyAction,
  strategyReady,
  linkedTaskByCardId,
  describeStrategyTask,
  onOpenStrategyTask,
  runbookEnabled,
  runbookByCardId,
  onOpenBoardCardRunbook,
}: BoardsPageProps) {
  const [editorTab, setEditorTab] = useState<"details" | "script" | "assets">("details");
  const [assetsPage, setAssetsPage] = useState(1);
  const [ownerFilter, setOwnerFilter] = useState<OwnerFilter>("all");
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [newCardTitle, setNewCardTitle] = useState("");
  const [newCardColumnId, setNewCardColumnId] = useState("");
  const [newCardOwnerKind, setNewCardOwnerKind] = useState("unassigned");
  const [newCardOwnerAgentId, setNewCardOwnerAgentId] = useState("");
  const [newCardOwnerHumanId, setNewCardOwnerHumanId] = useState("");
  const [moveTargetColumnId, setMoveTargetColumnId] = useState("");

  useEffect(() => {
    setMoveTargetColumnId(selectedCard?.column_id ?? "");
  }, [selectedCard?.card_id, selectedCard?.column_id]);

  const knownTags = useMemo(() => {
    const tags = new Set<string>();
    for (const cards of cardsByColumn.values()) {
      for (const card of cards) {
        for (const tag of card.tags) {
          if (tag) tags.add(tag);
        }
      }
    }
    return Array.from(tags).sort();
  }, [cardsByColumn]);

  /* ── All cards flattened (for stats + agent filter list) ── */
  const allCards = useMemo(() => {
    const result: BoardCard[] = [];
    for (const cards of cardsByColumn.values()) result.push(...cards);
    return result;
  }, [cardsByColumn]);

  const agentOwners = useMemo(() => {
    const set = new Set<string>();
    for (const card of allCards) {
      if (card.owner_kind === "agent" && card.owner_agent_id) set.add(card.owner_agent_id);
    }
    return Array.from(set).sort();
  }, [allCards]);

  /* ── Filter cardsByColumn based on ownerFilter ── */
  const filteredCardsByColumn = useMemo(() => {
    if (ownerFilter === "all") return cardsByColumn;
    const filtered = new Map<string, BoardCard[]>();
    for (const [colId, cards] of cardsByColumn) {
      filtered.set(
        colId,
        cards.filter((c) =>
          ownerFilter === "unassigned"
            ? c.owner_kind === "unassigned"
            : c.owner_agent_id === ownerFilter
        ),
      );
    }
    return filtered;
  }, [cardsByColumn, ownerFilter]);

  /* ── Per-page stats ── */
  const totalCards = allCards.length;
  const inProgressCount = useMemo(() => {
    const inProgressCols = new Set(
      columns.filter((c) => /progress|doing|active/i.test(c.name)).map((c) => c.column_id),
    );
    return allCards.filter((c) => inProgressCols.has(c.column_id)).length;
  }, [allCards, columns]);
  const doneCount = useMemo(() => {
    const doneCols = new Set(
      columns.filter((c) => /done|complete|finished/i.test(c.name)).map((c) => c.column_id),
    );
    return allCards.filter((c) => doneCols.has(c.column_id)).length;
  }, [allCards, columns]);

  const handleOpenCreateModal = () => {
    setNewCardTitle("");
    setNewCardColumnId(columns[0]?.column_id ?? "");
    setNewCardOwnerKind("unassigned");
    setNewCardOwnerAgentId("");
    setNewCardOwnerHumanId("");
    setCreateModalOpen(true);
  };

  const handleSubmitCreate = async () => {
    const title = newCardTitle.trim();
    if (!title || !newCardColumnId) return;
    if (newCardOwnerKind === "human" && !newCardOwnerHumanId.trim()) return;
    if (newCardOwnerKind === "agent" && !newCardOwnerAgentId.trim()) return;
    const opts: { owner_kind?: string; owner_agent_id?: string; owner_human_id?: string } = {};
    if (newCardOwnerKind !== "unassigned") {
      opts.owner_kind = newCardOwnerKind;
      if (newCardOwnerKind === "agent" && newCardOwnerAgentId) {
        opts.owner_agent_id = newCardOwnerAgentId;
      }
      if (newCardOwnerKind === "human" && newCardOwnerHumanId.trim()) {
        opts.owner_human_id = newCardOwnerHumanId.trim();
      }
    }
    const created = await onCreateCard(
      newCardColumnId,
      title,
      Object.keys(opts).length > 0 ? opts : undefined
    );
    if (created) {
      setCreateModalOpen(false);
    }
  };

  const assetsPagination = usePagination(selectedCard?.assets ?? [], ASSETS_PAGE_SIZE);
  const visibleAssets = assetsPagination.getPage(assetsPage);
  const canCreateCard =
    newCardTitle.trim().length > 0 &&
    newCardColumnId.length > 0 &&
    (newCardOwnerKind !== "human" || newCardOwnerHumanId.trim().length > 0) &&
    (newCardOwnerKind !== "agent" || newCardOwnerAgentId.trim().length > 0);

  const boardScrollerRef = useRef<HTMLDivElement | null>(null);
  // eslint-disable-next-line react-hooks/incompatible-library
  const columnVirtualizer = useVirtualizer({
    count: columns.length,
    horizontal: true,
    getScrollElement: () => boardScrollerRef.current,
    estimateSize: () => 320,
    overscan: 2,
  });

  const cardEditorOpen = selectedCard !== null;
  const linkedTask = selectedCard
    ? linkedTaskByCardId.get(selectedCard.card_id) ?? null
    : null;
  const selectedCardRunbook = selectedCard
    ? runbookByCardId.get(selectedCard.card_id) ?? null
    : null;
  const linkedTaskContext = linkedTask
    ? describeStrategyTask(linkedTask.task_id)
    : null;
  const canMoveSelectedCard =
    Boolean(selectedCard) &&
    Boolean(moveTargetColumnId) &&
    moveTargetColumnId !== selectedCard?.column_id &&
    !editorBusy;

  return (
    <section className="mc-board-full">
      <div className="mc-board-toolbar">
        <div className="mc-board-toolbar-left">
          <label>
            Board
            <select
              value={activeBoardId ?? ""}
              onChange={(event) => void onBoardChange(event.target.value)}
            >
              {boards.map((item) => (
                <option key={item.board_id} value={item.board_id}>
                  {item.name}
                </option>
              ))}
            </select>
          </label>
          <button type="button" onClick={handleOpenCreateModal}>+ New Card</button>
        </div>
        <div className="mc-board-stats">
          <span>Cards: {totalCards}</span>
          <span>In Progress: {inProgressCount}</span>
          <span>Done: {doneCount}</span>
        </div>
      </div>

      {/* ── Owner filter dropdown ── */}
      <div className="mc-board-filter-bar">
        <label>
          Owner
          <select
            value={ownerFilter}
            onChange={(event) => setOwnerFilter(event.target.value as OwnerFilter)}
          >
            <option value="all">All</option>
            <option value="unassigned">Unassigned</option>
            {agentOwners.map((agentId) => (
              <option key={agentId} value={agentId}>
                {agents.find((a) => a.agent_id === agentId)?.name ?? agentId}
              </option>
            ))}
          </select>
        </label>
      </div>

      {loading ? <div className="mc-board-loading">Loading board\u2026</div> : null}
      <div className="mc-board-scroll" ref={boardScrollerRef}>
        <div
          className="mc-board-canvas"
          style={{ width: `${columnVirtualizer.getTotalSize()}px` }}
        >
          {columnVirtualizer.getVirtualItems().map((virtualColumn: { index: number; start: number }) => {
            const column = columns[virtualColumn.index];
            const cards = filteredCardsByColumn.get(column.column_id) ?? [];
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
                  onSelectCard={(cardId) => {
                    setAssetsPage(1);
                    onSelectCard(cardId);
                  }}
                  onDropCard={onDropCard}
                  onCreateCard={onCreateCard}
                  strategyReady={strategyReady}
                  linkedTaskByCardId={linkedTaskByCardId}
                  onOpenStrategyTask={onOpenStrategyTask}
                  runbookEnabled={runbookEnabled}
                  runbookByCardId={runbookByCardId}
                  onOpenBoardCardRunbook={onOpenBoardCardRunbook}
                />
              </div>
            );
          })}
        </div>
      </div>

      {/* ── Card editor modal with sub-tabs ── */}
      <Modal
        open={cardEditorOpen}
        onClose={() => onSelectCard(null)}
        title={selectedCard?.title ?? "Card Editor"}
        subtitle={selectedCard?.latest_run_id ? `run: ${selectedCard.latest_run_id}` : undefined}
        width="680px"
        footer={
          <>
            <button type="button" className="ghost" onClick={() => onSelectCard(null)}>
              Close
            </button>
            <button type="button" disabled={editorBusy} onClick={() => void onRunCard()}>
              {editorBusyAction === "run" ? "Running..." : "Run Card"}
            </button>
            <button type="button" disabled={editorBusy} onClick={() => void onSaveCardDraft()}>
              {editorBusyAction === "save" ? "Saving..." : "Save Card"}
            </button>
          </>
        }
      >
        <Tabs
          tabs={[
            { id: "details", label: "Details" },
            { id: "script", label: "Script" },
            { id: "assets", label: "Assets", count: selectedCard?.assets.length },
          ]}
          activeTab={editorTab}
          onTabChange={(id) => setEditorTab(id as "details" | "script" | "assets")}
        />

        {editorTab === "details" ? (
          <div className="mc-card-editor-details">
            <label className="mc-modal-field">
              Title
              <input
                disabled={editorBusy}
                value={cardEditor.title}
                onChange={(event) =>
                  setCardEditor((previous) => ({
                    ...previous,
                    title: event.target.value,
                  }))
                }
              />
            </label>

            <label className="mc-modal-field">
              Description
              <textarea
                disabled={editorBusy}
                value={cardEditor.description}
                onChange={(event) =>
                  setCardEditor((previous) => ({
                    ...previous,
                    description: event.target.value,
                  }))
                }
                rows={3}
              />
            </label>
            {strategyReady ? (
              <StrategyTaskContextPanel
                className="mc-board-strategy-panel"
                task={linkedTask}
                context={linkedTaskContext}
                emptyMessage="Link this board card from Strategy to expose project, owner, and manager context here without changing board execution flow."
                onOpen={
                  linkedTask ? () => onOpenStrategyTask(linkedTask.task_id) : undefined
                }
              />
            ) : null}
            {runbookEnabled ? (
              <RunbookLinkPanel
                className="mc-board-runbook-panel"
                summary={selectedCardRunbook}
                emptyMessage="Runbook appears once this card has a linked run, session, or strategy execution path."
                onOpen={
                  selectedCard
                    ? () => onOpenBoardCardRunbook(selectedCard.card_id)
                    : undefined
                }
              />
            ) : null}

            <label className="mc-modal-field">
              Owner
              <select
                disabled={editorBusy}
                value={
                  cardEditor.ownerKind === "agent" && cardEditor.ownerAgentId
                    ? `agent:${cardEditor.ownerAgentId}`
                    : cardEditor.ownerKind
                }
                onChange={(event) => {
                  const val = event.target.value;
                  if (val.startsWith("agent:")) {
                    setCardEditor((previous) => ({
                      ...previous,
                      ownerKind: "agent",
                      ownerAgentId: val.slice(6),
                    }));
                  } else if (val === "human") {
                    setCardEditor((previous) => ({
                      ...previous,
                      ownerKind: "human",
                      ownerAgentId: "",
                    }));
                  } else {
                    setCardEditor((previous) => ({
                      ...previous,
                      ownerKind: "unassigned",
                      ownerAgentId: "",
                      ownerHumanId: "",
                    }));
                  }
                }}
              >
                <option value="unassigned">Unassigned</option>
                {agents.map((agent) => (
                  <option key={agent.agent_id} value={`agent:${agent.agent_id}`}>
                    {agent.name || agent.agent_id}
                  </option>
                ))}
                <option value="human">Human (custom)</option>
              </select>
            </label>
            {cardEditor.ownerKind === "human" ? (
              <label className="mc-modal-field">
                Human ID
                <input
                  disabled={editorBusy}
                  value={cardEditor.ownerHumanId}
                  onChange={(event) =>
                    setCardEditor((previous) => ({
                      ...previous,
                      ownerHumanId: event.target.value,
                    }))
                  }
                />
              </label>
            ) : null}

            <div className="mc-field-grid">
              <label className="mc-modal-field">
                Move To Column
                <div className="mc-board-inline-actions">
                  <select
                    disabled={editorBusy}
                    value={moveTargetColumnId}
                    onChange={(event) => setMoveTargetColumnId(event.target.value)}
                  >
                    {columns.map((column) => (
                      <option key={column.column_id} value={column.column_id}>
                        {column.name}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="ghost"
                    disabled={!canMoveSelectedCard}
                    onClick={() => void onMoveCardToColumn(moveTargetColumnId)}
                  >
                    {editorBusyAction === "move" ? "Moving..." : "Move"}
                  </button>
                </div>
              </label>
              <label className="mc-modal-field">
                Due
                <input
                  type="datetime-local"
                  disabled={editorBusy}
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

            <TagPicker
              label="Tags"
              value={cardEditor.tagsCsv}
              disabled={editorBusy}
              onChange={(next) =>
                setCardEditor((previous) => ({
                  ...previous,
                  tagsCsv: next,
                }))
              }
              suggestions={knownTags}
            />
          </div>
        ) : editorTab === "script" ? (
          <div className="mc-card-editor-script">
            <textarea
              className="mc-script-area"
              disabled={editorBusy}
              value={cardEditor.scriptMarkdown}
              onChange={(event) =>
                setCardEditor((previous) => ({
                  ...previous,
                  scriptMarkdown: event.target.value,
                }))
              }
              placeholder="Write agent instructions in markdown..."
            />
          </div>
        ) : (
          <div className="mc-card-editor-assets">
            <label className="upload-pill">
              <input
                type="file"
                disabled={editorBusy}
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (!file) return;
                  void onUploadAsset(file);
                  event.currentTarget.value = "";
                }}
              />
              Upload Asset
            </label>
            {editorBusyAction === "upload" ? (
              <p className="mc-board-inline-hint">Uploading asset...</p>
            ) : null}
            <ul className="mc-asset-list">
              {visibleAssets.map((asset) => (
                <li key={asset.card_asset_id}>
                  <button
                    type="button"
                    onClick={() => {
                      if (selectedCard) void onPreviewAsset(selectedCard.card_id, asset.card_asset_id);
                    }}
                  >
                    {asset.filename}
                  </button>
                  <span>{formatBytes(asset.bytes)}</span>
                </li>
              ))}
              {(selectedCard?.assets ?? []).length === 0 ? (
                <li className="mc-empty-drawer">No assets uploaded yet.</li>
              ) : null}
            </ul>
            <Pagination
              currentPage={assetsPage}
              totalPages={assetsPagination.totalPages}
              onPageChange={setAssetsPage}
            />
            {selectedPreviewUrl ? (
              <div className="mc-preview-wrap">
                <img src={selectedPreviewUrl} alt="asset preview" />
              </div>
            ) : null}
          </div>
        )}
      </Modal>

      {/* ── New Card creation modal ── */}
      <Modal
        open={createModalOpen}
        onClose={() => setCreateModalOpen(false)}
        title="New Card"
        subtitle="Create a card in the selected column"
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setCreateModalOpen(false)}>
              Cancel
            </button>
            <button type="button" disabled={!canCreateCard} onClick={() => void handleSubmitCreate()}>
              Create Card
            </button>
          </>
        }
      >
        <label className="mc-modal-field">
          Title
          <input
            value={newCardTitle}
            onChange={(event) => setNewCardTitle(event.target.value)}
            placeholder="Card title"
            autoFocus
          />
        </label>
        <label className="mc-modal-field">
          Column
          <select
            value={newCardColumnId}
            onChange={(event) => setNewCardColumnId(event.target.value)}
          >
            {columns.map((col) => (
              <option key={col.column_id} value={col.column_id}>
                {col.name}
              </option>
            ))}
          </select>
        </label>
        <label className="mc-modal-field">
          Owner
          <select
            value={newCardOwnerKind}
            onChange={(event) => {
              const nextKind = event.target.value;
              setNewCardOwnerKind(nextKind);
              if (nextKind !== "agent") {
                setNewCardOwnerAgentId("");
              }
              if (nextKind !== "human") {
                setNewCardOwnerHumanId("");
              }
            }}
          >
            <option value="unassigned">unassigned</option>
            <option value="agent">agent</option>
            <option value="human">human</option>
          </select>
        </label>
        {newCardOwnerKind === "agent" ? (
          <label className="mc-modal-field">
            Agent
            <select
              value={newCardOwnerAgentId}
              onChange={(event) => setNewCardOwnerAgentId(event.target.value)}
            >
              <option value="">none</option>
              {agents.map((agent) => (
                <option key={agent.agent_id} value={agent.agent_id}>
                  {agent.name || agent.agent_id} ({agent.agent_id})
                </option>
              ))}
            </select>
          </label>
        ) : null}
        {newCardOwnerKind === "human" ? (
          <label className="mc-modal-field">
            Human ID
            <input
              value={newCardOwnerHumanId}
              onChange={(event) => setNewCardOwnerHumanId(event.target.value)}
              placeholder="human owner id"
            />
          </label>
        ) : null}
      </Modal>
    </section>
  );
}
