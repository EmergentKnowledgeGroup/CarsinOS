import clsx from "clsx";
import type {
  AgentMailFileLeaseResponse,
  AgentMailMessageResponse,
  AgentMailThreadDetailResponse,
  AgentMailThreadSummaryResponse,
} from "../../types";
import { formatDateTime } from "../../utils/datetime";

interface ChatroomsPageProps {
  onRefresh: () => void;
  newRoomName: string;
  onNewRoomNameChange: (next: string) => void;
  newRoomParticipants: string;
  onNewRoomParticipantsChange: (next: string) => void;
  onCreateRoom: () => Promise<void>;
  roomThreads: AgentMailThreadSummaryResponse[];
  selectedRoomThreadId: string | null;
  onSelectRoomThread: (threadId: string) => void;
  roomThreadDetail: AgentMailThreadDetailResponse | null;
  roomMessages: AgentMailMessageResponse[];
  onPostRoomReaction: (emoji: string) => Promise<void>;
  mailPrincipalOverride: string;
  onAcknowledgeMessage: (messageId: string, principalOverride?: string) => Promise<void>;
  onDownloadAttachment: (
    messageId: string,
    attachmentId: string,
    filename: string
  ) => Promise<void>;
  chatComposeSender: string;
  onChatComposeSenderChange: (next: string) => void;
  chatComposeRecipients: string;
  onChatComposeRecipientsChange: (next: string) => void;
  chatComposeBody: string;
  onChatComposeBodyChange: (next: string) => void;
  chatAttachmentFiles: File[];
  onChatAttachmentFilesChange: (files: File[]) => void;
  onSendRoomMessage: () => Promise<void>;
  onAcknowledgeRoomUnread: () => Promise<void>;
  onReserveSelectedRoomWorkspace: () => Promise<void>;
  leases: AgentMailFileLeaseResponse[];
  onReleaseFileLease: (leaseId: string) => Promise<void>;
}

