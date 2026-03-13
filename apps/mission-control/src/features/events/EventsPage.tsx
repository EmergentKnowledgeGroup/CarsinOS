import { useState, useMemo } from "react";
import { formatDateTime, formatRelative } from "../../utils/datetime";
import { EmptyState } from "../../ui/EmptyState";
import { Pagination } from "../../ui/Pagination";
import { Surface } from "../../ui/Surface";
import { usePagination } from "../../ui/usePagination";
import { eventDomain, eventSummary, redactEventPayload } from "../../lib/eventStream";

export interface EventsPageEventItem {
  event_id: string;
  event_type: string;
  entity: string;
  ts_unix_ms: number;
  payload: Record<string, unknown>;
}

interface EventsPageProps {
  showRawEvents: boolean;
  onShowRawEventsChange: (next: boolean) => void;
  visibleEvents: EventsPageEventItem[];
}

const EVENTS_PAGE_SIZE = 12;

type DomainFilter = "all" | "board" | "job" | "approval" | "channel" | "agent_mail" | "heartbeat";

const DOMAIN_FILTERS: { id: DomainFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "board", label: "Board" },
  { id: "job", label: "Job" },
  { id: "approval", label: "Approval" },
  { id: "channel", label: "Channel" },
  { id: "agent_mail", label: "Mail" },
];

/** Color-code left border by event domain */
function domainTone(domain: string): string {
  switch (domain) {
    case "board": return "accent";
    case "job": return "info";
    case "approval": return "warn";
    case "channel": return "purple";
    case "agent_mail": return "ok";
    case "heartbeat": return "muted";
    default: return "";
  }
}


function EventItem({ event }: { event: EventsPageEventItem }) {
  const [expanded, setExpanded] = useState(false);
  const domain = eventDomain(event.event_type);
  const tone = domainTone(domain);
  const summary = eventSummary(event.event_type, event.payload);
  const redactedPayload = expanded ? redactEventPayload(event.payload) : null;
  const payloadId = `mc-event-payload-${event.event_id}`;

  return (
    <article className={`mc-event-item mc-event-domain-${tone}`}>
      <div className="mc-event-head">
        <span className="mc-event-type">{event.event_type}</span>
        {summary ? <span className="mc-event-summary">{summary}</span> : null}
        <span className="mc-event-entity">{event.entity}</span>
        <span className="mc-event-time" title={formatDateTime(event.ts_unix_ms)}>{formatRelative(event.ts_unix_ms)}</span>
        <button
          type="button"
          className="mc-event-expand ghost"
          onClick={() => setExpanded(!expanded)}
          aria-expanded={expanded}
          aria-controls={payloadId}
        >
          {expanded ? "▾ Hide" : "▸ JSON"}
        </button>
      </div>
      {expanded ? (
        <pre id={payloadId} className="mc-event-payload" aria-hidden={!expanded}>
          {JSON.stringify(redactedPayload, null, 2)}
        </pre>
      ) : null}
    </article>
  );
}

export function EventsPage(props: EventsPageProps) {
  const [domainFilter, setDomainFilter] = useState<DomainFilter>("all");
  const [eventsPage, setEventsPage] = useState(1);

  const filtered = useMemo(() => {
    if (domainFilter === "all") return props.visibleEvents;
    return props.visibleEvents.filter((e) => eventDomain(e.event_type) === domainFilter);
  }, [props.visibleEvents, domainFilter]);

  const pagination = usePagination(filtered, EVENTS_PAGE_SIZE);
  const pageItems = pagination.getPage(eventsPage);

  return (
    <section className="mc-alt-grid">
      <Surface
        title="Realtime Event Stream"
        subtitle={`${filtered.length} events`}
        headerRight={
          <label className="mc-checkbox">
            <input
              type="checkbox"
              checked={props.showRawEvents}
              onChange={(event) => props.onShowRawEventsChange(event.target.checked)}
            />
            Show heartbeats
          </label>
        }
      >
        <div className="mc-event-filters">
          {DOMAIN_FILTERS.map((f) => (
            <button
              key={f.id}
              type="button"
              className={`mc-filter-chip ${domainFilter === f.id ? "mc-filter-chip-active" : ""}`}
              aria-pressed={domainFilter === f.id}
              onClick={() => { setDomainFilter(f.id); setEventsPage(1); }}
            >
              {f.label}
            </button>
          ))}
        </div>
        <div className="mc-events">
          {pageItems.map((event) => (
            <EventItem key={event.event_id} event={event} />
          ))}
          {filtered.length === 0 ? (
            <EmptyState message={props.visibleEvents.length === 0 ? "Listening for events\u2026" : "No events match the current filter."} />
          ) : null}
        </div>
        <Pagination currentPage={eventsPage} totalPages={pagination.totalPages} onPageChange={setEventsPage} />
      </Surface>
    </section>
  );
}
