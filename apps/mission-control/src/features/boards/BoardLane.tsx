import { useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import clsx from "clsx";
import type { BoardCard, BoardColumn } from "../../types";

export interface BoardLaneProps {
  column: BoardColumn;
  cards: BoardCard[];
  selectedCardId: string | null;
  dragCardId: string | null;
  setDragCardId: (value: string | null) => void;
  onSelectCard: (cardId: string) => void;
  onDropCard: (cardId: string, columnId: string, beforeCardId?: string) => void;
  onCreateCard: (columnId: string, title: string) => Promise<void>;
}

export function BoardLane(props: BoardLaneProps) {
  const [newCardTitle, setNewCardTitle] = useState("");
  const listRef = useRef<HTMLDivElement | null>(null);

  // eslint-disable-next-line react-hooks/incompatible-library
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
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void submitCreate();
            }
          }}
          placeholder="Add card"
        />
        <button type="button" onClick={submitCreate}>
          Add
        </button>
      </div>
    </section>
  );
}
