import { formatDateTime } from "../../utils/datetime";
import { EmptyState } from "../../ui/EmptyState";
import { Surface } from "../../ui/Surface";

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

export function EventsPage(props: EventsPageProps) {
  return (
    <section className="mc-alt-grid">
      <Surface
        title="Realtime Event Stream"
        headerRight={
          <label className="mc-checkbox">
            <input
              type="checkbox"
              checked={props.showRawEvents}
              onChange={(event) => props.onShowRawEventsChange(event.target.checked)}
            />
            Show raw heartbeat events
          </label>
        }
      >
        <div className="mc-events">
          {props.visibleEvents.map((event) => (
            <article key={event.event_id} className="mc-event-item">
              <div className="mc-event-head">
                <span>{event.event_type}</span>
                <span>{event.entity}</span>
                <span>{formatDateTime(event.ts_unix_ms)}</span>
              </div>
              <pre>{JSON.stringify(event.payload, null, 2)}</pre>
            </article>
          ))}
          {props.visibleEvents.length === 0 ? (
            <EmptyState message="No events captured yet." />
          ) : null}
        </div>
      </Surface>
    </section>
  );
}
