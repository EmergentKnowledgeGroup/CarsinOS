import type { BoardCard, BoardDetail } from "../../types";

export interface CardEditorDraft {
  title: string;
  description: string;
  ownerKind: string;
  ownerAgentId: string;
  ownerHumanId: string;
  dueAt: string;
  tagsCsv: string;
  scriptMarkdown: string;
}

export function emptyEditorDraft(): CardEditorDraft {
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

export function toCardsByColumn(board: BoardDetail | null): Map<string, BoardCard[]> {
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

export function withUpsertCard(board: BoardDetail, nextCard: BoardCard): BoardDetail {
  const cards = board.cards.filter((card) => card.card_id !== nextCard.card_id);
  cards.push(nextCard);
  cards.sort((a, b) => a.position - b.position);
  return {
    ...board,
    cards,
  };
}

export function withOptimisticMove(
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
