import type {
  MissionControlCalendarJob,
  MissionControlCalendarWeekResponse,
} from "../../types";
import { Chip } from "../../ui/Chip";
import { InlineActions } from "../../ui/InlineActions";
import { Surface } from "../../ui/Surface";
import { formatDateTime } from "../../utils/datetime";

interface CalendarPageProps {
  calendarWeek: MissionControlCalendarWeekResponse | null;
  calendarAlwaysRunning: MissionControlCalendarJob[];
  calendarNextUp: MissionControlCalendarJob[];
  calendarJobs: MissionControlCalendarJob[];
  onRunCalendarJobNow: (jobId: string) => Promise<void>;
  onToggleCalendarJob: (jobId: string, enabled: boolean) => Promise<void>;
}

export function CalendarPage(props: CalendarPageProps) {
  return (
    <section className="mc-alt-grid">
      <Surface
        title="Week Planning"
        subtitle={
          props.calendarWeek
            ? `${formatDateTime(props.calendarWeek.week_start_ms)} - ${formatDateTime(
                props.calendarWeek.week_end_ms
              )}`
            : "No week data loaded"
        }
      >
        <div className="mc-lane-grid">
          <section className="mc-lane-panel">
            <h3>Always Running</h3>
            <ul>
              {props.calendarAlwaysRunning.map((job) => (
                <li key={job.job_id}>
                  <div>
                    <strong>{job.name}</strong>
                    <p>{job.agent_id}</p>
                  </div>
                  <InlineActions>
                    <button type="button" onClick={() => void props.onRunCalendarJobNow(job.job_id)}>
                      Run now
                    </button>
                    <button
                      type="button"
                      className={job.enabled ? "danger" : ""}
                      onClick={() => void props.onToggleCalendarJob(job.job_id, !job.enabled)}
                    >
                      {job.enabled ? "Pause" : "Resume"}
                    </button>
                  </InlineActions>
                </li>
              ))}
            </ul>
          </section>
          <section className="mc-lane-panel">
            <h3>Next Up</h3>
            <ul>
              {props.calendarNextUp.map((job) => (
                <li key={job.job_id}>
                  <div>
                    <strong>{job.name}</strong>
                    <p>{formatDateTime(job.next_run_at)}</p>
                  </div>
                  <button type="button" onClick={() => void props.onRunCalendarJobNow(job.job_id)}>
                    Run now
                  </button>
                </li>
              ))}
            </ul>
          </section>
        </div>
      </Surface>
      <Surface title="Scheduler Matrix" subtitle={`${props.calendarJobs.length} jobs`}>
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
              {props.calendarJobs.map((job) => (
                <tr key={job.job_id}>
                  <td>
                    <strong>{job.name}</strong>
                    <p>{job.agent_id}</p>
                  </td>
                  <td>
                    {job.schedule_kind}
                    {job.interval_seconds ? ` / ${job.interval_seconds}s` : ""}
                    {job.cron_expr ? ` / ${job.cron_expr}` : ""}
                  </td>
                  <td>{formatDateTime(job.next_run_at)}</td>
                  <td>
                    <Chip label={job.enabled ? "enabled" : "paused"} tone={job.enabled ? "up" : "down"} />
                  </td>
                  <td>
                    <InlineActions>
                      <button type="button" onClick={() => void props.onRunCalendarJobNow(job.job_id)}>
                        Run
                      </button>
                      <button
                        type="button"
                        className={job.enabled ? "danger" : ""}
                        onClick={() => void props.onToggleCalendarJob(job.job_id, !job.enabled)}
                      >
                        {job.enabled ? "Pause" : "Resume"}
                      </button>
                    </InlineActions>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Surface>
    </section>
  );
}
