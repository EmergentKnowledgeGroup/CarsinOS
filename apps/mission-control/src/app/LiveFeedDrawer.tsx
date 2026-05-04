import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Badge } from "../ui/Badge";
import { formatDateTime, formatRelative } from "../utils/datetime";
import type {
  LiveFeedDomain,
  LiveFeedEvent,
  LiveFeedSeverityFilter,
} from "../lib/liveFeed";
import type { LiveFeedStorageMode } from "./useLiveFeedController";

interface LiveFeedDrawerProps {
  enabled: boolean;
  open: boolean;
  paused: boolean;
  unreadCount: number;
  domainFilter: LiveFeedDomain;
  severityFilter: LiveFeedSeverityFilter;
  events: LiveFeedEvent[];
  storageMode: LiveFeedStorageMode;
  storageError: string | null;
  recoveryAvailableCount: number;
  markAllUndoAvailable: boolean;
  clearUndoAvailable: boolean;
  approvalsCount: number;
  openBreakersCount: number;
  mailUnreadCount: number;
  onToggleOpen: () => void;
  onTogglePause: () => void;
  onDomainFilterChange: (value: LiveFeedDomain) => void;
  onSeverityFilterChange: (value: LiveFeedSeverityFilter) => void;
  onMarkAllRead: () => void;
  onUndoMarkAllRead: () => void;
  onClearSoft: () => void;
  onRestoreClear: () => void;
  onRestoreRecovery: () => void;
}

const DOMAIN_FILTERS: Array<{ id: LiveFeedDomain; label: string }> = [
  { id: "all", label: "All" },
  { id: "approvals", label: "Approvals" },
  { id: "jobs", label: "Jobs" },
  { id: "boards", label: "Boards" },
  { id: "mail", label: "Mail" },
  { id: "channels", label: "Channels" },
  { id: "system", label: "System" },
  { id: "other", label: "Other" },
];

const SEVERITY_FILTERS: Array<{ id: LiveFeedSeverityFilter; label: string }> = [
  { id: "all", label: "All" },
  { id: "critical_high", label: "Critical & High" },
  { id: "critical", label: "Critical" },
  { id: "high", label: "High" },
  { id: "normal", label: "Normal" },
  { id: "low", label: "Low" },
];

function EventPayload({ event }: { event: LiveFeedEvent }) {
  return <pre className="mc-live-feed-event-payload">{JSON.stringify(event.payload_redacted, null, 2)}</pre>;
}

function EventCard({ event }: { event: LiveFeedEvent }) {
  const [expanded, setExpanded] = useState(false);
  const severityClass = `mc-live-feed-severity-${event.severity}`;

  return (
    <article className={`mc-live-feed-event ${severityClass}`}>
      <div className="mc-live-feed-event-head">
        <div className="mc-live-feed-event-meta">
          <span className="mc-live-feed-event-domain">{event.domain}</span>
          <span className="mc-live-feed-event-severity">{event.severity}</span>
          <span className="mc-live-feed-event-type">{event.event_type}</span>
        </div>
        <span className="mc-live-feed-event-time" title={formatDateTime(event.ts_unix_ms)}>
          {formatRelative(event.ts_unix_ms)}
        </span>
      </div>
      <p className="mc-live-feed-event-summary">{event.summary}</p>
      <div className="mc-live-feed-event-actions">
        <span className="mc-live-feed-event-entity">{event.entity}</span>
        <button type="button" className="ghost" onClick={() => setExpanded((value) => !value)}>
          {expanded ? "Hide payload" : "Expand payload"}
        </button>
      </div>
      {expanded ? <EventPayload event={event} /> : null}
    </article>
  );
}

