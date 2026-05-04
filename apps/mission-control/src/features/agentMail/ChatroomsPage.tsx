import { useState, useRef, useEffect } from "react";
import clsx from "clsx";
import { SmilePlus } from "lucide-react";
import type {
  Agent,
  AgentMailFileLeaseResponse,
  AgentMailMessageResponse,
  AgentMailThreadDetailResponse,
  AgentMailThreadSummaryResponse,
} from "../../types";
import { formatDateTime, formatRelative } from "../../utils/datetime";
import { AgentPicker } from "../../ui/AgentPicker";
import { Avatar } from "../../ui/Avatar";
import { Modal } from "../../ui/Modal";
import { Pagination } from "../../ui/Pagination";
import { usePagination } from "../../ui/usePagination";

const ROOMS_PAGE_SIZE = 8;
const ROOM_MESSAGES_PAGE_SIZE = 10;

const REACTION_EMOJI = [
  { code: ":+1:", display: "\uD83D\uDC4D" },
  { code: ":eyes:", display: "\uD83D\uDC40" },
  { code: ":white_check_mark:", display: "\u2705" },
  { code: ":fire:", display: "\uD83D\uDD25" },
  { code: ":heart:", display: "\u2764\uFE0F" },
  { code: ":clap:", display: "\uD83D\uDC4F" },
  { code: ":thinking:", display: "\uD83E\uDD14" },
  { code: ":warning:", display: "\u26A0\uFE0F" },
  { code: ":rocket:", display: "\uD83D\uDE80" },
  { code: ":100:", display: "\uD83D\uDCAF" },
  { code: ":raised_hands:", display: "\uD83D\uDE4C" },
  { code: ":x:", display: "\u274C" },
];

interface ChatroomsPageProps {
  onRefresh: () => void;
  agents: Agent[];
  mailboxFilter: "all" | "inbox" | "outbox";
  mailSearch: string;
  newRoomName: string;
  onNewRoomNameChange: (next: string) => void;
  newRoomParticipants: string;
  onNewRoomParticipantsChange: (next: string) => void;
  onCreateRoom: () => Promise<boolean>;
  roomThreads: AgentMailThreadSummaryResponse[];
  selectedRoomThreadId: string | null;
  onSelectRoomThread: (threadId: string) => void;
  roomThreadDetail: AgentMailThreadDetailResponse | null;
  roomMessages: AgentMailMessageResponse[];
  onPostRoomReaction: (emoji: string) => Promise<void>;
  mailPrincipalOverride: string;
  onMailboxFilterChange: (next: "all" | "inbox" | "outbox") => void;
  onMailPrincipalOverrideChange: (next: string) => void;
  onMailSearchChange: (next: string) => void;
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
  onReleaseFileLease: (leaseId: string) => Promise<boolean>;
}

