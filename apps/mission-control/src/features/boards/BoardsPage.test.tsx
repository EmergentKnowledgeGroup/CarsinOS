// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { emptyEditorDraft } from "./boardModel";
import { BoardsPage } from "./BoardsPage";

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
  localStorage.clear();
});

describe("BoardsPage Trenches affordances", () => {
  it("offers Pin to Office from the board toolbar", async () => {
    await act(async () => {
      root = createRoot(container);
      root.render(
        <BoardsPage
          boards={[]}
          activeBoardId={null}
          onBoardChange={async () => {}}
          columns={[]}
          cardsByColumn={new Map()}
          selectedCardId={null}
          dragCardId={null}
          setDragCardId={() => {}}
          onSelectCard={() => {}}
          onDropCard={() => {}}
          onCreateCard={async () => true}
          selectedCard={null}
          cardEditor={emptyEditorDraft()}
          setCardEditor={() => {}}
          agents={[]}
          onSaveCardDraft={async () => {}}
          onRunCard={async () => {}}
          onMoveCardToColumn={async () => {}}
          onUploadAsset={async () => {}}
          onPreviewAsset={async () => {}}
          selectedPreviewUrl={null}
          editorBusy={false}
          editorBusyAction={null}
          strategyReady={false}
          linkedTaskByCardId={new Map()}
          describeStrategyTask={() => null}
          onOpenStrategyTask={() => false}
          runbookEnabled={false}
          runbookByCardId={new Map()}
          onOpenBoardCardRunbook={() => false}
        />,
      );
    });

    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) => button.getAttribute("aria-label") === "Pin Boards to Office",
    );
    expect(pin).toBeTruthy();
  });
});
