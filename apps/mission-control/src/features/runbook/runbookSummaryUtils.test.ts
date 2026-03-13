import { describe, expect, it } from "vitest";
import type { RunbookSummaryItemResponse } from "../../types";
import {
  buildRunbookSummaryIndex,
  getRunbookAttentionItems,
  getRunbookCurrentStepLabel,
  getRunbookStatusTone,
  getRunbookSummariesForEntity,
} from "./runbookSummaryUtils";

function makeItem(
  overrides: Partial<RunbookSummaryItemResponse>
): RunbookSummaryItemResponse {
  return {
    runbook_id: "strategy_task_execution:task-1",
    runbook_kind: "strategy_task_execution",
    anchor_kind: "task",
    anchor_id: "task-1",
    title: "Task one",
    status: "blocked",
    status_reason: "Awaiting approval.",
    owner_agent_id: "agent-1",
    owner_agent_label: "Lyra",
    primary_entity_label: "Task one",
    updated_at_ms: 10,
    current_step_label: "Await approval",
    warning_count: 0,
    linked_entities: [
      {
        entity_kind: "task",
        entity_id: "task-1",
        display_label: "Task one",
        deep_link: {
          tab: "strategy",
          target_kind: "task",
          target_id: "task-1",
          context: null,
        },
      },
    ],
    availability: {
      is_limited: false,
      is_stale: false,
      last_refresh_at_ms: 10,
      missing_source_kinds: [],
      stale_reason: null,
    },
    ...overrides,
  };
}

describe("runbookSummaryUtils", () => {
  it("indexes anchors and linked entities across runbook kinds", () => {
    const items = [
      makeItem({}),
      makeItem({
        runbook_id: "board_card_run:card-1",
        runbook_kind: "board_card_run",
        anchor_kind: "card",
        anchor_id: "card-1",
        linked_entities: [
          {
            entity_kind: "board_card",
            entity_id: "card-1",
            display_label: "Card one",
            deep_link: {
              tab: "boards",
              target_kind: "card",
              target_id: "card-1",
              context: null,
            },
          },
          {
            entity_kind: "task",
            entity_id: "task-1",
            display_label: "Task one",
            deep_link: {
              tab: "strategy",
              target_kind: "task",
              target_id: "task-1",
              context: null,
            },
          },
        ],
      }),
      makeItem({
        runbook_id: "assistant_session_run:run-1",
        runbook_kind: "assistant_session_run",
        anchor_kind: "run",
        anchor_id: "run-1",
        linked_entities: [
          {
            entity_kind: "session",
            entity_id: "session-1",
            display_label: "Session one",
            deep_link: {
              tab: "assistant",
              target_kind: "session",
              target_id: "session-1",
              context: null,
            },
          },
          {
            entity_kind: "approval",
            entity_id: "approval-1",
            display_label: "Approval one",
            deep_link: {
              tab: "focus",
              target_kind: "approval",
              target_id: "approval-1",
              context: null,
            },
          },
        ],
      }),
    ];

    const index = buildRunbookSummaryIndex(items);

    expect(index.byTaskId.get("task-1")?.runbook_id).toBe("board_card_run:card-1");
    expect(index.byBoardCardId.get("card-1")?.runbook_id).toBe("board_card_run:card-1");
    expect(index.byRunId.get("run-1")?.runbook_id).toBe("assistant_session_run:run-1");
    expect(index.bySessionId.get("session-1")?.runbook_id).toBe(
      "assistant_session_run:run-1"
    );
    expect(index.byApprovalId.get("approval-1")?.runbook_id).toBe(
      "assistant_session_run:run-1"
    );
    expect(getRunbookSummariesForEntity(index, "task", "task-1")).toHaveLength(3);
  });

  it("derives compact display helpers", () => {
    const item = makeItem({
      status: "waiting",
      current_step_label: null,
      status_reason: "Waiting on approval.",
    });

    expect(getRunbookStatusTone("waiting")).toBe("warning");
    expect(getRunbookCurrentStepLabel(item)).toBe("Waiting on approval.");
    expect(getRunbookAttentionItems([item], 5)).toEqual([item]);
  });
});
