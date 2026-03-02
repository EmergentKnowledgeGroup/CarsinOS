import { useState } from "react";
import clsx from "clsx";
import { Bot, User, HelpCircle } from "lucide-react";
import type { BoardCard, BoardColumn } from "../../types";
import { Pagination } from "../../ui/Pagination";
import { usePagination } from "../../ui/usePagination";

function OwnerIcon({ kind }: { kind: string }) {
  switch (kind) {
    case "agent": return <Bot size={12} />;
    case "human": return <User size={12} />;
    default: return <HelpCircle size={12} />;
  }
}

const LANE_PAGE_SIZE = 8;

export interface BoardLaneProps {
  column: BoardColumn;
  cards: BoardCard[];
  selectedCardId: string | null;
  dragCardId: string | null;
  setDragCardId: (value: string | null) => void;
  onSelectCard: (cardId: string | null) => void;
  onDropCard: (cardId: string, columnId: string, beforeCardId?: string) => void;
  onCreateCard: (columnId: string, title: string) => Promise<boolean>;
}

export function BoardLane(props: BoardLaneProps) {
  const [newCardTitle, setNewCardTitle] = useState("");
  const [page, setPage] = useState(1);

  const { totalPages, getPage } = usePagination(props.cards, LANE_PAGE_SIZE);
  const visibleCards = getPage(page);

  const submitCreate = async () => {
    const title = newCardTitle.trim();
    if (!title) {
      return;
    }
    const created = await props.onCreateCard(props.column.column_id, title);
    if (created) {
      setNewCardTitle("");
    }
  };

  return (
    <section className="mc-lane">
      <header className="mc-lane-header">
        <h3>{props.column.name}</h3>
        <span>{props.cards.length}</span>
      </header>

      <div
        className="mc-lane-body"
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
        {visibleCards.map((card) => (
          <article
            key={card.card_id}
            className={clsx("mc-card", {
              "mc-card-selected": props.selectedCardId === card.card_id,
            })}
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
            <div className={clsx("mc-card-bar", `mc-card-bar-${card.owner_kind}`)} />
            <div className="mc-card-body">
              <div className="mc-card-title">{card.title}</div>
              <div className="mc-card-meta">
                <span><OwnerIcon kind={card.owner_kind} /> {card.owner_kind}</span>
                {card.latest_run_id ? <span className="mc-card-run">run: {card.latest_run_id}</span> : null}
              </div>
            </div>
          </article>
        ))}
        {visibleCards.length === 0 ? (
          <div className="mc-lane-empty">No cards</div>
        ) : null}
      </div>

      {totalPages > 1 ? (
        <Pagination currentPage={page} totalPages={totalPages} onPageChange={setPage} />
      ) : null}

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
