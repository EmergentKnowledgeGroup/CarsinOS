import { useEffect, useRef } from "react";
import { AlertCircle, CheckCircle2, Clock, FileText, X } from "lucide-react";
import type {
  AssistantDeskResponse,
  AssistantDeskTranscriptEvent,
  AssistantDeskWorkItem,
} from "../../types";
import { AssistantMarkdown } from "../assistant/AssistantMarkdown";
import type { useAssistantDeskController } from "./useAssistantDeskController";

const BUCKETS = [
  { key: "needs_you", label: "Needs you" },
  { key: "working", label: "Working" },
  { key: "done_recently", label: "Done recently" },
] as const;

const STATUS_LABELS: Record<string, string> = {
  needs_you: "Needs you",
  blocked: "Blocked",
  failed: "Failed",
  working: "Working",
  waiting: "Waiting",
  done: "Done",
  stale: "Stale",
};

type AssistantDeskController = ReturnType<typeof useAssistantDeskController>;

function statusLabel(status: string): string {
  return STATUS_LABELS[status] ?? status.replaceAll("_", " ");
}

function formatAge(value: string): string {
  const at = Date.parse(value);
  if (!Number.isFinite(at)) {
    return "updated just now";
  }
  const deltaSeconds = Math.max(0, Math.round((Date.now() - at) / 1000));
  if (deltaSeconds < 60) {
    return "updated just now";
  }
  const minutes = Math.round(deltaSeconds / 60);
  if (minutes < 60) {
    return `updated ${minutes}m ago`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 24) {
    return `updated ${hours}h ago`;
  }
  return `updated ${Math.round(hours / 24)}d ago`;
}

function bucketItems(
  desk: AssistantDeskResponse | null,
  bucket: (typeof BUCKETS)[number]["key"]
): AssistantDeskWorkItem[] {
  return desk?.buckets[bucket] ?? [];
}

function filesLabel(item: AssistantDeskWorkItem): string {
  return item.changed_file_count > 0
    ? `Files: ${item.changed_file_count}`
    : "Files: not tracked yet";
}

function artifactsLabel(item: AssistantDeskWorkItem): string {
  return item.artifact_count > 0 ? `Artifacts: ${item.artifact_count}` : "Artifacts: none yet";
}

function StatusIcon({ status }: { status: string }) {
  if (status === "needs_you" || status === "blocked" || status === "failed") {
    return <AlertCircle size={15} aria-hidden="true" />;
  }
  if (status === "done") {
    return <CheckCircle2 size={15} aria-hidden="true" />;
  }
  return <Clock size={15} aria-hidden="true" />;
}

export function AssistantDeskStatusStrip(props: {
  controller: AssistantDeskController;
  onOpenDesk: () => void;
}) {
  const { controller } = props;
  const total =
    (controller.desk?.summary.needs_you_count ?? 0) +
    (controller.desk?.summary.working_count ?? 0) +
    (controller.desk?.summary.done_recently_count ?? 0);
  const empty = total === 0 && !controller.loading;

  return (
    <section className="mc-assistant-desk-strip" aria-label="Assistant Desk status">
      <div className="mc-assistant-desk-strip-items">
        {controller.visibleStatusItems.map((item) => (
          <button
            type="button"
            key={item.id}
            className={`mc-assistant-desk-chip mc-assistant-desk-chip-${item.status}`}
            onClick={() => {
              controller.selectWorkItem(item.id);
              props.onOpenDesk();
            }}
          >
            <span className="mc-assistant-desk-dot" aria-hidden="true" />
            <span>{statusLabel(item.status)}</span>
            <strong>{item.title}</strong>
          </button>
        ))}
        {controller.overflowStatusCount > 0 ? (
          <button
            type="button"
            className="mc-assistant-desk-chip"
            onClick={props.onOpenDesk}
          >
            +{controller.overflowStatusCount} more
          </button>
        ) : null}
        {empty ? <span className="mc-assistant-desk-idle">ExecAss is idle.</span> : null}
        {controller.loading ? (
          <span className="mc-assistant-desk-idle" role="status">
            Checking Desk...
          </span>
        ) : null}
        {controller.stale ? (
          <span className="mc-assistant-desk-warning">Last update is stale</span>
        ) : null}
      </div>
      <button type="button" className="ghost" onClick={props.onOpenDesk}>
        Open Desk
      </button>
    </section>
  );
}

function WorkCard(props: {
  item: AssistantDeskWorkItem;
  selected: boolean;
  onSelect: () => void;
  onOpenTranscript: (trigger: HTMLButtonElement) => void;
}) {
  return (
    <article
      className={`mc-assistant-desk-card${props.selected ? " mc-assistant-desk-card-selected" : ""}`}
    >
      <div className="mc-assistant-desk-card-main">
        <div className={`mc-assistant-desk-status mc-assistant-desk-status-${props.item.status}`}>
          <StatusIcon status={props.item.status} />
          <span>{statusLabel(props.item.status)}</span>
        </div>
        <h4>{props.item.title}</h4>
        <p>{props.item.current_action || props.item.task_label}</p>
        <div className="mc-assistant-desk-card-facts">
          <span>{props.item.owner_label}</span>
          <span>{formatAge(props.item.last_event_at)}</span>
          <span>{filesLabel(props.item)}</span>
          <span>{artifactsLabel(props.item)}</span>
        </div>
      </div>
      <div className="mc-assistant-desk-card-actions">
        <button
          type="button"
          className="ghost"
          onClick={(event) => props.onOpenTranscript(event.currentTarget)}
        >
          <FileText size={15} /> Transcript
        </button>
        <button type="button" className="ghost" onClick={props.onSelect}>
          Details
        </button>
      </div>
      {props.selected ? (
        <div className="mc-assistant-desk-details">
          <span>Provider: {props.item.details.provider_label || "not set"}</span>
          <span>Model: {props.item.details.model_label || "not set"}</span>
          <span>Workspace: {props.item.details.workspace_label || "not tracked yet"}</span>
          <span>Health: {props.item.details.source_health || "fresh"}</span>
          <span>Error: {props.item.details.last_error || "none"}</span>
        </div>
      ) : null}
    </article>
  );
}