export function ChatroomsPage(props: ChatroomsPageProps) {
  const hasSelectedRoom = Boolean(props.selectedRoomThreadId);

  return (
    <section className="mc-mail-grid">
      <article className="mc-surface mc-mail-sidebar">
        <header className="mc-surface-header">
          <h2>Rooms</h2>
          <button type="button" onClick={props.onRefresh}>
            Refresh
          </button>
        </header>
        <div className="mc-mail-create-thread">
          <h3>Create Room</h3>
          <label htmlFor="new-room-name">Room Name</label>
          <input
            id="new-room-name"
            value={props.newRoomName}
            onChange={(event) => props.onNewRoomNameChange(event.target.value)}
            placeholder="room name"
          />
          <label htmlFor="new-room-participants">Participants (CSV)</label>
          <input
            id="new-room-participants"
            value={props.newRoomParticipants}
            onChange={(event) => props.onNewRoomParticipantsChange(event.target.value)}
            placeholder="participants csv (lyra, claude)"
          />
          <button type="button" onClick={() => void props.onCreateRoom()}>
            Create Room
          </button>
        </div>
        <div className="mc-mail-thread-list">
          {props.roomThreads.map((thread) => (
            <button
              type="button"
              key={thread.thread_id}
              className={clsx(
                "mc-mail-thread-item",
                props.selectedRoomThreadId === thread.thread_id && "active"
              )}
              onClick={() => props.onSelectRoomThread(thread.thread_id)}
            >
              <div className="mc-mail-thread-head">
                <strong>{thread.subject}</strong>
                <span className="chip">{thread.participant_count} members</span>
              </div>
              <p>{thread.latest_message_preview ?? "No room messages yet."}</p>
              <small>{formatDateTime(thread.latest_message_at)}</small>
            </button>
          ))}
          {props.roomThreads.length === 0 ? (
            <div className="mc-empty-drawer">No rooms found.</div>
          ) : null}
        </div>
      </article>

      <article className="mc-surface mc-mail-thread-view">
        <header className="mc-surface-header">
          <h2>{props.roomThreadDetail?.thread.subject ?? "Select a room"}</h2>
          <div className="mc-inline-actions">
            <button
              type="button"
              disabled={!hasSelectedRoom}
              aria-disabled={!hasSelectedRoom}
              onClick={() => void props.onPostRoomReaction(":+1:")}
            >
              +1
            </button>
            <button
              type="button"
              disabled={!hasSelectedRoom}
              aria-disabled={!hasSelectedRoom}
              onClick={() => void props.onPostRoomReaction(":eyes:")}
            >
              eyes
            </button>
            <button
              type="button"
              disabled={!hasSelectedRoom}
              aria-disabled={!hasSelectedRoom}
              onClick={() => void props.onPostRoomReaction(":white_check_mark:")}
            >
              done
            </button>
          </div>
        </header>
        <div className="mc-mail-message-stream">
          {props.roomMessages.map((message) => (
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
                      {attachment.filename}
                    </button>
                  ))}
                </div>
              ) : null}
            </article>
          ))}
          {props.roomMessages.length === 0 ? (
            <div className="mc-empty-drawer">No messages in this room yet.</div>
          ) : null}
        </div>
        <div className="mc-mail-compose">
          <label>
            Sender Principal
            <input
              value={props.chatComposeSender}
              onChange={(event) => props.onChatComposeSenderChange(event.target.value)}
              placeholder="optional sender override"
            />
          </label>
          <label>
            Mention recipients (CSV)
            <input
              value={props.chatComposeRecipients}
              onChange={(event) => props.onChatComposeRecipientsChange(event.target.value)}
              placeholder="optional explicit recipients"
            />
          </label>
          <label>
            Message
            <textarea
              value={props.chatComposeBody}
              onChange={(event) => props.onChatComposeBodyChange(event.target.value)}
              placeholder="Type to room..."
            />
          </label>
          <label className="upload-pill">
            <input
              type="file"
              multiple
              onChange={(event) =>
                props.onChatAttachmentFilesChange(Array.from(event.target.files ?? []))
              }
            />
            Attach files ({props.chatAttachmentFiles.length})
          </label>
          <button
            type="button"
            onClick={() => void props.onSendRoomMessage()}
            disabled={!props.selectedRoomThreadId}
          >
            Send to Room
          </button>
        </div>
      </article>

      <article className="mc-surface mc-mail-compose-panel">
        <header className="mc-surface-header">
          <h2>Room Moderation</h2>
          <p>Guardrails and audit-friendly controls</p>
        </header>
        <div className="mc-chatroom-side">
          <section>
            <h3>Participants</h3>
            <div className="mc-chip-cloud">
              {(props.roomThreadDetail?.participants ?? []).map((participant) => (
                <span key={participant.principal_id} className="chip">
                  {participant.principal_id}
                </span>
              ))}
              {(props.roomThreadDetail?.participants ?? []).length === 0 ? (
                <span className="chip">no participants loaded</span>
              ) : null}
            </div>
          </section>
          <section>
            <h3>Moderation Actions</h3>
            <div className="mc-inline-actions">
              <button type="button" onClick={() => void props.onAcknowledgeRoomUnread()}>
                Ack All Unread (principal)
              </button>
              <button type="button" onClick={() => void props.onReserveSelectedRoomWorkspace()}>
                Reserve Room Workspace
              </button>
            </div>
          </section>
          <section>
            <h3>Active Leases</h3>
            <ul className="mc-mail-lease-list">
              {props.leases.map((lease) => (
                <li key={lease.lease_id}>
                  <div>
                    <strong>{lease.glob_pattern}</strong>
                    <p>{lease.exclusive ? "exclusive" : "shared"}</p>
                  </div>
                  <button type="button" onClick={() => void props.onReleaseFileLease(lease.lease_id)}>
                    Release
                  </button>
                </li>
              ))}
              {props.leases.length === 0 ? <li>No active leases.</li> : null}
            </ul>
          </section>
        </div>
      </article>
    </section>
  );
}
