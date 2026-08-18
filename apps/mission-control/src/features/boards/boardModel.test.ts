import { describe, expect, it } from "vitest";
import type { BoardCard, BoardColumn, BoardDetail, BoardSummary } from "../../types";
import { toCardsByColumn, withOptimisticMove, withUpsertCard } from "./boardModel";

function makeBoardSummary(partial: Partial<BoardSummary> = {}): BoardSummary {
  return {
    board_id: partial.board_id ?? "board-1",
    board_key: partial.board_key ?? "board",
    name: partial.name ?? "Board",
    board_type: partial.board_type ?? "kanban",
    created_at: partial.created_at ?? 0,
    updated_at: partial.updated_at ?? 0,
    column_count: partial.column_count ?? 2,
    card_count: partial.card_count ?? 3,
  };
}

function makeColumn(partial: Partial<BoardColumn>): BoardColumn {
  return {
    column_id: partial.column_id ?? "todo",
    board_id: partial.board_id ?? "board-1",
    column_key: partial.column_key ?? partial.column_id ?? "todo",
    name: partial.name ?? "Todo",
    position: partial.position ?? 0,
    created_at: partial.created_at ?? 0,
    updated_at: partial.updated_at ?? 0,
  };
}

function makeCard(partial: Partial<BoardCard>): BoardCard {
  return {
    card_id: partial.card_id ?? "card-1",
    board_id: partial.board_id ?? "board-1",
    column_id: partial.column_id ?? "todo",
    title: partial.title ?? "Card",
    description: partial.description ?? null,
    owner_kind: partial.owner_kind ?? "unassigned",
    owner_agent_id: partial.owner_agent_id ?? null,
    owner_human_id: partial.owner_human_id ?? null,
    due_at: partial.due_at ?? null,
    tags: partial.tags ?? [],
    script_markdown: partial.script_markdown ?? null,
    linked_session_id: partial.linked_session_id ?? null,
    latest_run_id: partial.latest_run_id ?? null,
    position: partial.position ?? 0,
    created_at: partial.created_at ?? 0,
    updated_at: partial.updated_at ?? 0,
    assets: partial.assets ?? [],
  };
}

function makeBoard(cards: BoardCard[]): BoardDetail {
  return {
    board: makeBoardSummary(),
    columns: [
      makeColumn({ column_id: "todo", name: "Todo", position: 0 }),
      makeColumn({ column_id: "doing", name: "Doing", position: 1 }),
    ],
    cards,
  };
}

describe("boardModel", () => {
  it("groups cards by column and sorts by position", () => {
    const board = makeBoard([
      makeCard({ card_id: "b", column_id: "todo", position: 2 }),
      makeCard({ card_id: "a", column_id: "todo", position: 1 }),
      makeCard({ card_id: "c", column_id: "doing", position: 0 }),
    ]);

    const grouped = toCardsByColumn(board);
    expect(grouped.get("todo")?.map((card) => card.card_id)).toEqual(["a", "b"]);
    expect(grouped.get("doing")?.map((card) => card.card_id)).toEqual(["c"]);
  });

  it("upserts a card and preserves positional ordering", () => {
    const board = makeBoard([
      makeCard({ card_id: "a", position: 0 }),
      makeCard({ card_id: "b", position: 2 }),
    ]);

    const next = withUpsertCard(board, makeCard({ card_id: "b", title: "Updated", position: 1 }));
    expect(next.cards.map((card) => `${card.card_id}:${card.position}`)).toEqual(["a:0", "b:1"]);
    expect(next.cards.find((card) => card.card_id === "b")?.title).toBe("Updated");
  });

  it("moves a card optimistically across columns and reindexes positions", () => {
    const board = makeBoard([
      makeCard({ card_id: "a", column_id: "todo", position: 0 }),
      makeCard({ card_id: "b", column_id: "todo", position: 1 }),
      makeCard({ card_id: "c", column_id: "doing", position: 0 }),
    ]);

    const moved = withOptimisticMove(board, "b", "doing", "c");

    const todoCards = moved.cards
      .filter((card) => card.column_id === "todo")
      .sort((left, right) => left.position - right.position)
      .map((card) => `${card.card_id}:${card.position}`);
    const doingCards = moved.cards
      .filter((card) => card.column_id === "doing")
      .sort((left, right) => left.position - right.position)
      .map((card) => `${card.card_id}:${card.position}`);

    expect(todoCards).toEqual(["a:0"]);
    expect(doingCards).toEqual(["b:0", "c:1"]);
  });
});
