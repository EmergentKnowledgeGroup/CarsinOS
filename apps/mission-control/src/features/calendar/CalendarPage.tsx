import { useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { Bell, Brain, ClipboardCheck, Play, Pause, Plus, Zap, Clock, CalendarDays } from "lucide-react";
import type {
  Agent,
  CreateJobRequest,
  JobRunResponse,
  MissionControlCalendarJob,
  MissionControlCalendarWeekResponse,
  RunbookSummaryItemResponse,
  TaskResponse,
} from "../../types";
import { Chip } from "../../ui/Chip";
import { Tabs } from "../../ui/Tabs";
import { Pagination } from "../../ui/Pagination";
import { usePagination } from "../../ui/usePagination";
import { formatRelative } from "../../utils/datetime";
import { RunbookLinkPanel } from "../runbook/RunbookLinkPanel";
import { StrategyTaskContextPanel } from "../strategy/StrategyTaskContextPanel";
import type { StrategyTaskContextSnapshot } from "../strategy/useStrategyController";
import {
  buildExecAssHeartbeatJobRequest,
  EXECASS_HEARTBEAT_PRESETS,
  hasExecAssHeartbeatJob,
  resolveExecAssHeartbeatAgent,
  type ExecAssHeartbeatPresetKind,
} from "./execAssHeartbeatPresets";
import { summarizeExecAssWakeupAudit } from "./execAssWakeupAudit";

interface CalendarPageProps {
  calendarWeek: MissionControlCalendarWeekResponse | null;
  calendarAlwaysRunning: MissionControlCalendarJob[];
  calendarNextUp: MissionControlCalendarJob[];
  calendarJobs: MissionControlCalendarJob[];
  agents: Agent[];
  execAssAgentId: string | null;
  onRunCalendarJobNow: (jobId: string) => Promise<void>;
  onToggleCalendarJob: (jobId: string, enabled: boolean) => Promise<void>;
  onLoadCalendarJobHistory: (jobId: string) => Promise<JobRunResponse[]>;
  onCreateExecAssHeartbeatJob: (request: CreateJobRequest) => Promise<void>;
  strategyReady: boolean;
  taskByJobId: Map<string, TaskResponse>;
  describeStrategyTask: (taskId: string) => StrategyTaskContextSnapshot | null;
  onOpenStrategyTask: (taskId: string) => boolean;
  runbookEnabled: boolean;
  runbookByJobId: Map<string, RunbookSummaryItemResponse>;
  onOpenJobRunbook: (jobId: string) => boolean;
}

const TABS = [
  { id: "week", label: "Week View" },
  { id: "schedule", label: "Schedule" },
  { id: "active", label: "Active Jobs" },
];

const SCHEDULE_PAGE_SIZE = 6;
const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/* ── ExecAss Heartbeat Setup ─────────────────────────────────────────────── */

function ExecAssHeartbeatPanel({
  agents,
  execAssAgentId,
  jobs,
  onCreateJob,
}: {
  agents: Agent[];
  execAssAgentId: string | null;
  jobs: MissionControlCalendarJob[];
  onCreateJob: (request: CreateJobRequest) => Promise<void>;
}) {
  const [checkInMinutes, setCheckInMinutes] = useState(60);
  const [dailyLearningHours, setDailyLearningHours] = useState(24);
  const [jobWatchMinutes, setJobWatchMinutes] = useState(15);
  const [busyKind, setBusyKind] = useState<ExecAssHeartbeatPresetKind | null>(null);
  const agent = useMemo(
    () => resolveExecAssHeartbeatAgent(agents, execAssAgentId),
    [agents, execAssAgentId]
  );

  const createPreset = async (
    kind: ExecAssHeartbeatPresetKind,
    intervalMinutes: number
  ) => {
    if (!agent || busyKind !== null) {
      return;
    }
    setBusyKind(kind);
    try {
      await onCreateJob(
        buildExecAssHeartbeatJobRequest({
          kind,
          agent,
          intervalMinutes,
          existingJobCount: jobs.length,
        })
      );
    } finally {
      setBusyKind(null);
    }
  };

  const renderPreset = (
    kind: ExecAssHeartbeatPresetKind,
    icon: ReactNode,
    intervalValue: number,
    setIntervalValue: (value: number) => void,
    intervalUnit: "minutes" | "hours"
  ) => {
    const preset = EXECASS_HEARTBEAT_PRESETS.find((item) => item.key === kind);
    if (!preset) {
      return null;
    }
    const alreadyScheduled = hasExecAssHeartbeatJob(jobs, kind);
    const busy = busyKind === kind;
    const minutes = intervalUnit === "hours" ? intervalValue * 60 : intervalValue;

    return (
      <div className="mc-cal-heartbeat-card" key={kind}>
        <div className="mc-cal-heartbeat-icon">{icon}</div>
        <div className="mc-cal-heartbeat-copy">
          <strong>{preset.label}</strong>
          <span>{preset.description}</span>
          <label>
            Every
            <input
              type="number"
              min={intervalUnit === "hours" ? 1 : 5}
              max={intervalUnit === "hours" ? 168 : 1440}
              value={intervalValue}
              onChange={(event) =>
                setIntervalValue(Number.parseInt(event.target.value, 10) || 1)
              }
            />
            {intervalUnit}
          </label>
        </div>
        <button
          type="button"
          className="mc-primary-btn mc-cal-heartbeat-action"
          disabled={!agent || alreadyScheduled || busyKind !== null}
          onClick={() => void createPreset(kind, minutes)}
          title={
            alreadyScheduled
              ? "This ExecAss heartbeat is already scheduled"
              : `Create ${preset.label} heartbeat`
          }
        >
          <Plus size={14} />
          {alreadyScheduled ? "Scheduled" : busy ? "Creating" : "Create"}
        </button>
      </div>
    );
  };

  return (
    <section className="mc-cal-heartbeat-panel" aria-label="ExecAss heartbeat setup">
      <div className="mc-cal-heartbeat-header">
        <div>
          <p className="mc-cal-heartbeat-eyebrow">ExecAss wakeups</p>
          <h2>Heartbeat setup</h2>
        </div>
        <span className="mc-cal-heartbeat-agent">
          {agent
            ? `${agent.name} / ${agent.model_provider}:${agent.model_id}`
            : "No assistant agent available"}
        </span>
      </div>
      <div className="mc-cal-heartbeat-grid">
        {renderPreset("check_in", <Bell size={16} />, checkInMinutes, setCheckInMinutes, "minutes")}
        {renderPreset(
          "daily_learning",
          <Brain size={16} />,
          dailyLearningHours,
          setDailyLearningHours,
          "hours"
        )}
        {renderPreset(
          "job_watch",
          <ClipboardCheck size={16} />,
          jobWatchMinutes,
          setJobWatchMinutes,
          "minutes"
        )}
      </div>
    </section>
  );
}

function ExecAssWakeupAuditPanel({
  jobId,
  onLoadJobHistory,
}: {
  jobId: string;
  onLoadJobHistory: (jobId: string) => Promise<JobRunResponse[]>;
}) {
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState<ReturnType<typeof summarizeExecAssWakeupAudit>>(null);

  const loadAudit = () => {
    if (expanded && !busy) {
      setExpanded(false);
      return;
    }
    setBusy(true);
    setError(null);
    void onLoadJobHistory(jobId)
      .then((runs) => {
        setSummary(summarizeExecAssWakeupAudit(runs));
        setExpanded(true);
      })
      .catch((auditError: unknown) => {
        setError(String(auditError));
        setExpanded(true);
      })
      .finally(() => {
        setBusy(false);
      });
  };

  return (
    <div className="mc-cal-audit">
      <div className="mc-cal-audit-head">
        <span>Wakeup audit</span>
        <button
          type="button"
          className="mc-topbar-icon-btn"
          disabled={busy}
          onClick={loadAudit}
          title={expanded ? "Hide wakeup audit" : "Load wakeup audit"}
        >
          {busy ? <span className="mc-btn-busy">Loading</span> : <ClipboardCheck size={13} />}
        </button>
      </div>
      {expanded ? (
        <div className="mc-cal-audit-body">
          {error ? <p className="mc-cal-audit-error">{error}</p> : null}
          {!error && !summary && !busy ? (
            <p className="mc-cal-audit-muted">No wakeup audit runs yet.</p>
          ) : null}
          {summary ? (
            <>
              <div className="mc-cal-audit-chip-row">
                <Chip
                  label={summary.status}
                  tone={summary.attentionCount > 0 ? "high" : "up"}
                />
                <Chip
                  label={summary.llmInvoked ? "LLM used" : "No LLM"}
                  tone={summary.llmInvoked ? "high" : "up"}
                />
                <Chip label={`${summary.checkedCount} checked`} tone="medium" />
                <Chip label={`${summary.attentionCount} found`} tone={summary.attentionCount > 0 ? "high" : "up"} />
              </div>
              {summary.attentionItems.length > 0 ? (
                <ul className="mc-cal-audit-list">
                  {summary.attentionItems.slice(0, 4).map((item) => (
                    <li key={`${item.category}:${item.kind}:${item.summary}`}>
                      <strong>{item.category}</strong>
                      <span>{item.summary}</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="mc-cal-audit-muted">No attention items found.</p>
              )}
              <div className="mc-cal-audit-checked">
                {summary.checked.map((item) => (
                  <span
                    key={item.category}
                    className={
                      item.status === "attention"
                        ? "mc-cal-audit-category mc-cal-audit-category-hot"
                        : "mc-cal-audit-category"
                    }
                    title={item.summary}
                  >
                    {item.category}
                  </span>
                ))}
              </div>
              <details className="mc-cal-audit-raw">
                <summary>Raw packet</summary>
                <pre>{summary.rawOutput}</pre>
              </details>
            </>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** Convert ms interval to human-readable duration */
function formatInterval(seconds: number | null): string {
  if (seconds === null) return "";
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h`;
  return `${Math.round(seconds / 86400)}d`;
}

/** Stable color for a job based on its name hash */
function jobColor(name: string): string {
  const colors = [
    "var(--accent)",
    "var(--info)",
    "var(--ok)",
    "var(--warn)",
    "var(--danger)",
    "var(--accent-hover)",
    "var(--info-border, var(--info))",
    "var(--ok-border, var(--ok))",
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = (hash * 31 + name.charCodeAt(i)) | 0;
  }
  return colors[Math.abs(hash) % colors.length];
}

/* ── Week Grid ──────────────────────────────────────────────────────────── */

function WeekGrid({
  calendarWeek,
  onRunNow,
  strategyReady,
  taskByJobId,
  runbookEnabled,
  runbookByJobId,
  onOpenJobRunbook,
}: {
  calendarWeek: MissionControlCalendarWeekResponse | null;
  onRunNow: (jobId: string) => Promise<void>;
  strategyReady: boolean;
  taskByJobId: Map<string, TaskResponse>;
  runbookEnabled: boolean;
  runbookByJobId: Map<string, RunbookSummaryItemResponse>;
  onOpenJobRunbook: (jobId: string) => boolean;
}) {
  const { daySlots, alwaysRunning, nextUp } = useMemo(() => {
    if (!calendarWeek)
      return { daySlots: [] as MissionControlCalendarJob[][], alwaysRunning: [] as MissionControlCalendarJob[], nextUp: [] as MissionControlCalendarJob[] };

    const weekStart = calendarWeek.week_start_ms;
    const slots: MissionControlCalendarJob[][] = Array.from({ length: 7 }, () => []);

    // Place each job with a next_run_at into the correct day slot
    for (const job of calendarWeek.jobs) {
      if (job.next_run_at !== null) {
        const dayIndex = Math.floor((job.next_run_at - weekStart) / 86400000);
        if (dayIndex >= 0 && dayIndex < 7) {
          slots[dayIndex].push(job);
        }
      }
      // Jobs with interval that run every day get placed in each day
      if (
        job.interval_seconds !== null &&
        job.interval_seconds > 0 &&
        job.interval_seconds <= 86400 &&
        job.lane === "always_running"
      ) {
        for (let d = 0; d < 7; d++) {
          if (!slots[d].some((j) => j.job_id === job.job_id)) {
            slots[d].push(job);
          }
        }
      }
    }

    return {
      daySlots: slots,
      alwaysRunning: calendarWeek.always_running,
      nextUp: calendarWeek.next_up,
    };
  }, [calendarWeek]);

  if (!calendarWeek) {
    return (
      <div className="mc-team-empty">
        <CalendarDays size={40} />
        <p>No calendar data available</p>
        <p className="mc-team-empty-sub">Connect to a gateway to see your schedule.</p>
      </div>
    );
  }

  const weekStart = new Date(calendarWeek.week_start_ms);

  return (
    <div className="mc-cal-week">
      {/* Always Running strip */}
      {alwaysRunning.length > 0 ? (
        <div className="mc-cal-always">
          <div className="mc-cal-always-label">
            <Zap size={12} />
            Always Running
          </div>
          <div className="mc-cal-always-items">
            {alwaysRunning.map((job) => (
              <button
                key={job.job_id}
                type="button"
                className="mc-cal-always-chip"
                onClick={() => void onRunNow(job.job_id)}
                title={`Run ${job.name} now`}
                style={{ "--job-color": jobColor(job.name) } as CSSProperties}
              >
                <span className="mc-cal-job-dot" />
                {job.name}
                {job.interval_seconds ? (
                  <span className="mc-cal-interval">
                    every {formatInterval(job.interval_seconds)}
                  </span>
                ) : null}
                {strategyReady && taskByJobId.has(job.job_id) ? (
                  <span className="mc-cal-linked-badge">Task linked</span>
                ) : null}
                {runbookEnabled && runbookByJobId.has(job.job_id) ? (
                  <span className="mc-cal-linked-badge">
                    Runbook
                  </span>
                ) : null}
              </button>
            ))}
          </div>
        </div>
      ) : null}

      {/* Day columns */}
      <div className="mc-cal-grid">
        {DAY_NAMES.map((dayName, i) => {
          const dayDate = new Date(weekStart);
          dayDate.setDate(dayDate.getDate() + i);
          const isToday = new Date().toDateString() === dayDate.toDateString();
          const dayJobs = daySlots[i] ?? [];

          return (
            <div
              key={dayName}
              className={`mc-cal-day ${isToday ? "mc-cal-day-today" : ""}`}
            >
              <div className="mc-cal-day-header">
                <span className="mc-cal-day-name">{dayName}</span>
                <span className="mc-cal-day-date">{dayDate.getDate()}</span>
              </div>
              <div className="mc-cal-day-body">
                {dayJobs.map((job) => (
                <button
                  key={job.job_id}
                    type="button"
                    className="mc-cal-job-block"
                    onClick={() => void onRunNow(job.job_id)}
                    title={`${job.name} — ${job.agent_id}\nClick to run now`}
                    style={{ "--job-color": jobColor(job.name) } as CSSProperties}
                >
                  <span className="mc-cal-job-dot" />
                  <span className="mc-cal-job-name">{job.name}</span>
                  {strategyReady && taskByJobId.has(job.job_id) ? (
                    <span className="mc-cal-job-link-badge">Task</span>
                  ) : null}
                  {runbookEnabled && runbookByJobId.has(job.job_id) ? (
                    <span className="mc-cal-job-link-badge">Runbook</span>
                  ) : null}
                </button>
              ))}
                {dayJobs.length === 0 ? (
                  <span className="mc-cal-day-empty">—</span>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>

      {/* Next Up strip */}
      {nextUp.length > 0 ? (
        <div className="mc-cal-next-up">
          <div className="mc-cal-always-label">
            <Clock size={12} />
            Next Up
          </div>
          <div className="mc-cal-next-items">
            {nextUp.slice(0, 5).map((job) => (
              <div key={job.job_id} className="mc-cal-next-item">
                <span
                  className="mc-cal-job-dot"
                  style={{ "--job-color": jobColor(job.name) } as CSSProperties}
                />
                <span className="mc-cal-next-name">{job.name}</span>
                <span className="mc-cal-next-time">
                  {formatRelative(job.next_run_at)}
                </span>
                {strategyReady && taskByJobId.has(job.job_id) ? (
                  <span className="mc-cal-next-badge">Task linked</span>
                ) : null}
                {runbookEnabled && runbookByJobId.has(job.job_id) ? (
                  <button
                    type="button"
                    className="mc-cal-next-badge"
                    onClick={() => onOpenJobRunbook(job.job_id)}
                  >
                    Runbook
                  </button>
                ) : null}
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* ── Schedule Table ─────────────────────────────────────────────────────── */

function ScheduleTable({
  jobs,
  onRunNow,
  onToggle,
  onLoadJobHistory,
  strategyReady,
  taskByJobId,
  describeStrategyTask,
  onOpenStrategyTask,
  runbookEnabled,
  runbookByJobId,
  onOpenJobRunbook,
}: {
  jobs: MissionControlCalendarJob[];
  onRunNow: (jobId: string) => Promise<void>;
  onToggle: (jobId: string, enabled: boolean) => Promise<void>;
  onLoadJobHistory: (jobId: string) => Promise<JobRunResponse[]>;
  strategyReady: boolean;
  taskByJobId: Map<string, TaskResponse>;
  describeStrategyTask: (taskId: string) => StrategyTaskContextSnapshot | null;
  onOpenStrategyTask: (taskId: string) => boolean;
  runbookEnabled: boolean;
  runbookByJobId: Map<string, RunbookSummaryItemResponse>;
  onOpenJobRunbook: (jobId: string) => boolean;
}) {
  const [page, setPage] = useState(1);
  const [busyJobActions, setBusyJobActions] = useState<Set<string>>(new Set());
  const busyJobActionsRef = useRef<Set<string>>(new Set());
  const { totalPages, getPage } = usePagination(jobs, SCHEDULE_PAGE_SIZE);
  const visible = getPage(page);

  const runBusyJobAction = (key: string, fn: () => Promise<void>) => {
    if (busyJobActionsRef.current.has(key)) {
      return;
    }
    busyJobActionsRef.current.add(key);
    setBusyJobActions(new Set(busyJobActionsRef.current));
    void fn()
      .catch((error: unknown) => {
        console.error("calendar job action failed", { key, error });
      })
      .finally(() => {
        busyJobActionsRef.current.delete(key);
        setBusyJobActions(new Set(busyJobActionsRef.current));
      });
  };

  return (
    <div>
      <div className="mc-table-wrap">
        <table className="mc-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Schedule</th>
              <th>Next Run</th>
              <th>Status</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {visible.map((job) => {
              const linkedTask = strategyReady ? taskByJobId.get(job.job_id) ?? null : null;
              const runbookSummary = runbookEnabled
                ? runbookByJobId.get(job.job_id) ?? null
                : null;
              const linkedTaskContext = linkedTask
                ? describeStrategyTask(linkedTask.task_id)
                : null;
              return (
                <tr key={job.job_id}>
                  <td>
                    <strong>{job.name}</strong>
                    <p className="mc-table-sub">{job.agent_id}</p>
                    {strategyReady ? (
                      <StrategyTaskContextPanel
                        compact
                        className="mc-cal-strategy-panel"
                        task={linkedTask}
                        context={linkedTaskContext}
                        onOpen={
                          linkedTask
                            ? () => onOpenStrategyTask(linkedTask.task_id)
                            : undefined
                        }
                        emptyMessage={null}
                        openLabel="Open task"
                      />
                    ) : null}
                    {runbookEnabled ? (
                      <RunbookLinkPanel
                        compact
                        className="mc-cal-runbook-panel"
                        summary={runbookSummary}
                        emptyMessage={null}
                        onOpen={
                          runbookSummary
                            ? () => onOpenJobRunbook(job.job_id)
                            : undefined
                        }
                      />
                    ) : null}
                    <ExecAssWakeupAuditPanel
                      jobId={job.job_id}
                      onLoadJobHistory={onLoadJobHistory}
                    />
                  </td>
                <td className="mc-mono">
                  {job.schedule_kind}
                  {job.interval_seconds !== null
                    ? ` / ${formatInterval(job.interval_seconds)}`
                    : ""}
                  {job.cron_expr ? ` / ${job.cron_expr}` : ""}
                </td>
                <td className="mc-mono">{formatRelative(job.next_run_at)}</td>
                <td>
                  <Chip
                    label={job.enabled ? "enabled" : "paused"}
                    tone={job.enabled ? "up" : "down"}
                  />
                </td>
                <td>
                  <div className="mc-cal-actions">
                    {(() => {
                      const runKey = `run:${job.job_id}`;
                      const toggleKey = `toggle:${job.job_id}`;
                      const runBusy = busyJobActions.has(runKey);
                      const toggleBusy = busyJobActions.has(toggleKey);
                      return (
                        <>
                          <button
                            type="button"
                            className="mc-topbar-icon-btn"
                            disabled={runBusy}
                            onClick={() =>
                              runBusyJobAction(runKey, () => onRunNow(job.job_id))
                            }
                            title="Run now"
                          >
                            {runBusy ? <span className="mc-btn-busy">Running\u2026</span> : <Play size={13} />}
                          </button>
                          <button
                            type="button"
                            className="mc-topbar-icon-btn"
                            disabled={toggleBusy}
                            onClick={() =>
                              runBusyJobAction(toggleKey, () => onToggle(job.job_id, !job.enabled))
                            }
                            title={job.enabled ? "Pause" : "Resume"}
                          >
                            {toggleBusy ? <span className="mc-btn-busy">Working\u2026</span> : <Pause size={13} />}
                          </button>
                        </>
                      );
                    })()}
                  </div>
                </td>
              </tr>
              );
            })}
            {jobs.length === 0 ? (
              <tr>
                <td colSpan={5} className="mc-table-empty">
                  No scheduled jobs.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
      <Pagination currentPage={page} totalPages={totalPages} onPageChange={setPage} />
    </div>
  );
}

/* ── Active Jobs ────────────────────────────────────────────────────────── */

function ActiveJobsList({
  alwaysRunning,
  nextUp,
  onRunNow,
  onToggle,
  onLoadJobHistory,
  strategyReady,
  taskByJobId,
  describeStrategyTask,
  onOpenStrategyTask,
  runbookEnabled,
  runbookByJobId,
  onOpenJobRunbook,
}: {
  alwaysRunning: MissionControlCalendarJob[];
  nextUp: MissionControlCalendarJob[];
  onRunNow: (jobId: string) => Promise<void>;
  onToggle: (jobId: string, enabled: boolean) => Promise<void>;
  onLoadJobHistory: (jobId: string) => Promise<JobRunResponse[]>;
  strategyReady: boolean;
  taskByJobId: Map<string, TaskResponse>;
  describeStrategyTask: (taskId: string) => StrategyTaskContextSnapshot | null;
  onOpenStrategyTask: (taskId: string) => boolean;
  runbookEnabled: boolean;
  runbookByJobId: Map<string, RunbookSummaryItemResponse>;
  onOpenJobRunbook: (jobId: string) => boolean;
}) {
  const [busyJobActions, setBusyJobActions] = useState<Set<string>>(new Set());
  const busyJobActionsRef = useRef<Set<string>>(new Set());

  const runBusyJobAction = (key: string, fn: () => Promise<void>) => {
    if (busyJobActionsRef.current.has(key)) {
      return;
    }
    busyJobActionsRef.current.add(key);
    setBusyJobActions(new Set(busyJobActionsRef.current));
    void fn()
      .catch((error: unknown) => {
        console.error("active job action failed", { key, error });
      })
      .finally(() => {
        busyJobActionsRef.current.delete(key);
        setBusyJobActions(new Set(busyJobActionsRef.current));
      });
  };

  return (
    <div className="mc-cal-active">
      <div className="mc-cal-active-section">
        <h3>
          <Zap size={14} />
          Always Running
        </h3>
        {alwaysRunning.length === 0 ? (
          <p className="mc-cal-active-empty">No always-running jobs.</p>
        ) : (
          <div className="mc-cal-active-list">
            {alwaysRunning.map((job) => (
              (() => {
                const linkedTask = strategyReady ? taskByJobId.get(job.job_id) ?? null : null;
                const runbookSummary = runbookEnabled
                  ? runbookByJobId.get(job.job_id) ?? null
                  : null;
                const linkedTaskContext = linkedTask
                  ? describeStrategyTask(linkedTask.task_id)
                  : null;
                return (
                  <div key={job.job_id} className="mc-cal-active-item">
                    <span
                      className="mc-cal-job-dot"
                      style={{ "--job-color": jobColor(job.name) } as CSSProperties}
                    />
                    <div className="mc-cal-active-info">
                      <strong>{job.name}</strong>
                      <span className="mc-mono">{job.agent_id}</span>
                      {strategyReady ? (
                        <StrategyTaskContextPanel
                          compact
                          className="mc-cal-strategy-panel"
                          task={linkedTask}
                          context={linkedTaskContext}
                          onOpen={
                            linkedTask
                              ? () => onOpenStrategyTask(linkedTask.task_id)
                              : undefined
                          }
                          emptyMessage={null}
                          openLabel="Open task"
                        />
                      ) : null}
                      {runbookEnabled ? (
                        <RunbookLinkPanel
                          compact
                          className="mc-cal-runbook-panel"
                          summary={runbookSummary}
                          emptyMessage={null}
                          onOpen={
                            runbookSummary
                              ? () => onOpenJobRunbook(job.job_id)
                              : undefined
                          }
                        />
                      ) : null}
                      <ExecAssWakeupAuditPanel
                        jobId={job.job_id}
                        onLoadJobHistory={onLoadJobHistory}
                      />
                    </div>
                    <span className="mc-mono mc-cal-active-interval">
                      {formatInterval(job.interval_seconds)}
                    </span>
                    <div className="mc-cal-actions">
                      {(() => {
                        const runKey = `active:run:${job.job_id}`;
                        const toggleKey = `active:toggle:${job.job_id}`;
                        const runBusy = busyJobActions.has(runKey);
                        const toggleBusy = busyJobActions.has(toggleKey);
                        return (
                          <>
                            <button
                              type="button"
                              className="mc-topbar-icon-btn"
                              disabled={runBusy}
                              onClick={() =>
                                runBusyJobAction(runKey, () => onRunNow(job.job_id))
                              }
                              title="Run now"
                            >
                              <Play size={13} />
                            </button>
                            <button
                              type="button"
                              className="mc-topbar-icon-btn"
                              disabled={toggleBusy}
                              onClick={() =>
                                runBusyJobAction(toggleKey, () => onToggle(job.job_id, !job.enabled))
                              }
                              title={job.enabled ? "Pause" : "Resume"}
                            >
                              <Pause size={13} />
                            </button>
                          </>
                        );
                      })()}
                    </div>
                  </div>
                );
              })()
            ))}
          </div>
        )}
      </div>

      <div className="mc-cal-active-section">
        <h3>
          <Clock size={14} />
          Next Up
        </h3>
        {nextUp.length === 0 ? (
          <p className="mc-cal-active-empty">No upcoming jobs.</p>
        ) : (
          <div className="mc-cal-active-list">
            {nextUp.map((job) => (
              (() => {
                const linkedTask = strategyReady ? taskByJobId.get(job.job_id) ?? null : null;
                const runbookSummary = runbookEnabled
                  ? runbookByJobId.get(job.job_id) ?? null
                  : null;
                const linkedTaskContext = linkedTask
                  ? describeStrategyTask(linkedTask.task_id)
                  : null;
                return (
                  <div key={job.job_id} className="mc-cal-active-item">
                    <span
                      className="mc-cal-job-dot"
                      style={{ "--job-color": jobColor(job.name) } as CSSProperties}
                    />
                    <div className="mc-cal-active-info">
                      <strong>{job.name}</strong>
                      <span className="mc-mono">{job.agent_id}</span>
                      {strategyReady ? (
                        <StrategyTaskContextPanel
                          compact
                          className="mc-cal-strategy-panel"
                          task={linkedTask}
                          context={linkedTaskContext}
                          onOpen={
                            linkedTask
                              ? () => onOpenStrategyTask(linkedTask.task_id)
                              : undefined
                          }
                          emptyMessage={null}
                          openLabel="Open task"
                        />
                      ) : null}
                      {runbookEnabled ? (
                        <RunbookLinkPanel
                          compact
                          className="mc-cal-runbook-panel"
                          summary={runbookSummary}
                          emptyMessage={null}
                          onOpen={
                            runbookSummary
                              ? () => onOpenJobRunbook(job.job_id)
                              : undefined
                          }
                        />
                      ) : null}
                      <ExecAssWakeupAuditPanel
                        jobId={job.job_id}
                        onLoadJobHistory={onLoadJobHistory}
                      />
                    </div>
                    <span className="mc-mono mc-cal-active-interval">
                      {formatRelative(job.next_run_at)}
                    </span>
                    {(() => {
                      const runKey = `next:run:${job.job_id}`;
                      const runBusy = busyJobActions.has(runKey);
                      return (
                        <button
                          type="button"
                          className="mc-topbar-icon-btn"
                          disabled={runBusy}
                          onClick={() =>
                            runBusyJobAction(runKey, () => onRunNow(job.job_id))
                          }
                          title="Run now"
                        >
                          <Play size={13} />
                        </button>
                      );
                    })()}
                  </div>
                );
              })()
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/* ── Main Calendar Page ─────────────────────────────────────────────────── */

export function CalendarPage(props: CalendarPageProps) {
  const [activeTab, setActiveTab] = useState("week");

  const tabsWithCounts = useMemo(
    () =>
      TABS.map((tab) => {
        if (tab.id === "schedule") return { ...tab, count: props.calendarJobs.length };
        if (tab.id === "active")
          return {
            ...tab,
            count: props.calendarAlwaysRunning.length + props.calendarNextUp.length,
          };
        return tab;
      }),
    [props.calendarJobs.length, props.calendarAlwaysRunning.length, props.calendarNextUp.length]
  );

  return (
    <div className="mc-calendar-page">
      <ExecAssHeartbeatPanel
        agents={props.agents}
        execAssAgentId={props.execAssAgentId}
        jobs={props.calendarJobs}
        onCreateJob={props.onCreateExecAssHeartbeatJob}
      />

      <Tabs tabs={tabsWithCounts} activeTab={activeTab} onTabChange={setActiveTab} />

      {activeTab === "week" ? (
        <WeekGrid
          calendarWeek={props.calendarWeek}
          onRunNow={props.onRunCalendarJobNow}
          strategyReady={props.strategyReady}
          taskByJobId={props.taskByJobId}
          runbookEnabled={props.runbookEnabled}
          runbookByJobId={props.runbookByJobId}
          onOpenJobRunbook={props.onOpenJobRunbook}
        />
      ) : null}

      {activeTab === "schedule" ? (
        <ScheduleTable
          jobs={props.calendarJobs}
          onRunNow={props.onRunCalendarJobNow}
          onToggle={props.onToggleCalendarJob}
          onLoadJobHistory={props.onLoadCalendarJobHistory}
          strategyReady={props.strategyReady}
          taskByJobId={props.taskByJobId}
          describeStrategyTask={props.describeStrategyTask}
          onOpenStrategyTask={props.onOpenStrategyTask}
          runbookEnabled={props.runbookEnabled}
          runbookByJobId={props.runbookByJobId}
          onOpenJobRunbook={props.onOpenJobRunbook}
        />
      ) : null}

      {activeTab === "active" ? (
        <ActiveJobsList
          alwaysRunning={props.calendarAlwaysRunning}
          nextUp={props.calendarNextUp}
          onRunNow={props.onRunCalendarJobNow}
          onToggle={props.onToggleCalendarJob}
          onLoadJobHistory={props.onLoadCalendarJobHistory}
          strategyReady={props.strategyReady}
          taskByJobId={props.taskByJobId}
          describeStrategyTask={props.describeStrategyTask}
          onOpenStrategyTask={props.onOpenStrategyTask}
          runbookEnabled={props.runbookEnabled}
          runbookByJobId={props.runbookByJobId}
          onOpenJobRunbook={props.onOpenJobRunbook}
        />
      ) : null}
    </div>
  );
}
