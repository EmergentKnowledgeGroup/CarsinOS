import clsx from "clsx";
import type {
  AgentMailFileLeaseResponse,
  AgentMailMessageResponse,
  AgentMailThreadDetailResponse,
  AgentMailThreadSummaryResponse,
} from "../../types";
import { formatDateTime } from "../../utils/datetime";
import { formatBytes } from "../../utils/files";

interface MailPageProps {
  onRefresh: () => void;
  mailboxFilter: "all" | "inbox" | "outbox";
  onMailboxFilterChange: (next: "all" | "inbox" | "outbox") => void;
  mailPrincipalOverride: string;
  onMailPrincipalOverrideChange: (next: string) => void;
  mailSearch: string;
  onMailSearchChange: (next: string) => void;
  newMailThreadSubject: string;
  onNewMailThreadSubjectChange: (next: string) => void;
  newMailThreadParticipants: string;
  onNewMailThreadParticipantsChange: (next: string) => void;
  onCreateDirectThread: () => Promise<void>;
  mailThreads: AgentMailThreadSummaryResponse[];
  selectedMailThreadId: string | null;
  onSelectMailThread: (threadId: string) => void;
  mailThreadDetail: AgentMailThreadDetailResponse | null;
  mailMessages: AgentMailMessageResponse[];
  onAcknowledgeMessage: (messageId: string, principalOverride?: string) => Promise<void>;
  onDownloadAttachment: (
    messageId: string,
    attachmentId: string,
    filename: string
  ) => Promise<void>;
  mailComposeSender: string;
  onMailComposeSenderChange: (next: string) => void;
  mailComposeRecipients: string;
  onMailComposeRecipientsChange: (next: string) => void;
  mailComposeBody: string;
  onMailComposeBodyChange: (next: string) => void;
  mailAttachmentFiles: File[];
  onMailAttachmentFilesChange: (files: File[]) => void;
  onSendMessage: () => Promise<void>;
  onSummarizeToNote: () => Promise<void>;
  leaseHolderPrincipal: string;
  onLeaseHolderPrincipalChange: (next: string) => void;
  leaseGlobPattern: string;
  onLeaseGlobPatternChange: (next: string) => void;
  leaseTtlMs: string;
  onLeaseTtlMsChange: (next: string) => void;
  leaseNote: string;
  onLeaseNoteChange: (next: string) => void;
  leaseExclusive: boolean;
  onLeaseExclusiveChange: (next: boolean) => void;
  onCreateFileLease: () => Promise<void>;
  leases: AgentMailFileLeaseResponse[];
  onReleaseFileLease: (leaseId: string) => Promise<void>;
}

