import clsx from "clsx";
import type {
  ChannelRuntimeAdapterStatusResponse,
  MissionControlFocusItem,
} from "../../types";
import { Chip } from "../../ui/Chip";
import { InlineActions } from "../../ui/InlineActions";
import { Surface } from "../../ui/Surface";
import { formatDateTime } from "../../utils/datetime";

interface FocusPageProps {
  focusItems: MissionControlFocusItem[];
  approvalsCount: number;
  channelStatuses: ChannelRuntimeAdapterStatusResponse[];
  onResolveFocusApproval: (approvalId: string, decision: "approve" | "deny") => Promise<void>;
  onRunCalendarJobNow: (jobId: string) => Promise<void>;
  onReconnectFocusChannel: (provider: string) => Promise<void>;
}

export function FocusPage(props: FocusPageProps) {
  return (
    <section className="mc-alt-grid">
      <Surface
        title="Operator Focus Queue"
        subtitle={`${props.focusItems.length} open attention items`}
      >
        <div className="mc-focus-list">
          {props.focusItems.map((item) => {
            const approvalId = String(item.action_payload.approval_id ?? "").trim();
            const jobId = String(item.action_payload.job_id ?? "").trim();
            const provider = String(item.action_payload.provider ?? "").trim();
            return (
              <article key={item.item_id} className={clsx("mc-focus-item", item.severity)}>
                <div className="mc-focus-head">
                  <Chip label={item.severity} tone={item.severity} />
                  <span>{item.category}</span>
                  <span>{formatDateTime(item.created_at)}</span>
                </div>
                <h3>{item.title}</h3>
                <p>{item.detail}</p>
                <InlineActions>
                  {item.category === "approval" ? (
                    <>
                      <button
                        type="button"
                        disabled={!approvalId}
                        aria-disabled={!approvalId}
                        onClick={() =>
                          approvalId
                            ? void props.onResolveFocusApproval(approvalId, "approve")
                            : undefined
                        }
                      >
                        Approve
                      </button>
                      <button
                        type="button"
                        className="danger"
                        disabled={!approvalId}
                        aria-disabled={!approvalId}
                        onClick={() =>
                          approvalId
                            ? void props.onResolveFocusApproval(approvalId, "deny")
                            : undefined
                        }
                      >
                        Deny
                      </button>
                    </>
                  ) : null}
                  {item.category === "run_failure" ? (
                    <button
                      type="button"
                      disabled={!jobId}
                      aria-disabled={!jobId}
                      onClick={() => (jobId ? void props.onRunCalendarJobNow(jobId) : undefined)}
                    >
                      Retry Job
                    </button>
                  ) : null}
                  {item.category === "channel_health" ? (
                    <button
                      type="button"
                      disabled={!provider}
                      aria-disabled={!provider}
                      onClick={() =>
                        provider ? void props.onReconnectFocusChannel(provider) : undefined
                      }
                    >
                      Reconnect Channel
                    </button>
                  ) : null}
                </InlineActions>
              </article>
            );
          })}
        </div>
      </Surface>
      <Surface title="Ops Snapshot" subtitle="Live queue and channel posture">
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
            <span>
              {
                props.channelStatuses.filter(
                  (item) => !item.healthy || item.lifecycle_state !== "running"
                ).length
              }
            </span>
          </li>
        </ul>
        <div className="mc-channel-grid">
          {props.channelStatuses.map((item) => (
            <article key={item.provider} className="mc-channel-card">
              <h3>{item.provider}</h3>
              <p>{item.lifecycle_state}</p>
              <p>{item.last_error ?? item.detail ?? (item.healthy ? "healthy" : "unhealthy")}</p>
              <button type="button" onClick={() => void props.onReconnectFocusChannel(item.provider)}>
                Reconnect
              </button>
            </article>
          ))}
        </div>
      </Surface>
    </section>
  );
}