export function LiveFeedDrawer(props: LiveFeedDrawerProps) {
  const parentRef = useRef<HTMLDivElement | null>(null);

  const summaryText = useMemo(() => {
    if (props.events.length === 0) {
      return "No events in the current filter.";
    }
    return `${props.events.length} events in view`;
  }, [props.events.length]);

  // eslint-disable-next-line react-hooks/incompatible-library
  const rowVirtualizer = useVirtualizer({
    count: props.events.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 104,
    overscan: 8,
  });

  if (!props.enabled) {
    return null;
  }

  return (
    <aside
      className={`mc-live-feed-drawer ${props.open ? "mc-live-feed-drawer-open" : "mc-live-feed-drawer-closed"}`}
      data-testid="live-feed-drawer"
      data-open={props.open ? "true" : "false"}
      aria-label="Live Feed"
      aria-hidden={props.open ? undefined : true}
      inert={props.open ? undefined : true}
    >
      <header className="mc-live-feed-header">
        <div>
          <h2>Live Feed</h2>
          <p>{summaryText}</p>
        </div>
        <div className="mc-live-feed-header-actions">
          <Badge count={props.unreadCount} tone={props.unreadCount > 0 ? "danger" : "accent"} />
          <button type="button" className="ghost" onClick={props.onToggleOpen}>
            {props.open ? "Hide" : "Show"}
          </button>
        </div>
      </header>

      <div className="mc-live-feed-count-strip">
        <span>Approvals: {props.approvalsCount}</span>
        <span>Breakers: {props.openBreakersCount}</span>
        <span>Mail unread: {props.mailUnreadCount}</span>
      </div>

      {props.open ? (
        <>
          <div className="mc-live-feed-toolbar">
            <button
              type="button"
              className={`mc-filter-chip ${props.paused ? "mc-filter-chip-active" : ""}`}
              aria-pressed={props.paused}
              data-testid="live-feed-pause"
              onClick={props.onTogglePause}
            >
              {props.paused ? "Paused" : "Pause"}
            </button>
            <button type="button" className="ghost" onClick={props.onMarkAllRead}>
              <span data-testid="live-feed-mark-all-read">Mark all read</span>
            </button>
            <button type="button" className="ghost" onClick={props.onClearSoft}>
              <span data-testid="live-feed-soft-clear">Soft clear</span>
            </button>
          </div>

          <div className="mc-live-feed-toolbar mc-live-feed-toolbar-wrap">
            {DOMAIN_FILTERS.map((filter) => (
              <button
                key={filter.id}
                type="button"
                className={`mc-filter-chip ${props.domainFilter === filter.id ? "mc-filter-chip-active" : ""}`}
                aria-pressed={props.domainFilter === filter.id}
                onClick={() => props.onDomainFilterChange(filter.id)}
              >
                {filter.label}
              </button>
            ))}
          </div>

          <div className="mc-live-feed-toolbar mc-live-feed-toolbar-wrap">
            {SEVERITY_FILTERS.map((filter) => (
              <button
                key={filter.id}
                type="button"
                className={`mc-filter-chip ${props.severityFilter === filter.id ? "mc-filter-chip-active" : ""}`}
                aria-pressed={props.severityFilter === filter.id}
                onClick={() => props.onSeverityFilterChange(filter.id)}
              >
                {filter.label}
              </button>
            ))}
          </div>

          <div className="mc-live-feed-undo-row">
            {props.markAllUndoAvailable ? (
              <button type="button" className="ghost" onClick={props.onUndoMarkAllRead} data-testid="live-feed-undo-mark-all-read">
                Undo mark all read
              </button>
            ) : null}
            {props.clearUndoAvailable ? (
              <button type="button" className="ghost" onClick={props.onRestoreClear} data-testid="live-feed-undo-soft-clear">
                Undo soft clear
              </button>
            ) : null}
            {props.recoveryAvailableCount > 0 ? (
              <button type="button" className="ghost" onClick={props.onRestoreRecovery} data-testid="live-feed-restore-history">
                Restore 30m history ({props.recoveryAvailableCount})
              </button>
            ) : null}
          </div>

          <div className="mc-live-feed-storage-note">
            <span>Recovery mode: {props.storageMode === "durable" ? "durable" : "memory-only"}</span>
            {props.storageError ? <span>{props.storageError}</span> : null}
          </div>

          <div className="mc-live-feed-scroll" ref={parentRef}>
            {props.events.length === 0 ? (
              <div className="mc-live-feed-empty">No events yet.</div>
            ) : (
              <div
                style={{
                  height: `${rowVirtualizer.getTotalSize()}px`,
                  position: "relative",
                  width: "100%",
                }}
              >
                {rowVirtualizer.getVirtualItems().map((virtualItem) => {
                  const event = props.events[virtualItem.index];
                  if (!event) {
                    return null;
                  }
                  return (
                    <div
                      key={virtualItem.key}
                      data-index={virtualItem.index}
                      ref={rowVirtualizer.measureElement}
                      style={{
                        left: 0,
                        position: "absolute",
                        top: 0,
                        transform: `translateY(${virtualItem.start}px)`,
                        width: "100%",
                      }}
                    >
                      <EventCard event={event} />
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </>
      ) : (
        <div className="mc-live-feed-collapsed">
          <span>Unread: {props.unreadCount}</span>
          {(props.approvalsCount > 0 || props.openBreakersCount > 0) ? (
            <span className="mc-live-feed-collapsed-detail">
              {props.approvalsCount > 0 ? `${props.approvalsCount} approval${props.approvalsCount !== 1 ? "s" : ""}` : null}
              {props.approvalsCount > 0 && props.openBreakersCount > 0 ? " \u00B7 " : null}
              {props.openBreakersCount > 0 ? `${props.openBreakersCount} breaker${props.openBreakersCount !== 1 ? "s" : ""}` : null}
            </span>
          ) : null}
        </div>
      )}
    </aside>
  );
}