export function MailPage(props: MailPageProps) {
  const handleMailboxFilterChange = (raw: string) => {
    if (raw === "all" || raw === "inbox" || raw === "outbox") {
      props.onMailboxFilterChange(raw);
    }
  };

  return (
    <section className="mc-mail-grid">
      <article className="mc-surface mc-mail-sidebar">
        <header className="mc-surface-header">
          <h2>Mail Threads</h2>
          <button type="button" onClick={props.onRefresh}>
            Refresh
          </button>
        </header>
        <div className="mc-mail-filters">
          <label>
            Mailbox
            <select
              value={props.mailboxFilter}
              onChange={(event) => handleMailboxFilterChange(event.target.value)}
            >
              <option value="inbox">inbox</option>
              <option value="outbox">outbox</option>
              <option value="all">all</option>
            </select>
          </label>
          <label>
            Principal Override
            <input
              value={props.mailPrincipalOverride}
              onChange={(event) => props.onMailPrincipalOverrideChange(event.target.value)}
              placeholder="optional principal id"
            />
          </label>
          <label>
            Search
            <input
              value={props.mailSearch}
              onChange={(event) => props.onMailSearchChange(event.target.value)}
              placeholder="subject/body contains..."
            />
          </label>
        </div>
        <div className="mc-mail-create-thread">
          <h3>New Direct Thread</h3>
          <label htmlFor="new-direct-thread-subject">Thread subject</label>
          <input
            id="new-direct-thread-subject"
            value={props.newMailThreadSubject}
            onChange={(event) => props.onNewMailThreadSubjectChange(event.target.value)}
            placeholder="Thread subject"
          />
          <label htmlFor="new-direct-thread-participants">Participants (comma-separated)</label>
          <input
            id="new-direct-thread-participants"
            value={props.newMailThreadParticipants}
            onChange={(event) => props.onNewMailThreadParticipantsChange(event.target.value)}
            placeholder="participants csv (lyra, claude)"
          />
          <button type="button" onClick={() => void props.onCreateDirectThread()}>
            Create Thread
          </button>
        </div>
        <div className="mc-mail-thread-list">
          {props.mailThreads.map((thread) => (
            <button
              type="button"
              key={thread.thread_id}
              className={clsx(
                "mc-mail-thread-item",
                props.selectedMailThreadId === thread.thread_id && "active"
              )}
              onClick={() => props.onSelectMailThread(thread.thread_id)}
            >
              <div className="mc-mail-thread-head">
                <strong>{thread.subject}</strong>
                {thread.unread_count > 0 ? (
                  <span className="chip chip-error">{thread.unread_count} unread</span>
                ) : null}
              </div>
              <p>{thread.latest_message_preview ?? "No messages yet."}</p>
              <small>
                {thread.latest_sender_principal ?? "n/a"} • {formatDateTime(thread.latest_message_at)}
              </small>
            </button>
          ))}
          {props.mailThreads.length === 0 ? (
            <div className="mc-empty-drawer">No direct threads for current filters.</div>
          ) : null}
        </div>
      </article>

      <article className="mc-surface mc-mail-thread-view">
        <header className="mc-surface-header">
          <h2>{props.mailThreadDetail?.thread.subject ?? "Select a thread"}</h2>
          <p>{props.mailMessages.length} message(s)</p>
        </header>
        <div className="mc-mail-message-stream">
          {props.mailMessages.map((message) => (
            <article key={message.message_id} className="mc-mail-message">
              <div className="mc-mail-message-head">
                <div>
                  <strong>{message.sender_principal}</strong>
                  <span>{formatDateTime(message.created_at)}</span>
                </div>
                <button
                  type="button"
                  onClick={() =>
                    void props.onAcknowledgeMessage(
                      message.message_id,
                      props.mailPrincipalOverride || undefined
                    )
                  }
                >
                  Ack
                </button>
              </div>
              <pre>{message.body_text}</pre>
              <div className="mc-mail-message-meta">
                <span>
                  to{" "}
                  {message.recipients
                    .map((recipient) => recipient.recipient_principal)
                    .join(", ")}
                </span>
                <span>
                  {
                    message.recipients.filter(
                      (recipient) => recipient.acked_at !== null
                    ).length
                  }
                  /{message.recipients.length} acked
                </span>
              </div>
              {message.attachments.length > 0 ? (
                <div className="mc-mail-attachment-row">
                  {message.attachments.map((attachment) => (
                    <button
                      type="button"
                      key={attachment.attachment_id}
                      onClick={() =>
                        void props.onDownloadAttachment(
                          message.message_id,
                          attachment.attachment_id,
                          attachment.filename
                        )
                      }
                    >
                      {attachment.filename} ({formatBytes(attachment.bytes)})
                    </button>
                  ))}
                </div>
              ) : null}
            </article>
          ))}
          {props.mailMessages.length === 0 ? (
            <div className="mc-empty-drawer">No messages in this thread yet.</div>
          ) : null}
        </div>
      </article>

      <article className="mc-surface mc-mail-compose-panel">
        <header className="mc-surface-header">
          <h2>Compose + Leases</h2>
          <p>Dispatch, summarize, and coordinate safely</p>
        </header>
        <div className="mc-mail-compose">
          <label>
            Sender Principal
            <input
              value={props.mailComposeSender}
              onChange={(event) => props.onMailComposeSenderChange(event.target.value)}
              placeholder="optional sender override"
            />
          </label>
          <label>
            Recipients (CSV)
            <input
              value={props.mailComposeRecipients}
              onChange={(event) => props.onMailComposeRecipientsChange(event.target.value)}
              placeholder="blank = thread participants"
            />
          </label>
          <label>
            Body
            <textarea
              value={props.mailComposeBody}
              onChange={(event) => props.onMailComposeBodyChange(event.target.value)}
              placeholder="Write a clear handoff message..."
            />
          </label>
          <label className="upload-pill">
            <input
              type="file"
              multiple
              onChange={(event) =>
                props.onMailAttachmentFilesChange(Array.from(event.target.files ?? []))
              }
            />
            Attach files ({props.mailAttachmentFiles.length})
          </label>
          <div className="mc-inline-actions">
            <button
              type="button"
              onClick={() => void props.onSendMessage()}
              disabled={!props.selectedMailThreadId}
            >
              Send
            </button>
            <button type="button" onClick={() => void props.onSummarizeToNote()}>
              Summarize to Note
            </button>
          </div>
        </div>
        <section className="mc-mail-lease-panel">
          <h3>Advisory File Leases</h3>
          <div className="mc-mail-lease-form">
            <label htmlFor="lease-holder-principal">Holder principal</label>
            <input
              id="lease-holder-principal"
              value={props.leaseHolderPrincipal}
              onChange={(event) => props.onLeaseHolderPrincipalChange(event.target.value)}
              placeholder="holder principal (optional)"
            />
            <label htmlFor="lease-glob-pattern">Glob pattern</label>
            <input
              id="lease-glob-pattern"
              value={props.leaseGlobPattern}
              onChange={(event) => props.onLeaseGlobPatternChange(event.target.value)}
              placeholder="glob pattern"
            />
            <label htmlFor="lease-ttl-ms">TTL (ms)</label>
            <input
              id="lease-ttl-ms"
              value={props.leaseTtlMs}
              onChange={(event) => props.onLeaseTtlMsChange(event.target.value)}
              placeholder="ttl ms"
            />
            <label htmlFor="lease-note">Note</label>
            <input
              id="lease-note"
              value={props.leaseNote}
              onChange={(event) => props.onLeaseNoteChange(event.target.value)}
              placeholder="note (optional)"
            />
            <label className="mc-checkbox">
              <input
                type="checkbox"
                checked={props.leaseExclusive}
                onChange={(event) => props.onLeaseExclusiveChange(event.target.checked)}
              />
              Exclusive lock
            </label>
            <button type="button" onClick={() => void props.onCreateFileLease()}>
              Reserve
            </button>
          </div>
          <ul className="mc-mail-lease-list">
            {props.leases.map((lease) => (
              <li key={lease.lease_id}>
                <div>
                  <strong>{lease.glob_pattern}</strong>
                  <p>
                    {lease.holder_principal} • expires {formatDateTime(lease.expires_at)}
                  </p>
                </div>
                <button type="button" onClick={() => void props.onReleaseFileLease(lease.lease_id)}>
                  Release
                </button>
              </li>
            ))}
            {props.leases.length === 0 ? <li>No active leases.</li> : null}
          </ul>
        </section>
      </article>
    </section>
  );
}
