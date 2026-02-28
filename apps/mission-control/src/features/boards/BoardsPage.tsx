import { useRef, type Dispatch, type SetStateAction } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Agent, BoardCard, BoardColumn } from "../../types";
import { formatBytes } from "../../utils/files";
import { EmptyState } from "../../ui/EmptyState";
import { BoardLane } from "./BoardLane";
import type { CardEditorDraft } from "./boardModel";

interface BoardsPageProps {
  boards: Array<{ board_id: string; name: string }>;
  activeBoardId: string | null;
  onBoardChange: (boardId: string) => Promise<void>;
  columns: BoardColumn[];
  cardsByColumn: Map<string, BoardCard[]>;
  selectedCardId: string | null;
  dragCardId: string | null;
  setDragCardId: (value: string | null) => void;
  onSelectCard: (cardId: string) => void;
  onDropCard: (cardId: string, columnId: string, beforeCardId?: string) => void;
  onCreateCard: (columnId: string, title: string) => Promise<void>;
  selectedCard: BoardCard | null;
  cardEditor: CardEditorDraft;
  setCardEditor: Dispatch<SetStateAction<CardEditorDraft>>;
  agents: Agent[];
  onSaveCardDraft: () => Promise<void>;
  onRunCard: () => Promise<void>;
  onUploadAsset: (file: File) => Promise<void>;
  onPreviewAsset: (cardId: string, cardAssetId: string) => Promise<void>;
  selectedPreviewUrl: string | null;
}

export function BoardsPage({
  boards,
  activeBoardId,
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
  onUploadAsset,
  onPreviewAsset,
  selectedPreviewUrl,
}: BoardsPageProps) {
  const boardScrollerRef = useRef<HTMLDivElement | null>(null);
  // eslint-disable-next-line react-hooks/incompatible-library
  const columnVirtualizer = useVirtualizer({
    count: columns.length,
    horizontal: true,
    getScrollElement: () => boardScrollerRef.current,
    estimateSize: () => 320,
    overscan: 2,
  });

  return (
    <section className="mc-main-grid">
      <section className="mc-board-panel">
        <div className="mc-board-toolbar">
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
        </div>

        <div className="mc-board-scroll" ref={boardScrollerRef}>
          <div
            className="mc-board-canvas"
            style={{ width: `${columnVirtualizer.getTotalSize()}px` }}
          >
            {columnVirtualizer.getVirtualItems().map((virtualColumn: { index: number; start: number }) => {
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
                    onSelectCard={onSelectCard}
                    onDropCard={onDropCard}
                    onCreateCard={onCreateCard}
                  />
                </div>
              );
            })}
          </div>
        </div>
      </section>

      <aside className="mc-drawer">
        {!selectedCard ? (
          <EmptyState className="mc-empty-drawer" message="Select a card to edit and run." />
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
              <button type="button" onClick={() => void onSaveCardDraft()}>
                Save Card
              </button>
              <button type="button" onClick={() => void onRunCard()}>
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
                    void onUploadAsset(file);
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
                      onClick={() => void onPreviewAsset(selectedCard!.card_id, asset.card_asset_id)}
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
  );
}
