import { describe, expect, it } from "vitest";
import type { RunbookSummaryItemResponse } from "../../types";
import { chooseRunbookSelection } from "./runbookSelection";

function item(
  runbookId: string,
  runbookKind: string,
  anchorId: string,
): RunbookSummaryItemResponse {
  return {
    runbook_id: runbookId,
    runbook_kind: runbookKind,
    anchor_kind: "assistant_run",
    anchor_id: anchorId,
    title: runbookId,
    status: "active",
    status_reason: "Running.",
    owner_agent_id: null,
    owner_agent_label: null,
    primary_entity_label: runbookId,
    updated_at_ms: 1,
    current_step_label: "Running",
    warning_count: 0,
    linked_entities: [],
    availability: {
      is_limited: false,
      is_stale: false,
      last_refresh_at_ms: 1,
      missing_source_kinds: [],
      stale_reason: null,
    },
  };
}

describe("chooseRunbookSelection", () => {
  it("preserves an explicit target that is outside the active filter", () => {
    const filtered = item("board_card_run:card-1", "board_card_run", "card-1");
    const requested = item(
      "assistant_session_run:run-1",
      "assistant_session_run",
      "run-1",
    );

    expect(
      chooseRunbookSelection(
        [filtered],
        [filtered, requested],
        requested.runbook_id,
      ),
    ).toBe(requested);
  });
});
