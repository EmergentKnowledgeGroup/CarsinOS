import { useRef, useState } from "react";
import clsx from "clsx";
import { ChevronDown, ChevronRight } from "lucide-react";
import type {
  ChannelRuntimeAdapterStatusResponse,
  MissionControlFocusItem,
  RunbookSummaryItemResponse,
  TaskResponse,
} from "../../types";
import { Chip } from "../../ui/Chip";
import { InlineActions } from "../../ui/InlineActions";
import { Pagination } from "../../ui/Pagination";
import { Surface } from "../../ui/Surface";
import { Tabs } from "../../ui/Tabs";
import { usePagination } from "../../ui/usePagination";
import { formatDateTime, formatRelative } from "../../utils/datetime";
import { redactSecrets } from "../../lib/redaction";
import { RunbookLinkPanel } from "../runbook/RunbookLinkPanel";
import { StrategyTaskContextPanel } from "../strategy/StrategyTaskContextPanel";
import type { StrategyTaskContextSnapshot } from "../strategy/useStrategyController";

const FOCUS_PAGE_SIZE = 6;

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

/** Extract human-readable context fields from an approval's action_payload. */
function extractApprovalContext(payload: Record<string, unknown>): Array<[string, string]> {
  const entries: Array<[string, string]> = [];
  const fields: Array<[string, string]> = [
    ["tool_name", "Tool"],
    ["tool_input", "Arguments"],
    ["request_summary", "Summary"],
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
}

export function FocusPage(props: FocusPageProps) {
  const [subTab, setSubTab] = useState<"queue" | "status">("queue");
  const [focusPage, setFocusPage] = useState(1);
  const [expandedItems, setExpandedItems] = useState<Set<string>>(new Set());
  const [busyItems, setBusyItems] = useState<Set<string>>(new Set());
  const busyItemsRef = useRef<Set<string>>(new Set());
  const focusPagination = usePagination(props.focusItems, FOCUS_PAGE_SIZE);
  const visibleFocusItems = focusPagination.getPage(focusPage);

  const degradedCount = props.channelStatuses.filter(
    (item) => !item.healthy || item.lifecycle_state !== "running"
  ).length;

  const withBusy = (itemId: string, fn: () => Promise<void>) => {
    if (busyItemsRef.current.has(itemId)) {
      return;
    }
    busyItemsRef.current.add(itemId);
    setBusyItems(new Set(busyItemsRef.current));
    void fn()
      .catch(() => undefined)
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
        ]}
        activeTab={subTab}
        onTabChange={(id) => setSubTab(id as "queue" | "status")}
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
                </article>
              );
            })}
            {visibleFocusItems.length === 0 ? (
              <div className="mc-empty-drawer">No focus items — all clear.</div>
            ) : null}
          </div>
          <Pagination currentPage={focusPage} totalPages={focusPagination.totalPages} onPageChange={setFocusPage} />
        </Surface>
      ) : (
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
              </article>
              );
            })}
            {props.channelStatuses.length === 0 ? (
              <div className="mc-empty-drawer">No channel adapters registered.</div>
            ) : null}
          </div>
        </Surface>
      )}
    </section>
  );
}
