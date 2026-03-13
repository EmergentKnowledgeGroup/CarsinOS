export const RUNBOOK_FETCH_PAGE_LIMIT = 200;
export const RUNBOOK_REFRESH_DEBOUNCE_MS = 250;
export const RUNBOOK_STALE_THRESHOLD_MS = 30_000;
export const RUNBOOK_HISTORY_PREVIEW_LIMIT = 12;
export const RUNBOOK_COCKPIT_ATTENTION_LIMIT = 24;

export const RUNBOOK_UNSUPPORTED_STATUS_FRAGMENTS = [
  "404",
  "405",
  "501",
  "not found",
  "method not allowed",
];

export const RUNBOOK_KIND_OPTIONS = [
  { value: "all", label: "All runbooks" },
  { value: "assistant_session_run", label: "Assistant runs" },
  { value: "board_card_run", label: "Board card runs" },
  { value: "scheduled_job_run", label: "Scheduled jobs" },
  { value: "strategy_task_execution", label: "Strategy tasks" },
] as const;

export const RUNBOOK_STATUS_OPTIONS = [
  { value: "all", label: "All statuses" },
  { value: "pending", label: "Pending" },
  { value: "active", label: "Active" },
  { value: "waiting", label: "Waiting" },
  { value: "blocked", label: "Blocked" },
  { value: "failed", label: "Failed" },
  { value: "completed", label: "Completed" },
  { value: "limited", label: "Limited" },
] as const;
