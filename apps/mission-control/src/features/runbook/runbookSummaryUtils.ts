import type {
  RunbookEntityRefResponse,
  RunbookSummaryItemResponse,
} from "../../types";

export type RunbookStatusTone = "up" | "down" | "warning" | "checking" | "";

export interface RunbookSummaryIndex {
  byRunbookId: Map<string, RunbookSummaryItemResponse>;
  byTaskId: Map<string, RunbookSummaryItemResponse>;
  byBoardCardId: Map<string, RunbookSummaryItemResponse>;
  byJobId: Map<string, RunbookSummaryItemResponse>;
  byRunId: Map<string, RunbookSummaryItemResponse>;
  bySessionId: Map<string, RunbookSummaryItemResponse>;
  byApprovalId: Map<string, RunbookSummaryItemResponse>;
  byEntityKey: Map<string, RunbookSummaryItemResponse[]>;
}

function entityKey(entityKind: string, entityId: string): string {
  return `${entityKind}:${entityId}`;
}

function setAnchorEntry(
  target: Map<string, RunbookSummaryItemResponse>,
  key: string | null,
  item: RunbookSummaryItemResponse
) {
  if (!key) {
    return;
  }
  target.set(key, item);
}

function addEntityEntry(
  target: Map<string, RunbookSummaryItemResponse[]>,
  entity: RunbookEntityRefResponse | { entity_kind: string; entity_id: string },
  item: RunbookSummaryItemResponse
) {
  if (!entity.entity_kind || !entity.entity_id) {
    return;
  }
  const key = entityKey(entity.entity_kind, entity.entity_id);
  const current = target.get(key);
  if (current) {
    if (current.some((entry) => entry.runbook_id === item.runbook_id)) {
      return;
    }
    current.push(item);
    return;
  }
  target.set(key, [item]);
}

/**
 * Anchor maps use last-wins semantics via setAnchorEntry, while byEntityKey
 * intentionally accumulates every matching item via addEntityEntry.
 */
export function buildRunbookSummaryIndex(
  items: RunbookSummaryItemResponse[]
): RunbookSummaryIndex {
  const index: RunbookSummaryIndex = {
    byRunbookId: new Map(),
    byTaskId: new Map(),
    byBoardCardId: new Map(),
    byJobId: new Map(),
    byRunId: new Map(),
    bySessionId: new Map(),
    byApprovalId: new Map(),
    byEntityKey: new Map(),
  };

  for (const item of items) {
    index.byRunbookId.set(item.runbook_id, item);
    addEntityEntry(index.byEntityKey, {
      entity_kind: item.anchor_kind,
      entity_id: item.anchor_id,
    }, item);
    if (item.runbook_kind === "strategy_task_execution") {
      setAnchorEntry(index.byTaskId, item.anchor_id, item);
    }
    if (item.runbook_kind === "board_card_run") {
      setAnchorEntry(index.byBoardCardId, item.anchor_id, item);
    }
    if (item.runbook_kind === "scheduled_job_run") {
      setAnchorEntry(index.byJobId, item.anchor_id, item);
    }
    if (item.runbook_kind === "assistant_session_run") {
      setAnchorEntry(index.byRunId, item.anchor_id, item);
    }
    for (const entity of item.linked_entities) {
      addEntityEntry(index.byEntityKey, entity, item);
      switch (entity.entity_kind) {
        case "task":
          setAnchorEntry(index.byTaskId, entity.entity_id, item);
          break;
        case "board_card":
        case "card":
          setAnchorEntry(index.byBoardCardId, entity.entity_id, item);
          break;
        case "job":
          setAnchorEntry(index.byJobId, entity.entity_id, item);
          break;
        case "run":
          setAnchorEntry(index.byRunId, entity.entity_id, item);
          break;
        case "session":
          setAnchorEntry(index.bySessionId, entity.entity_id, item);
          break;
        case "approval":
          setAnchorEntry(index.byApprovalId, entity.entity_id, item);
          break;
        default:
          break;
      }
    }
  }

  return index;
}

export function getRunbookSummariesForEntity(
  index: RunbookSummaryIndex,
  entityKind: string,
  entityId: string | null | undefined
): RunbookSummaryItemResponse[] {
  const trimmedEntityId = entityId?.trim();
  if (!trimmedEntityId) {
    return [];
  }
  return index.byEntityKey.get(entityKey(entityKind, trimmedEntityId)) ?? [];
}

export function getRunbookStatusTone(status: string): RunbookStatusTone {
  switch (status) {
    case "completed":
      return "up";
    case "failed":
    case "blocked":
      return "down";
    case "waiting":
    case "limited":
      return "warning";
    case "active":
      return "checking";
    default:
      return "";
  }
}

export function getRunbookCurrentStepLabel(
  item: RunbookSummaryItemResponse | null
): string | null {
  if (!item) {
    return null;
  }
  return item.current_step_label ?? item.status_reason ?? null;
}

export function getRunbookAttentionItems(
  items: RunbookSummaryItemResponse[],
  limit: number,
  statuses: string[] = ["waiting", "blocked", "failed", "limited", "active"]
): RunbookSummaryItemResponse[] {
  const allowed = new Set(statuses);
  return items.filter((item) => allowed.has(item.status)).slice(0, limit);
}
