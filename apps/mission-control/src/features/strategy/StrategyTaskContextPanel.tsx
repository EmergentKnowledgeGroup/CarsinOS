import clsx from "clsx";
import { Link2 } from "lucide-react";
import { Chip } from "../../ui/Chip";
import type { TaskResponse } from "../../types";
import type { StrategyTaskContextSnapshot } from "./useStrategyController";

interface StrategyTaskContextPanelProps {
  task: TaskResponse | null;
  context: StrategyTaskContextSnapshot | null;
  onOpen?: () => void;
  openLabel?: string;
  emptyMessage?: string | null;
  compact?: boolean;
  className?: string;
}

function toneForTaskStatus(status: string): string {
  switch (status) {
    case "done":
      return "up";
    case "blocked":
    case "archived":
      return "down";
    case "in_progress":
      return "checking";
    default:
      return "";
  }
}

function toneForPriority(priority: string): string {
  switch (priority) {
    case "critical":
    case "high":
      return "warning";
    default:
      return "";
  }
}

export function StrategyTaskContextPanel({
  task,
  context,
  onOpen,
  openLabel = "Open in Strategy",
  emptyMessage = null,
  compact = false,
  className,
}: StrategyTaskContextPanelProps) {
  if (!task) {
    if (!emptyMessage) {
      return null;
    }
    return (
      <div
        className={clsx(
          "mc-strategy-runtime-link",
          "is-empty",
          compact && "is-compact",
          className
        )}
      >
        <div className="mc-strategy-runtime-link-head">
          <span className="mc-strategy-runtime-link-label">Strategy link</span>
          <Chip label="unlinked" />
        </div>
        <p>{emptyMessage}</p>
      </div>
    );
  }

  const managerLabel = context?.managerChain.length
    ? context.managerChain.map((agent) => agent.name).join(" -> ")
    : "";

  return (
    <div
      className={clsx(
        "mc-strategy-runtime-link",
        compact && "is-compact",
        className
      )}
    >
      <div className="mc-strategy-runtime-link-head">
        <div className="mc-strategy-runtime-link-title">
          <span className="mc-strategy-runtime-link-label">Strategy link</span>
          <strong>{task.title}</strong>
        </div>
        <Chip label={task.status.replaceAll("_", " ")} tone={toneForTaskStatus(task.status)} />
      </div>
      {!compact && task.detail.trim() ? (
        <p className="mc-strategy-runtime-link-detail">{task.detail}</p>
      ) : null}
      <div className="mc-strategy-runtime-chip-row">
        {context?.goal ? <Chip label={`Goal ${context.goal.title}`} tone="checking" /> : null}
        {context?.project ? (
          <Chip label={`Project ${context.project.name}`} tone="checking" />
        ) : null}
        <Chip label={`Priority ${task.priority}`} tone={toneForPriority(task.priority)} />
        <Chip
          label={`Owner ${context?.owner?.name ?? "Unassigned"}`}
          tone={context?.owner ? "up" : ""}
        />
      </div>
      {managerLabel ? (
        <p className="mc-strategy-runtime-link-meta">Manager chain: {managerLabel}</p>
      ) : null}
      {task.blocked_reason ? (
        <p className="mc-strategy-runtime-link-meta">Blocked: {task.blocked_reason}</p>
      ) : null}
      {onOpen ? (
        <button
          type="button"
          className="ghost mc-strategy-runtime-link-open"
          onClick={onOpen}
        >
          <Link2 size={14} />
          {openLabel}
        </button>
      ) : null}
    </div>
  );
}
