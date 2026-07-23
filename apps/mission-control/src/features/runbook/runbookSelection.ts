import type { RunbookSummaryItemResponse } from "../../types";

export function chooseRunbookSelection(
  filteredItems: RunbookSummaryItemResponse[],
  allItems: RunbookSummaryItemResponse[],
  selectedRunbookId: string,
): RunbookSummaryItemResponse | null {
  return (
    filteredItems.find((item) => item.runbook_id === selectedRunbookId) ??
    allItems.find((item) => item.runbook_id === selectedRunbookId) ??
    filteredItems[0] ??
    null
  );
}