function TranscriptBody({ event }: { event: AssistantDeskTranscriptEvent }) {
  const content = event.body_markdown || event.text || "";
  if (!content.trim()) {
    return <p className="mc-assistant-desk-muted">No detail recorded.</p>;
  }
  return <AssistantMarkdown content={content} />;
}

function TranscriptDrawer(props: {
  controller: AssistantDeskController;
  onClose: () => void;
}) {
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const { onClose } = props;

  useEffect(() => {
    closeButtonRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const transcript = props.controller.transcript;
  const title =
    props.controller.selectedWorkItem?.title || transcript?.title || "Assistant transcript";

  return (
    <aside
      className="mc-assistant-desk-transcript"
      role="dialog"
      aria-modal="true"
      aria-label={`${title} transcript`}
    >
      <header>
        <div>
          <p>Transcript</p>
          <h3>{title}</h3>
        </div>
        <button type="button" className="ghost" onClick={props.onClose} ref={closeButtonRef}>
          <X size={16} /> Close
        </button>
      </header>
      <button
        type="button"
        className="ghost mc-assistant-desk-jump"
        onClick={() => {
          const pane = document.querySelector(".mc-assistant-desk-transcript-events");
          pane?.scrollTo({ top: pane.scrollHeight, behavior: "smooth" });
        }}
      >
        Jump to latest
      </button>
      <div className="mc-assistant-desk-transcript-events">
        {props.controller.transcriptLoading ? (
          <p className="mc-assistant-desk-muted">Loading transcript...</p>
        ) : null}
        {props.controller.transcriptError ? (
          <p className="mc-form-error">{props.controller.transcriptError}</p>
        ) : null}
        {transcript?.events.length ? (
          transcript.events.map((event) => (
            <article key={event.id} className="mc-assistant-desk-event">
              <div className="mc-assistant-desk-event-meta">
                <strong>{event.title || event.role || event.source || "System"}</strong>
                <span>{event.at}</span>
              </div>
              <TranscriptBody event={event} />
            </article>
          ))
        ) : !props.controller.transcriptLoading ? (
          <p className="mc-assistant-desk-muted">
            No transcript events yet. CarsinOS will show the audit trail here as work arrives.
          </p>
        ) : null}
      </div>
    </aside>
  );
}

export function AssistantDeskPanel(props: {
  open: boolean;
  controller: AssistantDeskController;
  onClose: () => void;
}) {
  const transcriptTriggerRef = useRef<HTMLButtonElement | null>(null);
  const closeTranscript = () => {
    props.controller.closeTranscript();
    window.requestAnimationFrame(() => {
      transcriptTriggerRef.current?.focus();
    });
  };

  if (!props.open) {
    return null;
  }

  const desk = props.controller.desk;
  const hasItems = props.controller.allItems.length > 0;

  return (
    <section className="mc-surface mc-assistant-desk-panel" aria-label="Assistant Desk">
      <header className="mc-assistant-desk-header">
        <div>
          <p>Assistant Desk</p>
          <h3>What ExecAss is juggling</h3>
        </div>
        <button type="button" className="ghost" onClick={props.onClose}>
          <X size={16} /> Close
        </button>
      </header>
      {!hasItems && !props.controller.loading ? (
        <div className="mc-assistant-desk-empty">
          <strong>Nothing needs your attention right now.</strong>
          <span>
            Active runs, approvals, and recent finishes will appear here while you use Assistant.
          </span>
        </div>
      ) : null}
      <div className="mc-assistant-desk-buckets">
        {BUCKETS.map((bucket) => {
          const items = bucketItems(desk, bucket.key);
          return (
            <section key={bucket.key} className="mc-assistant-desk-bucket">
              <h4>{bucket.label}</h4>
              {items.length === 0 ? (
                <p className="mc-assistant-desk-muted">Nothing here.</p>
              ) : (
                items.map((item) => (
                  <WorkCard
                    key={item.id}
                    item={item}
                    selected={props.controller.selectedWorkItemId === item.id}
                    onSelect={() =>
                      props.controller.selectWorkItem(
                        props.controller.selectedWorkItemId === item.id ? null : item.id
                      )
                    }
                    onOpenTranscript={(trigger) => {
                      transcriptTriggerRef.current = trigger;
                      void props.controller.openTranscript(item.id);
                    }}
                  />
                ))
              )}
            </section>
          );
        })}
      </div>
      {props.controller.transcript || props.controller.transcriptLoading || props.controller.transcriptError ? (
        <TranscriptDrawer
          controller={props.controller}
          onClose={closeTranscript}
        />
      ) : null}
    </section>
  );
}
