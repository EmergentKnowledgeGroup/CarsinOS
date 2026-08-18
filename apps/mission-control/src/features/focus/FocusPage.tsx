import { useRef, useState } from "react";
import clsx from "clsx";
import { ChevronDown, ChevronRight } from "lucide-react";
import type {
  ChannelRuntimeAdapterStatusResponse,
  CircuitBreakerStateResponse,
  JobStatusResponse,
  MissionControlCalendarJob,
  MissionControlFocusItem,
  PluginRuntimeStatusResponse,
  RunbookSummaryItemResponse,
  TaskResponse,
} from "../../types";
import { Chip } from "../../ui/Chip";
import { EmptyState } from "../../ui/EmptyState";
import { InlineActions } from "../../ui/InlineActions";
import { Pagination } from "../../ui/Pagination";
import { Surface } from "../../ui/Surface";
import { Tabs } from "../../ui/Tabs";
import { usePagination } from "../../ui/usePagination";
import { formatDateTime, formatRelative } from "../../utils/datetime";
import { redactSecrets } from "../../lib/redaction";
import { PinRoomToOffice } from "../execassOffice/PinRoomToOffice";
import { RunbookLinkPanel } from "../runbook/RunbookLinkPanel";
import { StrategyTaskContextPanel } from "../strategy/StrategyTaskContextPanel";
import type { StrategyTaskContextSnapshot } from "../strategy/useStrategyController";

const FOCUS_PAGE_SIZE = 6;
const SCHED_JOBS_PAGE_SIZE = 6;

type FocusSubTab = "queue" | "status" | "breakers" | "scheduler";

export interface FocusQueueIntent {
  nonce: number;
  targetKind: string | null;
  targetId: string | null;
}

/**
 * Landing section per stable room id. The Breakers & Scheduler room owns the
 * focus route, so entering it lands on the operations-facing section while
 * Queue and System Status stay one tap away.
 */
const ROOM_LANDING_TAB: Readonly<Partial<Record<string, FocusSubTab>>> = {
  breakers: "breakers",
};

/** Human schedule fact from the authoritative job fields, never inferred. */
function describeSchedule(job: MissionControlCalendarJob): string {
  if (job.cron_expr) {
    return `cron ${job.cron_expr}`;
  }
  if (job.interval_seconds !== null && job.interval_seconds > 0) {
    const seconds = job.interval_seconds;
    if (seconds < 60) return `every ${seconds}s`;
    if (seconds < 3600) return `every ${Math.round(seconds / 60)}m`;
    if (seconds < 86400) return `every ${Math.round(seconds / 3600)}h`;
    return `every ${Math.round(seconds / 86400)}d`;
  }
  return job.schedule_kind;
}

function formatContextDisplay(value: unknown): string {
  const tag = Object.prototype.toString.call(value);
  const isJsonLike = Array.isArray(value) || tag === "[object Object]";
  if (isJsonLike) {
    try {
      return JSON.stringify(redactSecrets(value), null, 2);
    } catch {
      return redactSecrets(String(value));
    }
  }
  return redactSecrets(String(value));
}

function focusItemMatchesIntent(
  item: MissionControlFocusItem,
  intent: FocusQueueIntent,
): boolean {
  const targetId = intent.targetId?.trim();
  if (!targetId) {
    return false;
  }
  const payload = item.action_payload;
  const kind = intent.targetKind?.trim().toLowerCase();
  const field =
    kind === "approval"
      ? "approval_id"
      : kind === "task"
        ? "task_id"
        : kind === "job"
          ? "job_id"
          : kind === "run"
            ? "run_id"
            : null;
  if (field) {
    return String(payload[field] ?? "").trim() === targetId;
  }
  return (
    item.item_id === targetId ||
    ["approval_id", "task_id", "job_id", "run_id"].some(
      (key) => String(payload[key] ?? "").trim() === targetId,
    )
  );
}