export function ChatroomsPage(props: ChatroomsPageProps) {
  const hasSelectedRoom = Boolean(props.selectedRoomThreadId);
  const [createRoomOpen, setCreateRoomOpen] = useState(false);
  const [moderationOpen, setModerationOpen] = useState(false);
  const [releaseLeaseId, setReleaseLeaseId] = useState<string | null>(null);
  const [chatOptionsOpen, setChatOptionsOpen] = useState(false);
  const [emojiPickerOpen, setEmojiPickerOpen] = useState(false);
  const [sending, setSending] = useState(false);
  const [createRoomBusy, setCreateRoomBusy] = useState(false);
  const [releaseLeaseBusy, setReleaseLeaseBusy] = useState(false);
  const [busyActions, setBusyActions] = useState<Set<string>>(new Set());
  const [roomsPage, setRoomsPage] = useState(1);
  const [roomMsgsPage, setRoomMsgsPage] = useState(1);
  const emojiRef = useRef<HTMLDivElement>(null);
  const busyActionRef = useRef<Set<string>>(new Set());

  // Close emoji picker on outside click
  useEffect(() => {
    if (!emojiPickerOpen) return;
    const handler = (e: MouseEvent) => {
      if (emojiRef.current && !emojiRef.current.contains(e.target as Node)) {
        setEmojiPickerOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [emojiPickerOpen]);

  const roomsPagination = usePagination(props.roomThreads, ROOMS_PAGE_SIZE);
  const roomMsgsPagination = usePagination(props.roomMessages, ROOM_MESSAGES_PAGE_SIZE);
  const visibleRooms = roomsPagination.getPage(roomsPage);
  const visibleRoomMsgs = roomMsgsPagination.getPage(roomMsgsPage);
  const hasActiveFilters =
    props.mailboxFilter !== "all" ||
    props.mailPrincipalOverride.trim().length > 0 ||
    props.mailSearch.trim().length > 0;

  const runBusyAction = (key: string, fn: () => Promise<unknown>) => {
    if (busyActionRef.current.has(key)) {
      return;
    }
    busyActionRef.current.add(key);
    setBusyActions(new Set(busyActionRef.current));
    void fn()
      .catch((error: unknown) => {
        console.error("chatroom action failed", { key, error });
      })
      .finally(() => {
        busyActionRef.current.delete(key);
        setBusyActions(new Set(busyActionRef.current));
      });
  };

  const isBusyAction = (key: string) => busyActions.has(key);

  const handleCreateRoom = async () => {
    if (createRoomBusy) {
      return;
    }
    setCreateRoomBusy(true);
    try {
      const created = await props.onCreateRoom();
      if (created) {
        setCreateRoomOpen(false);
      }
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      setCreateRoomBusy(false);
    }
  };

  const handleReleaseLease = async () => {
    if (!releaseLeaseId || releaseLeaseBusy) {
      return;
    }
    setReleaseLeaseBusy(true);
    try {
      const released = await props.onReleaseFileLease(releaseLeaseId);
      if (released) {
        setReleaseLeaseId(null);
      }
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      setReleaseLeaseBusy(false);
    }
  };

  const handleSendRoomMessage = async () => {
    if (!props.selectedRoomThreadId || sending) {
      return;
    }
    setSending(true);
    try {
      await props.onSendRoomMessage();
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      setSending(false);
    }
  };

  const clearFilters = () => {
    setRoomsPage(1);
    props.onMailboxFilterChange("all");
    props.onMailPrincipalOverrideChange("");
    props.onMailSearchChange("");
  };

  return (
    <section className="mc-mail-grid mc-mail-grid-2col">
      {/* ── Room sidebar ── */}
      <article className="mc-surface mc-mail-sidebar">
        <header className="mc-surface-header">
          <h2>Rooms</h2>
          <button type="button" onClick={() => setCreateRoomOpen(true)}>
            + New Room
          </button>
        </header>
        <div className="mc-mail-thread-list">
          {visibleRooms.map((thread) => (
            <button
              type="button"
              key={thread.thread_id}
              className={clsx(
                "mc-mail-thread-item",
                props.selectedRoomThreadId === thread.thread_id && "active"
              )}
              onClick={() => {
                setRoomMsgsPage(1);
                props.onSelectRoomThread(thread.thread_id);
              }}
            >
              <div className="mc-mail-thread-head">
                <strong>{thread.subject}</strong>
                <span className="chip">{thread.participant_count} members</span>
              </div>
              <p>{thread.latest_message_preview ?? "No room messages yet."}</p>
              <small title={formatDateTime(thread.latest_message_at)}>{formatRelative(thread.latest_message_at)}</small>
            </button>
          ))}
          {visibleRooms.length === 0 ? (
            hasActiveFilters ? (
              <div className="mc-empty-drawer mc-empty-drawer-stack">
                <span>No rooms match your current mail filters.</span>
                <button type="button" className="ghost" onClick={clearFilters}>
                  Clear filters
                </button>
              </div>
            ) : (
              <div className="mc-empty-drawer">No rooms found yet.</div>
            )
          ) : null}
        </div>
        <Pagination currentPage={roomsPage} totalPages={roomsPagination.totalPages} onPageChange={setRoomsPage} />
      </article>

      {/* ── Conversation + compose ── */}
      <article className="mc-surface mc-mail-thread-view">
        <header className="mc-surface-header">
          <h2>{props.roomThreadDetail?.thread.subject ?? "Select a room"}</h2>
          <button
            type="button"
            disabled={!hasSelectedRoom}
            onClick={() => setModerationOpen(true)}
          >
            Room Settings
          </button>
        </header>
        <div className="mc-mail-message-stream">
          <Pagination currentPage={roomMsgsPage} totalPages={roomMsgsPagination.totalPages} onPageChange={setRoomMsgsPage} />
          {visibleRoomMsgs.map((message) => (
            <article key={message.message_id} className="mc-mail-message">
              <div className="mc-mail-message-head">
                <div>
                  <Avatar name={message.sender_principal} />
                  <strong>{message.sender_principal}</strong>
                  <span title={formatDateTime(message.created_at)}>{formatRelative(message.created_at)}</span>
                </div>
                <button
                  type="button"
                  disabled={isBusyAction(`ack:${message.message_id}`)}
                  onClick={() =>
                    runBusyAction(`ack:${message.message_id}`, () =>
                      props.onAcknowledgeMessage(
                        message.message_id,
                        props.mailPrincipalOverride || undefined
                      )
                    )
                  }
                >
                  {isBusyAction(`ack:${message.message_id}`) ? "Acknowledging\u2026" : "Acknowledge"}
                </button>
              </div>
              <pre>{message.body_text}</pre>
              {message.attachments.length > 0 ? (
                <div className="mc-mail-attachment-row">
                  {message.attachments.map((attachment) => {
                    const downloadKey = `download:${message.message_id}:${attachment.attachment_id}`;
                    return (
                      <button
                        type="button"
                        key={attachment.attachment_id}
                        disabled={isBusyAction(downloadKey)}
                        onClick={() =>
                          runBusyAction(downloadKey, () =>
                            props.onDownloadAttachment(
                              message.message_id,
                              attachment.attachment_id,
                              attachment.filename
                            )
                          )
                        }
                      >
                        {isBusyAction(downloadKey) ? "Downloading..." : attachment.filename}
                      </button>
                    );
                  })}
                </div>
              ) : null}
            </article>
          ))}
          {visibleRoomMsgs.length === 0 ? (
            <div className="mc-empty-drawer">No messages in this room yet.</div>
          ) : null}
        </div>
        {/* ── Inline compose (3 controls at rest) ── */}
        <div className="mc-mail-compose mc-mail-compose-inline">
          <textarea
            value={props.chatComposeBody}
            onChange={(event) => props.onChatComposeBodyChange(event.target.value)}
            placeholder="Type to room..."
            rows={3}
          />
          <div className="mc-inline-actions">
            <div className="mc-emoji-picker-wrap" ref={emojiRef}>
              <button
                type="button"
                disabled={!hasSelectedRoom}
                onClick={() => setEmojiPickerOpen(!emojiPickerOpen)}
                title="React"
              >
                <SmilePlus size={16} />
              </button>
              {emojiPickerOpen ? (
                <div className="mc-emoji-picker">
                  {REACTION_EMOJI.map((emoji) => (
                    <button
                      key={emoji.code}
                      type="button"
                      className="mc-emoji-btn"
                      title={emoji.code}
                      disabled={isBusyAction(`reaction:${emoji.code}`)}
                      onClick={() => {
                        runBusyAction(`reaction:${emoji.code}`, async () => {
                          await props.onPostRoomReaction(emoji.code);
                          setEmojiPickerOpen(false);
                        });
                      }}
                    >
                      {emoji.display}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
            <button
              type="button"
              className={chatOptionsOpen ? "mc-options-active" : "ghost"}
              onClick={() => setChatOptionsOpen(!chatOptionsOpen)}
            >
              Options
            </button>
            <button
              type="button"
              onClick={() => void handleSendRoomMessage()}
              disabled={!props.selectedRoomThreadId || sending}
            >
              {sending ? "Sending..." : "Send"}
            </button>
          </div>
          {chatOptionsOpen ? (
            <div className="mc-mail-compose-options">
              <label className="upload-pill">
                <input
                  type="file"
                  multiple
                  onChange={(event) => {
                    props.onChatAttachmentFilesChange(Array.from(event.target.files ?? []));
                    event.currentTarget.value = "";
                  }}
                />
                Attach ({props.chatAttachmentFiles.length})
              </label>
              <button type="button" className="ghost" onClick={props.onRefresh}>
                Refresh
              </button>
              <label>
                Sender
                <select
                  value={props.chatComposeSender}
                  onChange={(event) => props.onChatComposeSenderChange(event.target.value)}
                >
                  <option value="">default</option>
                  {props.agents.map((agent) => (
                    <option key={agent.agent_id} value={agent.agent_id}>
                      {agent.name || agent.agent_id}
                    </option>
                  ))}
                </select>
              </label>
              <AgentPicker
                label="Additional recipients (optional)"
                agents={props.agents}
                value={props.chatComposeRecipients}
                onChange={props.onChatComposeRecipientsChange}
              />
            </div>
          ) : null}
        </div>
      </article>

      {/* ── Create room modal ── */}
      <Modal
        open={createRoomOpen}
        onClose={() => setCreateRoomOpen(false)}
        title="Create Room"
        subtitle="Start a new chatroom for multi-agent coordination"
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setCreateRoomOpen(false)}>
              Cancel
            </button>
            <button type="button" disabled={createRoomBusy} onClick={() => void handleCreateRoom()}>
              {createRoomBusy ? "Creating..." : "Create Room"}
            </button>
          </>
        }
      >
        <label className="mc-modal-field">
          Room Name
          <input
            value={props.newRoomName}
            onChange={(event) => props.onNewRoomNameChange(event.target.value)}
            placeholder="room name"
            autoFocus
          />
        </label>
        <AgentPicker
          label="Participants"
          agents={props.agents}
          value={props.newRoomParticipants}
          onChange={props.onNewRoomParticipantsChange}
        />
      </Modal>

      {/* ── Room moderation modal ── */}
      <Modal
        open={moderationOpen}
        onClose={() => setModerationOpen(false)}
        title="Room Settings"
        subtitle={props.roomThreadDetail?.thread.subject ?? ""}
        width="600px"
      >
        <section className="mc-modal-section">
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
        <section className="mc-modal-section">
          <h3>Moderation Actions</h3>
          <div className="mc-inline-actions">
            <button
              type="button"
              disabled={!hasSelectedRoom || isBusyAction("mod:ack-all-unread")}
              onClick={() =>
                runBusyAction("mod:ack-all-unread", () => props.onAcknowledgeRoomUnread())
              }
            >
              {isBusyAction("mod:ack-all-unread") ? "Working\u2026" : "Acknowledge All Unread"}
            </button>
            <button
              type="button"
              disabled={!hasSelectedRoom || isBusyAction("mod:reserve-workspace")}
              onClick={() =>
                runBusyAction("mod:reserve-workspace", () => props.onReserveSelectedRoomWorkspace())
              }
              title="Claim an exclusive file-lock workspace for this room"
            >
              {isBusyAction("mod:reserve-workspace") ? "Working\u2026" : "Reserve Workspace"}
            </button>
          </div>
        </section>
        <section className="mc-modal-section">
          <h3>Active Leases</h3>
          <ul className="mc-mail-lease-list">
            {props.leases.map((lease) => (
              <li key={lease.lease_id}>
                <div>
                  <strong>{lease.glob_pattern}</strong>
                  <p>{lease.exclusive ? "exclusive" : "shared"}</p>
                </div>
                <button
                  type="button"
                  className="danger"
                  onClick={() => setReleaseLeaseId(lease.lease_id)}
                >
                  Release
                </button>
              </li>
            ))}
            {props.leases.length === 0 ? <li>No active leases.</li> : null}
          </ul>
        </section>
      </Modal>

      {/* ── Release lease confirmation ── */}
      <Modal
        open={releaseLeaseId !== null}
        onClose={() => setReleaseLeaseId(null)}
        title="Release Lease?"
        subtitle="This will release the advisory file lock immediately."
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setReleaseLeaseId(null)}>
              Cancel
            </button>
            <button
              type="button"
              className="danger"
              disabled={releaseLeaseBusy}
              onClick={() => void handleReleaseLease()}
            >
              {releaseLeaseBusy ? "Releasing..." : "Release"}
            </button>
          </>
        }
      >
        <p>
          Are you sure you want to release the lease on{" "}
          <strong>{props.leases.find((l) => l.lease_id === releaseLeaseId)?.glob_pattern ?? "this file"}</strong>?
          Other agents may begin writing to these paths.
        </p>
      </Modal>
    </section>
  );
}
