import clsx from "clsx";
import { ArrowRight, Workflow } from "lucide-react";
import { Chip } from "../../ui/Chip";
import type { RunbookSummaryItemResponse } from "../../types";
import {
  getRunbookCurrentStepLabel,
  getRunbookStatusTone,
} from "./runbookSummaryUtils";

interface RunbookLinkPanelProps {
  summary: RunbookSummaryItemResponse | null;
  onOpen?: (() => void) | null;
  openLabel?: string;
  emptyMessage?: string | null;
  compact?: boolean;
  className?: string;
}

export function RunbookLinkPanel({
  summary,
  onOpen,
  openLabel = "Open in Runbook",
  emptyMessage = null,
  compact = false,
  className,
}: RunbookLinkPanelProps) {
  if (!summary) {
    if (!emptyMessage) {
      return null;
    }
    return (
      <div className={clsx("mc-runbook-link-card", compact && "is-compact", "is-empty", className)}>
        <div className="mc-runbook-link-head">
          <div className="mc-runbook-link-title">
            <Workflow size={14} />
            <span>Runbook</span>
          </div>
          <Chip label="unavailable" />
        </div>
        <p>{emptyMessage}</p>
      </div>
    );
  }

  const currentStepLabel = getRunbookCurrentStepLabel(summary);
  const isLimited = summary.availability.is_limited || summary.warning_count > 0;

  return (
    <div className={clsx("mc-runbook-link-card", compact && "is-compact", className)}>
      <div className="mc-runbook-link-head">
        <div className="mc-runbook-link-title">
          <Workflow size={14} />
          <span>Runbook</span>
        </div>
        <div className="mc-runbook-link-chip-row">
          <Chip
            label={summary.status.replaceAll("_", " ")}
            tone={getRunbookStatusTone(summary.status)}
          />
          {isLimited ? <Chip label="limited" tone="warning" /> : null}
        </div>
      </div>
      <strong>{summary.primary_entity_label}</strong>
      {currentStepLabel ? (
        <p className="mc-runbook-link-step">Current step: {currentStepLabel}</p>
      ) : null}
      {!compact &&
      summary.status_reason &&
      summary.status_reason !== currentStepLabel ? (
        <p className="mc-runbook-link-reason">{summary.status_reason}</p>
      ) : null}
      {onOpen ? (
        <button
          type="button"
          className="ghost mc-runbook-link-open"
          onClick={onOpen}
        >
          <ArrowRight size={14} />
          {openLabel}
        </button>
      ) : null}
    </div>
  );
}