/** Extract human-readable context fields from an approval's action_payload. */
function extractApprovalContext(payload: Record<string, unknown>): Array<[string, string]> {
  const entries: Array<[string, string]> = [];
  const fields: Array<[string, string]> = [
    ["approval_kind", "Approval"],
    ["proposal_id", "Proposal"],
    ["audit_ref", "Audit"],
    ["run_id", "Run"],
    ["tool_name", "Tool"],
    ["tool_input", "Arguments"],
    ["request_summary", "Summary"],
    ["request", "Request"],
    ["requesting_agent", "Agent"],
    ["session_id", "Session"],
    ["agent_id", "Agent ID"],
    ["command", "Command"],
  ];
  for (const [key, label] of fields) {
    const value = payload[key];
    if (value !== undefined && value !== null && value !== "") {
      const display = formatContextDisplay(value);
      entries.push([label, display]);
    }
  }
  // Show remaining keys not already covered
  const coveredKeys = new Set(["approval_id", "job_id", "provider", ...fields.map(([k]) => k)]);
  for (const [key, value] of Object.entries(payload)) {
    if (!coveredKeys.has(key) && value !== undefined && value !== null && value !== "") {
      const display = formatContextDisplay(value);
      entries.push([key, display]);
    }
  }
  return entries;
}

interface FocusPageProps {
  /** Stable room id resolved by the elevator; drives the landing section. */
  activeRoomId: string | null;
  focusItems: MissionControlFocusItem[];
  approvalsCount: number;
  channelStatuses: ChannelRuntimeAdapterStatusResponse[];
  onResolveFocusApproval: (approvalId: string, decision: "approve" | "deny") => Promise<void>;
  onRunCalendarJobNow: (jobId: string) => Promise<void>;
  onReconnectFocusChannel: (provider: string) => Promise<void>;
  strategyReady: boolean;
  approvalTaskByApprovalId: Map<string, TaskResponse>;
  taskById: Map<string, TaskResponse>;
  taskByJobId: Map<string, TaskResponse>;
  describeStrategyTask: (taskId: string) => StrategyTaskContextSnapshot | null;
  onOpenStrategyTask: (taskId: string) => boolean;
  runbookEnabled: boolean;
  getRunbookForFocusItem: (
    item: MissionControlFocusItem
  ) => RunbookSummaryItemResponse | null;
  onOpenRunbookForFocusItem: (item: MissionControlFocusItem) => boolean;
  /** Authoritative scheduler/breaker status; null until the gateway answers. */
  jobsStatus: JobStatusResponse | null;
  openBreakers: CircuitBreakerStateResponse[];
  openPluginBreakers: PluginRuntimeStatusResponse[];
  calendarJobs: MissionControlCalendarJob[];
  onToggleCalendarJob: (jobId: string, enabled: boolean) => Promise<void>;
  /** Exact queue deep-link intent; a new nonce outranks the room landing. */
  queueIntent: FocusQueueIntent;
}

export function FocusPage(props: FocusPageProps) {
  const [subTab, setSubTab] = useState<FocusSubTab>(
    () =>
      (props.activeRoomId ? ROOM_LANDING_TAB[props.activeRoomId] : undefined) ??
      "queue",
  );
  // Only an actual room change may force a landing section; internal tab
  // clicks and unrelated rerenders must never reset the user's selection.
  const [lastRoomId, setLastRoomId] = useState(props.activeRoomId);
  if (props.activeRoomId !== lastRoomId) {
    setLastRoomId(props.activeRoomId);
    const landing = props.activeRoomId
      ? ROOM_LANDING_TAB[props.activeRoomId]
      : undefined;
    if (landing) {
      setSubTab(landing);
    }
  }
  const [lastQueueIntentNonce, setLastQueueIntentNonce] = useState(
    props.queueIntent.nonce,
  );
  const [pendingQueueIntent, setPendingQueueIntent] =
    useState<FocusQueueIntent | null>(null);
  const [focusPage, setFocusPage] = useState(1);
  const [schedJobsPage, setSchedJobsPage] = useState(1);
  const [expandedItems, setExpandedItems] = useState<Set<string>>(new Set());
  const [busyItems, setBusyItems] = useState<Set<string>>(new Set());
  const [actionErrors, setActionErrors] = useState<Map<string, string>>(new Map());
  const busyItemsRef = useRef<Set<string>>(new Set());
  // Queue deep links must preserve the requested entity, not merely open the
  // Queue tab. Keep an unmatched intent pending while live focus data loads.
  const incomingQueueIntent =
    props.queueIntent.nonce !== lastQueueIntentNonce
      ? props.queueIntent
      : pendingQueueIntent;
  if (props.queueIntent.nonce !== lastQueueIntentNonce) {
    setLastQueueIntentNonce(props.queueIntent.nonce);
    setSubTab("queue");
  }
  if (incomingQueueIntent) {
    const targetIndex = props.focusItems.findIndex((item) =>
      focusItemMatchesIntent(item, incomingQueueIntent),
    );
    if (targetIndex >= 0) {
      const targetItem = props.focusItems[targetIndex];
      setFocusPage(Math.floor(targetIndex / FOCUS_PAGE_SIZE) + 1);
      setExpandedItems((current) => {
        if (current.has(targetItem.item_id)) {
          return current;
        }
        const next = new Set(current);
        next.add(targetItem.item_id);
        return next;
      });
      if (pendingQueueIntent !== null) {
        setPendingQueueIntent(null);
      }
    } else if (incomingQueueIntent.targetId) {
      if (pendingQueueIntent?.nonce !== incomingQueueIntent.nonce) {
        setPendingQueueIntent(incomingQueueIntent);
      }
    } else if (pendingQueueIntent !== null) {
      setPendingQueueIntent(null);
    }
  }
  const focusPagination = usePagination(props.focusItems, FOCUS_PAGE_SIZE);
  const clampedFocusPage = focusPagination.clampPage(focusPage);
  if (clampedFocusPage !== focusPage) {
    // Keep state aligned with the rendered page. Rendering only through
    // getPage's clamp leaves a stale later page behind, which resurfaces when
    // the live queue shrinks and then grows again.
    setFocusPage(clampedFocusPage);
  }
  const visibleFocusItems = focusPagination.getPage(clampedFocusPage);

  const schedJobsPagination = usePagination(props.calendarJobs, SCHED_JOBS_PAGE_SIZE);
  const clampedSchedJobsPage = schedJobsPagination.clampPage(schedJobsPage);
  if (clampedSchedJobsPage !== schedJobsPage) {
    setSchedJobsPage(clampedSchedJobsPage);
  }
  const visibleSchedJobs = schedJobsPagination.getPage(clampedSchedJobsPage);

  const degradedCount = props.channelStatuses.filter(
    (item) => !item.healthy || item.lifecycle_state !== "running"
  ).length;

  // Empty breaker/scheduler arrays mean "healthy" only after the gateway has
  // actually answered; before that they are just not loaded yet.
  const statusKnown = props.jobsStatus !== null;
  const openBreakerCount =
    props.openBreakers.length + props.openPluginBreakers.length;
  const schedulerNowMs = props.jobsStatus
    ? Date.parse(props.jobsStatus.now_utc)
    : Number.NaN;

  const withBusy = (itemId: string, fn: () => Promise<void>) => {
    if (busyItemsRef.current.has(itemId)) {
      return;
    }
    setActionErrors((current) => {
      if (!current.has(itemId)) return current;
      const next = new Map(current);
      next.delete(itemId);
      return next;
    });
    busyItemsRef.current.add(itemId);
    setBusyItems(new Set(busyItemsRef.current));
    void fn()
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : String(error);
        setActionErrors((current) => new Map(current).set(itemId, message));
      })
      .finally(() => {
        busyItemsRef.current.delete(itemId);
        setBusyItems(new Set(busyItemsRef.current));
      });
  };

  const toggleExpand = (itemId: string) => {
    setExpandedItems((prev) => {
      const next = new Set(prev);
      if (next.has(itemId)) {
        next.delete(itemId);
      } else {
        next.add(itemId);
      }
      return next;
    });
  };

  return (
    <section className="mc-focus-page">
      <Tabs
        tabs={[
          { id: "queue", label: "Queue", count: props.focusItems.length },
          { id: "status", label: "System Status", count: degradedCount > 0 ? degradedCount : undefined },
          {
            id: "breakers",
            label: "Breakers",
            count: statusKnown && openBreakerCount > 0 ? openBreakerCount : undefined,
          },
          { id: "scheduler", label: "Scheduler" },
        ]}
        activeTab={subTab}
        onTabChange={(id) => setSubTab(id as FocusSubTab)}
      />

      {subTab === "queue" ? (
        <Surface
          title="Operator Focus Queue"
          subtitle={`${props.focusItems.length} open attention items`}
        >
          <div className="mc-focus-list">
            {visibleFocusItems.map((item) => {
              const approvalId = String(item.action_payload.approval_id ?? "").trim();
              const jobId = String(item.action_payload.job_id ?? "").trim();
              const payloadTaskId = String(item.action_payload.task_id ?? "").trim();
              const provider = String(item.action_payload.provider ?? "").trim();
              const isBusy = busyItems.has(item.item_id);
              const actionError = actionErrors.get(item.item_id);
              const isExpanded = expandedItems.has(item.item_id);
              const contextEntries = extractApprovalContext(item.action_payload);
              const hasContext = contextEntries.length > 0;
              const linkedTask = props.strategyReady
                ? payloadTaskId
                  ? props.taskById.get(payloadTaskId) ?? null
                  : approvalId
                    ? props.approvalTaskByApprovalId.get(approvalId) ?? null
                    : jobId
                      ? props.taskByJobId.get(jobId) ?? null
                      : null
                : null;
              const linkedTaskContext = linkedTask
                ? props.describeStrategyTask(linkedTask.task_id)
                : null;
              const linkedRunbook = props.runbookEnabled
                ? props.getRunbookForFocusItem(item)
                : null;
              return (
                <article key={item.item_id} className={clsx("mc-focus-item", item.severity)}>
                  <div className="mc-focus-head">
                    <Chip label={item.severity} tone={item.severity} />
                    <span>{item.category}</span>
                    <span title={formatDateTime(item.created_at)}>{formatRelative(item.created_at)}</span>
                  </div>
                  <h3>{item.title}</h3>
                  <p>{item.detail}</p>
                  {props.strategyReady ? (
                    <StrategyTaskContextPanel
                      compact
                      className="mc-focus-strategy-panel"
                      task={linkedTask}
                      context={linkedTaskContext}
                      onOpen={
                        linkedTask
                          ? () => props.onOpenStrategyTask(linkedTask.task_id)
                          : undefined
                      }
                      emptyMessage={null}
                      openLabel="Open task"
                    />
                  ) : null}
                  {props.runbookEnabled ? (
                    <RunbookLinkPanel
                      compact
                      className="mc-focus-runbook-panel"
                      summary={linkedRunbook}
                      emptyMessage={null}
                      onOpen={
                        linkedRunbook
                          ? () => props.onOpenRunbookForFocusItem(item)
                          : undefined
                      }
                    />
                  ) : null}
                  {hasContext ? (
                    <button
                      type="button"
                      className="mc-focus-details-toggle"
                      onClick={() => toggleExpand(item.item_id)}
                    >
                      {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      {isExpanded ? "Hide details" : "Show details"}
                    </button>
                  ) : null}
                  {isExpanded && hasContext ? (
                    <dl className="mc-focus-context">
                      {contextEntries.map(([label, value]) => (
                        <div key={label} className="mc-focus-context-row">
                          <dt>{label}</dt>
                          <dd>
                            {value.includes("\n") ? <pre>{value}</pre> : value}
                          </dd>
                        </div>
                      ))}
                    </dl>
                  ) : null}
                  <InlineActions>
                    {item.category === "approval" ? (
                      <>
                        <button
                          type="button"
                          disabled={!approvalId || isBusy}
                          aria-disabled={!approvalId || isBusy}
                          title={!approvalId ? "No approval ID linked" : undefined}
                          onClick={() =>
                            approvalId
                              ? withBusy(item.item_id, () => props.onResolveFocusApproval(approvalId, "approve"))
                              : undefined
                          }
                        >
                          {isBusy ? "Working..." : "Approve"}
                        </button>
                        <button
                          type="button"
                          className="danger"
                          disabled={!approvalId || isBusy}
                          aria-disabled={!approvalId || isBusy}
                          title={!approvalId ? "No approval ID linked" : undefined}
                          onClick={() =>
                            approvalId
                              ? withBusy(item.item_id, () => props.onResolveFocusApproval(approvalId, "deny"))
                              : undefined
                          }
                        >
                          {isBusy ? "Working..." : "Deny"}
                        </button>
                      </>
                    ) : null}
                    {item.category === "run_failure" ? (
                      <button
                        type="button"
                        disabled={!jobId || isBusy}
                        aria-disabled={!jobId || isBusy}
                        title={!jobId ? "No job ID linked" : undefined}
                        onClick={() =>
                          jobId ? withBusy(item.item_id, () => props.onRunCalendarJobNow(jobId)) : undefined
                        }
                      >
                        {isBusy ? "Working..." : "Retry Job"}
                      </button>
                    ) : null}
                    {item.category === "channel_health" ? (
                      <button
                        type="button"
                        disabled={!provider || isBusy}
                        aria-disabled={!provider || isBusy}
                        title={!provider ? "No provider linked" : undefined}
                        onClick={() =>
                          provider
                            ? withBusy(item.item_id, () => props.onReconnectFocusChannel(provider))
                            : undefined
                        }
                      >
                        {isBusy ? "Working..." : "Reconnect Channel"}
                      </button>
                    ) : null}
                    {linkedRunbook ? (
                      <button
                        type="button"
                        onClick={() => props.onOpenRunbookForFocusItem(item)}
                      >
                        Open Runbook
                      </button>
                    ) : null}
                  </InlineActions>
                  {actionError ? (
                    <p className="mc-focus-action-error" role="alert">
                      Action failed: {actionError}
                    </p>
                  ) : null}
                </article>
              );
            })}
            {visibleFocusItems.length === 0 ? (
              <div className="mc-empty-drawer">No focus items — all clear.</div>
            ) : null}
          </div>
          <Pagination currentPage={clampedFocusPage} totalPages={focusPagination.totalPages} onPageChange={setFocusPage} />
        </Surface>
      ) : null}

      {subTab === "status" ? (
        <Surface title="System Status" subtitle="Live queue and channel posture">
          <ul className="mc-stat-list">
            <li>
              <strong>Pending approvals</strong>
              <span>{props.approvalsCount}</span>
            </li>
            <li>
              <strong>Channel adapters</strong>
              <span>{props.channelStatuses.length}</span>
            </li>
            <li>
              <strong>Degraded channels</strong>
              <span>{degradedCount}</span>
            </li>
          </ul>
          <div className="mc-channel-grid">
            {props.channelStatuses.map((item) => {
              const reconnectKey = `status-channel:${item.provider}`;
              const reconnectBusy = busyItems.has(reconnectKey);
              const reconnectError = actionErrors.get(reconnectKey);
              return (
              <article key={item.provider} className="mc-channel-card">
                <div className="mc-channel-card-header">
                  <h3>{item.provider}</h3>
                  <Chip
                    label={item.healthy ? "healthy" : "degraded"}
                    tone={item.healthy ? "up" : "down"}
                  />
                </div>
                <p>{item.lifecycle_state}</p>
                <p>{item.last_error ?? item.detail ?? (item.healthy ? "all systems go" : "unhealthy")}</p>
                <button
                  type="button"
                  disabled={reconnectBusy}
                  onClick={() =>
                    withBusy(reconnectKey, () => props.onReconnectFocusChannel(item.provider))
                  }
                >
                  {reconnectBusy ? "Working..." : "Reconnect"}
                </button>
                {reconnectError ? (
                  <p className="mc-focus-action-error" role="alert">
                    Action failed: {reconnectError}
                  </p>
                ) : null}
              </article>
              );
            })}
            {props.channelStatuses.length === 0 ? (
              <div className="mc-empty-drawer">No channel adapters registered.</div>
            ) : null}
          </div>
        </Surface>
      ) : null}

      {subTab === "breakers" ? (
        <Surface
          title="Circuit breakers"
          subtitle={
            statusKnown
              ? `${props.openBreakers.length} core open · ${props.openPluginBreakers.length} plugin faulted`
              : "Waiting for gateway status"
          }
        >
          {/* The pin rides the shared room-row idiom; it owns no breaker,
              scheduler, queue, or channel state. */}
          <div className="mc-room-tabs-row mc-breakers-room-row">
            <p className="mc-breakers-note">
              Breakers open after repeated failures and close on their own
              after cooldown once calls succeed — there is no manual reset.
            </p>
            <PinRoomToOffice roomId="breakers" />
          </div>
          {!statusKnown && openBreakerCount === 0 ? (
            <EmptyState message="Breaker status hasn't loaded yet — waiting for the gateway." />
          ) : (
            <>
              <section className="mc-breaker-group" aria-label="Core breakers">
                <h3>Core breakers</h3>
                <ul className="mc-breaker-list">
                  {props.openBreakers.map((breaker) => (
                    <li key={`${breaker.scope}:${breaker.target_id}`} className="mc-breaker-row">
                      <div className="mc-breaker-head">
                        <strong>{breaker.scope}</strong>
                        <span className="mc-breaker-target">{breaker.target_id}</span>
                        <Chip label={breaker.state} tone="down" />
                      </div>
                      <p className="mc-breaker-facts">
                        {breaker.consecutive_failures} consecutive failures
                        {breaker.last_error_code ? ` · last error ${breaker.last_error_code}` : ""}
                      </p>
                      {breaker.cooldown_until !== null ? (
                        <p className="mc-breaker-facts" title={formatDateTime(breaker.cooldown_until)}>
                          Cooldown until {formatDateTime(breaker.cooldown_until)}
                        </p>
                      ) : null}
                      <p className="mc-breaker-facts">
                        Updated {formatRelative(breaker.updated_at)}
                      </p>
                    </li>
                  ))}
                </ul>
                {props.openBreakers.length === 0 ? (
                  <p className="mc-breaker-empty">
                    {statusKnown
                      ? "No open core breakers."
                      : "Core breaker status hasn't loaded yet."}
                  </p>
                ) : null}
              </section>
              <section className="mc-breaker-group" aria-label="Plugin runtimes">
                <h3>Plugin runtimes</h3>
                <ul className="mc-breaker-list">
                  {props.openPluginBreakers.map((plugin) => (
                    <li key={plugin.plugin_id} className="mc-breaker-row">
                      <div className="mc-breaker-head">
                        <strong>{plugin.plugin_id}</strong>
                        <Chip label={plugin.faulted ? "faulted" : "recovering"} tone="down" />
                      </div>
                      <p className="mc-breaker-facts">
                        {plugin.consecutive_failures} consecutive failures
                        {plugin.last_error_code ? ` · ${plugin.last_error_code}` : ""}
                      </p>
                      {plugin.last_error ? (
                        <p className="mc-breaker-facts">{plugin.last_error}</p>
                      ) : null}
                      {plugin.disabled_until_ms !== null ? (
                        <p className="mc-breaker-facts">
                          Disabled until {formatDateTime(plugin.disabled_until_ms)}
                        </p>
                      ) : null}
                      <p className="mc-breaker-facts">
                        {plugin.last_success_ms !== null
                          ? `Last success ${formatRelative(plugin.last_success_ms)}`
                          : "No recorded success"}
                        {plugin.last_invoked_ms !== null
                          ? ` · last invoked ${formatRelative(plugin.last_invoked_ms)}`
                          : ""}
                      </p>
                    </li>
                  ))}
                </ul>
                {props.openPluginBreakers.length === 0 ? (
                  <p className="mc-breaker-empty">
                    {statusKnown
                      ? "No faulted plugin runtimes."
                      : "Plugin runtime status hasn't loaded yet."}
                  </p>
                ) : null}
              </section>
            </>
          )}
        </Surface>
      ) : null}

      {subTab === "scheduler" ? (
        <Surface
          title="Scheduler"
          subtitle={
            props.jobsStatus
              ? `Status as of ${
                  Number.isFinite(schedulerNowMs)
                    ? formatDateTime(schedulerNowMs)
                    : props.jobsStatus.now_utc
                }`
              : "Waiting for gateway status"
          }
        >
          {props.jobsStatus ? (
            <>
              <div className="mc-sched-posture">
                <Chip
                  label={props.jobsStatus.scheduler_running ? "running" : "stopped"}
                  tone={props.jobsStatus.scheduler_running ? "up" : "down"}
                />
                <span>
                  {props.jobsStatus.scheduler_lock.enabled
                    ? `Lock held by ${props.jobsStatus.scheduler_lock.owner || "unknown owner"}`
                    : "Lock disabled"}
                  {props.jobsStatus.scheduler_lock.detail
                    ? ` — ${props.jobsStatus.scheduler_lock.detail}`
                    : ""}
                </span>
              </div>
              <ul className="mc-stat-list mc-sched-stats">
                <li>
                  <strong>Jobs total</strong>
                  <span>{props.jobsStatus.jobs_total}</span>
                </li>
                <li>
                  <strong>Enabled</strong>
                  <span>{props.jobsStatus.jobs_enabled}</span>
                </li>
                <li>
                  <strong>Due now</strong>
                  <span>{props.jobsStatus.jobs_due}</span>
                </li>
              </ul>
              <section className="mc-sched-stop-reasons" aria-label="Top stop reasons">
                <h3>Top stop reasons</h3>
                {props.jobsStatus.top_stop_reasons.length > 0 ? (
                  <ul>
                    {props.jobsStatus.top_stop_reasons.map((reason) => (
                      <li key={reason.code}>
                        <code>{reason.code}</code>
                        <span>×{reason.count}</span>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="mc-breaker-empty">No recorded stop reasons.</p>
                )}
              </section>
            </>
          ) : (
            <EmptyState message="Scheduler status hasn't loaded yet — waiting for the gateway." />
          )}
          <section className="mc-sched-jobs-section" aria-label="Scheduled jobs">
            <h3>Scheduled jobs</h3>
            <ul className="mc-sched-job-list">
              {visibleSchedJobs.map((job) => {
                const jobActionKey = `sched-job:${job.job_id}`;
                const jobBusy = busyItems.has(jobActionKey);
                const jobError = actionErrors.get(jobActionKey);
                return (
                  <li key={job.job_id} className="mc-sched-job">
                    <div className="mc-sched-job-head">
                      <strong>{job.name}</strong>
                      <Chip
                        label={job.enabled ? "enabled" : "paused"}
                        tone={job.enabled ? "up" : ""}
                      />
                    </div>
                    <p className="mc-sched-job-facts">
                      {describeSchedule(job)} · agent {job.agent_id}
                    </p>
                    <p className="mc-sched-job-facts">
                      {job.next_run_at !== null
                        ? `Next run ${formatDateTime(job.next_run_at)}`
                        : "No next run scheduled"}
                      {job.last_run_at !== null
                        ? ` · last ran ${formatRelative(job.last_run_at)}`
                        : ""}
                    </p>
                    {job.last_error ? (
                      <p className="mc-sched-job-facts mc-sched-job-error">
                        Last error: {job.last_error}
                      </p>
                    ) : null}
                    <InlineActions>
                      <button
                        type="button"
                        disabled={jobBusy}
                        onClick={() =>
                          withBusy(jobActionKey, () =>
                            props.onRunCalendarJobNow(job.job_id)
                          )
                        }
                      >
                        {jobBusy ? "Working..." : "Run now"}
                      </button>
                      <button
                        type="button"
                        disabled={jobBusy}
                        onClick={() =>
                          withBusy(jobActionKey, () =>
                            props.onToggleCalendarJob(job.job_id, !job.enabled)
                          )
                        }
                      >
                        {jobBusy ? "Working..." : job.enabled ? "Pause" : "Resume"}
                      </button>
                    </InlineActions>
                    {jobError ? (
                      <p className="mc-focus-action-error" role="alert">
                        Action failed: {jobError}
                      </p>
                    ) : null}
                  </li>
                );
              })}
            </ul>
            {props.calendarJobs.length === 0 ? (
              <p className="mc-breaker-empty">
                {statusKnown
                  ? "No scheduled jobs."
                  : "Scheduled jobs haven't loaded yet."}
              </p>
            ) : null}
            <Pagination
              currentPage={clampedSchedJobsPage}
              totalPages={schedJobsPagination.totalPages}
              onPageChange={setSchedJobsPage}
            />
          </section>
        </Surface>
      ) : null}
    </section>
  );
}
