import { useMemo, useRef, useState } from "react";
import clsx from "clsx";
import type {
  Agent,
  AgentMailFileLeaseResponse,
  AgentMailMessageResponse,
  AgentMailThreadDetailResponse,
  AgentMailThreadSummaryResponse,
} from "../../types";
import { formatDateTime, formatRelative } from "../../utils/datetime";
import { formatBytes } from "../../utils/files";
import { AgentPicker } from "../../ui/AgentPicker";
import { Avatar } from "../../ui/Avatar";
import { Pagination } from "../../ui/Pagination";
import { ScrollRegion } from "../../ui/ScrollRegion";
import { Tabs } from "../../ui/Tabs";
import { Modal } from "../../ui/Modal";
import { usePagination } from "../../ui/usePagination";
import { PinRoomToOffice } from "../execassOffice/PinRoomToOffice";
import { PeopleRoutingSection } from "../peopleRouting/PeopleRoutingSection";
import type { PeopleRoutingController } from "../peopleRouting/usePeopleRoutingController";

const THREADS_PAGE_SIZE = 8;
const LEASES_PAGE_SIZE = 6;

const TTL_PRESETS = [
  { label: "5m", ms: "300000" },
  { label: "15m", ms: "900000" },
  { label: "1h", ms: "3600000" },
  { label: "4h", ms: "14400000" },
  { label: "24h", ms: "86400000" },
];

const GLOB_PRESETS = [
  { label: "All files", value: "**/*" },
  { label: "Source code", value: "src/**/*" },
  { label: "Config files", value: "*.{json,yaml,toml}" },
  { label: "Docs", value: "docs/**/*" },
  { label: "Custom", value: "" },
];
const CUSTOM_PRINCIPAL_VALUE = "__custom__";

type MailSection = "frontdesk" | "messages" | "leases";

/**
 * Landing section per stable room id. Basement "directory" owns the mail
 * route and lands on Front Desk; "locks" shares the route and lands on the
 * advisory lease machinery. No route-name or display-label guesses.
 */
const ROOM_LANDING_SECTION: Readonly<Partial<Record<string, MailSection>>> = {
  directory: "frontdesk",
  locks: "leases",
};

/** Plain owner-readable TTL, e.g. 900000 -> "15m", 3600000 -> "1h". */
function formatTtl(ttlMs: number): string {
  if (ttlMs >= 3_600_000 && ttlMs % 3_600_000 === 0) {
    return `${ttlMs / 3_600_000}h`;
  }
  if (ttlMs >= 60_000) {
    return `${Math.round(ttlMs / 60_000)}m`;
  }
  return `${Math.round(ttlMs / 1000)}s`;
}

interface MailPageProps {
  onRefresh: () => void;
  agents: Agent[];
  /**
   * The resolved stable room id from the elevator. Entering "directory"
   * lands on Front Desk; internal section clicks and unrelated rerenders
   * must never reset the user's selection.
   */
  activeRoomId: string | null;
  /** The single shared People & Routing authority (no second fetch loop). */
  peopleRouting: PeopleRoutingController;
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
  onCreateDirectThread: () => Promise<boolean>;
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
  onCreateFileLease: () => Promise<boolean>;
  leases: AgentMailFileLeaseResponse[];
  /** True while the lease read is in flight. */
  leasesLoading: boolean;
  /** True only after a lease read committed; distinguishes loaded-empty. */
  leasesLoaded: boolean;
  /** The newest lease read failure, or null when the last read committed. */
  leasesError: string | null;
  onRetryLeases: () => Promise<void> | void;
  onReleaseFileLease: (leaseId: string) => Promise<boolean>;
}

export function MailPage(props: MailPageProps) {
  const [subTab, setSubTab] = useState<MailSection>(
    () =>
      (props.activeRoomId ? ROOM_LANDING_SECTION[props.activeRoomId] : undefined) ??
      "messages",
  );
  // Only an actual room change may force a landing section; internal section
  // clicks and unrelated rerenders must never reset the user's selection.
  const [lastRoomId, setLastRoomId] = useState(props.activeRoomId);
  if (props.activeRoomId !== lastRoomId) {
    setLastRoomId(props.activeRoomId);
    const landing = props.activeRoomId
      ? ROOM_LANDING_SECTION[props.activeRoomId]
      : undefined;
    if (landing) {
      setSubTab(landing);
    }
  }
  const [createThreadOpen, setCreateThreadOpen] = useState(false);
  const [releaseLeaseId, setReleaseLeaseId] = useState<string | null>(null);
  const [composeOptionsOpen, setComposeOptionsOpen] = useState(false);
  const [mobileListOpen, setMobileListOpen] = useState(true);
  const [createLeaseOpen, setCreateLeaseOpen] = useState(false);
  const [useCustomPrincipal, setUseCustomPrincipal] = useState(false);
  const [threadsPage, setThreadsPage] = useState(1);
  const [leasesPage, setLeasesPage] = useState(1);
  const [sending, setSending] = useState(false);
  const [createThreadBusy, setCreateThreadBusy] = useState(false);
  const [createLeaseBusy, setCreateLeaseBusy] = useState(false);
  const [releaseLeaseBusy, setReleaseLeaseBusy] = useState(false);
  const [busyActions, setBusyActions] = useState<Set<string>>(new Set());
  const busyActionRef = useRef<Set<string>>(new Set());
  // React render state is not a same-tick lock: two clicks in one tick both
  // read the stale "not busy" state. These refs are the authoritative locks;
  // the matching state only drives labels/disabled styling.
  const sendLockRef = useRef(false);
  const createThreadLockRef = useRef(false);
  const createLeaseLockRef = useRef(false);
  const releaseLeaseLockRef = useRef(false);

  const handleSend = async () => {
    if (sendLockRef.current) {
      return;
    }
    sendLockRef.current = true;
    setSending(true);
    try {
      await props.onSendMessage();
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      sendLockRef.current = false;
      setSending(false);
    }
  };

  const threadsPagination = usePagination(props.mailThreads, THREADS_PAGE_SIZE);
  const leasesPagination = usePagination(props.leases, LEASES_PAGE_SIZE);

  // Keep stored pages aligned with the rendered clamp. Rendering only through
  // getPage's clamp leaves a stale later page behind, which resurfaces when
  // the list shrinks and then grows again.
  const clampedThreadsPage = threadsPagination.clampPage(threadsPage);
  if (clampedThreadsPage !== threadsPage) {
    setThreadsPage(clampedThreadsPage);
  }
  const clampedLeasesPage = leasesPagination.clampPage(leasesPage);
  if (clampedLeasesPage !== leasesPage) {
    setLeasesPage(clampedLeasesPage);
  }

  const visibleThreads = threadsPagination.getPage(clampedThreadsPage);
  const visibleLeases = leasesPagination.getPage(clampedLeasesPage);

  const runBusyAction = (key: string, fn: () => Promise<unknown>) => {
    if (busyActionRef.current.has(key)) {
      return;
    }
    busyActionRef.current.add(key);
    setBusyActions(new Set(busyActionRef.current));
    void fn()
      .catch((error: unknown) => {
        console.error("mail action failed", { key, error });
      })
      .finally(() => {
        busyActionRef.current.delete(key);
        setBusyActions(new Set(busyActionRef.current));
      });
  };

  const isBusyAction = (key: string) => busyActions.has(key);

  const handleMailboxFilterChange = (raw: string) => {
    if (raw === "all" || raw === "inbox" || raw === "outbox") {
      setThreadsPage(1);
      props.onMailboxFilterChange(raw);
    }
  };

  const handleCreateThread = async () => {
    if (createThreadLockRef.current) {
      return;
    }
    createThreadLockRef.current = true;
    setCreateThreadBusy(true);
    try {
      const created = await props.onCreateDirectThread();
      if (created) {
        setCreateThreadOpen(false);
      }
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      createThreadLockRef.current = false;
      setCreateThreadBusy(false);
    }
  };

  const handleReleaseLease = async () => {
    if (!releaseLeaseId || releaseLeaseLockRef.current) {
      return;
    }
    releaseLeaseLockRef.current = true;
    setReleaseLeaseBusy(true);
    try {
      const released = await props.onReleaseFileLease(releaseLeaseId);
      if (released) {
        setReleaseLeaseId(null);
      }
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      releaseLeaseLockRef.current = false;
      setReleaseLeaseBusy(false);
    }
  };

  const handleCreateLease = async () => {
    if (createLeaseLockRef.current) {
      return;
    }
    createLeaseLockRef.current = true;
    setCreateLeaseBusy(true);
    try {
      const created = await props.onCreateFileLease();
      if (created) {
        setCreateLeaseOpen(false);
      }
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      createLeaseLockRef.current = false;
      setCreateLeaseBusy(false);
    }
  };

  const principalIsKnown = useMemo(
    () =>
      props.mailPrincipalOverride === "" ||
      props.agents.some((agent) => agent.agent_id === props.mailPrincipalOverride),
    [props.agents, props.mailPrincipalOverride]
  );

  const principalSelectValue = useCustomPrincipal
    || !principalIsKnown
      ? CUSTOM_PRINCIPAL_VALUE
      : props.mailPrincipalOverride;
  const hasActiveFilters =
    props.mailboxFilter !== "all" ||
    props.mailPrincipalOverride.trim().length > 0 ||
    props.mailSearch.trim().length > 0;

  const clearFilters = () => {
    setUseCustomPrincipal(false);
    setThreadsPage(1);
    props.onMailboxFilterChange("all");
    props.onMailPrincipalOverrideChange("");
    props.onMailSearchChange("");
  };

  return (
    <section className="mc-mail-page" data-testid="mail-page">
      <Tabs
        tabs={[
          { id: "frontdesk", label: "Front Desk" },
          { id: "messages", label: "Messages", count: props.mailThreads.length },
          { id: "leases", label: "File locks", count: props.leases.length },
        ]}
        activeTab={subTab}
        onTabChange={(id) => setSubTab(id as MailSection)}
      />

      {subTab === "frontdesk" ? (
        <PeopleRoutingSection
          controller={props.peopleRouting}
          agents={props.agents}
          pin={
            props.activeRoomId === "directory" ? (
              <PinRoomToOffice roomId="directory" />
            ) : null
          }
        />
      ) : null}

      {subTab === "messages" ? (
        <div className={clsx("mc-mail-grid mc-mail-grid-2col", mobileListOpen ? "mc-mobile-list-open" : "mc-mobile-detail-open")}>
          {/* ── Thread sidebar ── */}
          <article className="mc-surface mc-mail-sidebar">
            <header className="mc-surface-header">
              <h2>Threads</h2>
              <div className="mc-inline-actions">
                <button type="button" onClick={() => setCreateThreadOpen(true)}>
                  + New Thread
                </button>
                <button type="button" onClick={props.onRefresh}>
                  Refresh
                </button>
              </div>
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
                Acting as
                <select
                  value={principalSelectValue}
                  onChange={(event) => {
                    const next = event.target.value;
                    if (next === CUSTOM_PRINCIPAL_VALUE) {
                      setUseCustomPrincipal(true);
                      if (principalIsKnown) {
                        props.onMailPrincipalOverrideChange("");
                      }
                      return;
                    }
                    setUseCustomPrincipal(false);
                    setThreadsPage(1);
                    props.onMailPrincipalOverrideChange(next);
                  }}
                >
                  <option value="">none (default)</option>
                  {props.agents.map((agent) => (
                    <option key={agent.agent_id} value={agent.agent_id}>
                      {agent.name || agent.agent_id}
                    </option>
                  ))}
                  <option value={CUSTOM_PRINCIPAL_VALUE}>Custom...</option>
                </select>
                {useCustomPrincipal || !principalIsKnown ? (
                  <input
                    value={props.mailPrincipalOverride}
                    onChange={(event) => {
                      setThreadsPage(1);
                      props.onMailPrincipalOverrideChange(event.target.value);
                    }}
                    placeholder="custom acting-as id"
                  />
                ) : null}
              </label>
              <label>
                Search
                <input
                  value={props.mailSearch}
                  onChange={(event) => {
                    setThreadsPage(1);
                    props.onMailSearchChange(event.target.value);
                  }}
                  placeholder="subject/body..."
                />
              </label>
            </div>
            <div className="mc-mail-thread-list">
              {visibleThreads.map((thread) => (
                <button
                  type="button"
                  key={thread.thread_id}
                  className={clsx(
                    "mc-mail-thread-item",
                    props.selectedMailThreadId === thread.thread_id && "active"
                  )}
                  onClick={() => {
                    setMobileListOpen(false);
                    props.onSelectMailThread(thread.thread_id);
                  }}
                >
                  <div className="mc-mail-thread-head">
                    <strong>{thread.subject}</strong>
                    {thread.unread_count > 0 ? (
                      <span className="chip chip-error">{thread.unread_count} unread</span>
                    ) : null}
                  </div>
                  <p>{thread.latest_message_preview ?? "No messages yet."}</p>
                  <small>
                    {thread.latest_sender_principal ?? "n/a"} • <span title={formatDateTime(thread.latest_message_at)}>{formatRelative(thread.latest_message_at)}</span>
                  </small>
                </button>
              ))}
              {visibleThreads.length === 0 ? (
                hasActiveFilters ? (
                  <div className="mc-empty-drawer mc-empty-drawer-stack">
                    <span>No direct threads match your current filters.</span>
                    <button type="button" className="ghost" onClick={clearFilters}>
                      Clear filters
                    </button>
                  </div>
                ) : (
                  <div className="mc-empty-drawer">
                    No direct threads yet. Start one with New Thread.
                  </div>
                )
              ) : null}
            </div>
            <Pagination currentPage={clampedThreadsPage} totalPages={threadsPagination.totalPages} onPageChange={setThreadsPage} />
          </article>

          {/* ── Conversation + inline compose ── */}
          <article className="mc-surface mc-mail-thread-view">
            <header className="mc-surface-header">
              <button type="button" className="mc-mobile-back-button ghost" onClick={() => setMobileListOpen(true)}>
                Back to threads
              </button>
              <h2>{props.mailThreadDetail?.thread.subject ?? "Select a thread"}</h2>
              <div className="mc-inline-actions">
                <span className="mc-msg-count">{props.mailMessages.length} message(s)</span>
                <button
                  type="button"
                  disabled={isBusyAction("summarize:mail-thread")}
                  onClick={() =>
                    runBusyAction("summarize:mail-thread", () => props.onSummarizeToNote())
                  }
                >
                  {isBusyAction("summarize:mail-thread") ? "Working..." : "Summarize"}
                </button>
              </div>
            </header>
            <ScrollRegion aria-label="Direct message history" className="mc-mail-message-stream">
              {props.mailMessages.map((message) => {
                const ackKey = `ack:${message.message_id}`;
                return (
                <article key={message.message_id} className="mc-mail-message">
                  <div className="mc-mail-message-head">
                    <div>
                      <Avatar name={message.sender_principal} />
                      <strong>{message.sender_principal}</strong>
                      <span title={formatDateTime(message.created_at)}>{formatRelative(message.created_at)}</span>
                    </div>
                    <button
                      type="button"
                      disabled={isBusyAction(ackKey)}
                      onClick={() =>
                        runBusyAction(ackKey, () =>
                          props.onAcknowledgeMessage(
                            message.message_id,
                            props.mailPrincipalOverride || undefined
                          )
                        )
                      }
                    >
                      {isBusyAction(ackKey) ? "Acknowledging\u2026" : "Acknowledge"}
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
                      /{message.recipients.length} acknowledged
                    </span>
                  </div>
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
                            {isBusyAction(downloadKey) ? "Downloading..." : `${attachment.filename} (${formatBytes(attachment.bytes)})`}
                          </button>
                        );
                      })}
                    </div>
                  ) : null}
                </article>
                );
              })}
              {props.mailMessages.length === 0 ? (
                <div className="mc-empty-drawer">No messages in this thread yet.</div>
              ) : null}
            </ScrollRegion>
            {/* ── Inline compose (3 controls at rest) ── */}
            <div className="mc-mail-compose mc-mail-compose-inline">
              <textarea
                value={props.mailComposeBody}
                onChange={(event) => props.onMailComposeBodyChange(event.target.value)}
                placeholder="Write a clear handoff message..."
                rows={3}
              />
              <div className="mc-inline-actions">
                <label className="upload-pill">
                  <input
                    type="file"
                    multiple
                    onChange={(event) => {
                      props.onMailAttachmentFilesChange(Array.from(event.target.files ?? []));
                      event.currentTarget.value = "";
                    }}
                  />
                  Attach ({props.mailAttachmentFiles.length})
                </label>
                <button
                  type="button"
                  className={composeOptionsOpen ? "mc-options-active" : "ghost"}
                  onClick={() => setComposeOptionsOpen(!composeOptionsOpen)}
                  aria-expanded={composeOptionsOpen}
                >
                  Options
                </button>
                <button
                  type="button"
                  className="primary"
                  onClick={() => void handleSend()}
                  disabled={!props.selectedMailThreadId || sending}
                >
                  {sending ? "Sending..." : "Send"}
                </button>
              </div>
              {composeOptionsOpen ? (
                <div className="mc-mail-compose-options">
                  <label>
                    Sender
                    <select
                      value={props.mailComposeSender}
                      onChange={(event) => props.onMailComposeSenderChange(event.target.value)}
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
                    label="Recipients (blank = thread)"
                    agents={props.agents}
                    value={props.mailComposeRecipients}
                    onChange={props.onMailComposeRecipientsChange}
                  />
                </div>
              ) : null}
            </div>
          </article>
        </div>
      ) : null}

      {subTab === "leases" ? (
        /* ── File locks tab ── */
        <div className="mc-lease-page">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Advisory file locks</h2>
              {props.leasesLoaded ? (
                <p>{props.leases.length} active file lock(s)</p>
              ) : null}
              {props.activeRoomId === "locks" ? (
                <PinRoomToOffice roomId="locks" />
              ) : null}
            </header>
            <p className="mc-lease-honesty">
              Cooperative advisory reservations agents coordinate through
              Agent Mail. Nothing is enforced at the filesystem level — a
              process that does not check the list can still write to these
              paths.
            </p>
            <button type="button" onClick={() => setCreateLeaseOpen(true)}>
              + New file lock
            </button>
            {props.leasesError ? (
              <div className="mc-empty-drawer mc-empty-drawer-stack" role="alert">
                <span>File locks couldn't be loaded: {props.leasesError}</span>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => void props.onRetryLeases()}
                >
                  Retry
                </button>
              </div>
            ) : null}
            {!props.leasesError && props.leasesLoading && !props.leasesLoaded ? (
              <div className="mc-empty-drawer">Loading file locks...</div>
            ) : null}
            {!props.leasesError && !props.leasesLoading && !props.leasesLoaded ? (
              <div className="mc-empty-drawer">
                File locks haven't loaded yet. Connect the gateway to see live
                reservations.
              </div>
            ) : null}
            {props.leasesLoaded ? (
              <>
                <ul className="mc-mail-lease-list">
                  {visibleLeases.map((lease) => (
                    <li key={lease.lease_id}>
                      <div>
                        <strong>{lease.glob_pattern}</strong>
                        <p>
                          {lease.holder_principal} •{" "}
                          {lease.exclusive ? "exclusive" : "shared"} •{" "}
                          {formatTtl(lease.ttl_ms)} TTL • expires{" "}
                          <span title={formatDateTime(lease.expires_at)}>
                            {formatRelative(lease.expires_at)}
                          </span>
                        </p>
                        {lease.note ? (
                          <p className="mc-lease-note">{lease.note}</p>
                        ) : null}
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
                  {visibleLeases.length === 0 ? (
                    <li>No active file locks.</li>
                  ) : null}
                </ul>
                <Pagination currentPage={clampedLeasesPage} totalPages={leasesPagination.totalPages} onPageChange={setLeasesPage} />
              </>
            ) : null}
          </article>
        </div>
      ) : null}

      {/* ── Create thread modal ── */}
      <Modal
        open={createThreadOpen}
        onClose={() => setCreateThreadOpen(false)}
        title="New Direct Thread"
        subtitle="Start a new mail conversation"
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setCreateThreadOpen(false)}>
              Cancel
            </button>
            <button type="button" className="primary" disabled={createThreadBusy} onClick={() => void handleCreateThread()}>
              {createThreadBusy ? "Creating..." : "Create Thread"}
            </button>
          </>
        }
      >
        <label className="mc-modal-field">
          Subject
          <input
            value={props.newMailThreadSubject}
            onChange={(event) => props.onNewMailThreadSubjectChange(event.target.value)}
            placeholder="Thread subject"
            autoFocus
          />
        </label>
        <AgentPicker
          label="Participants"
          agents={props.agents}
          value={props.newMailThreadParticipants}
          onChange={props.onNewMailThreadParticipantsChange}
        />
      </Modal>

      {/* ── Release lease confirmation ── */}
      <Modal
        open={releaseLeaseId !== null}
        onClose={() => setReleaseLeaseId(null)}
        title="Release file lock?"
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
          Are you sure you want to release the file lock on{" "}
          <strong>{props.leases.find((l) => l.lease_id === releaseLeaseId)?.glob_pattern ?? "this file"}</strong>?
          Other agents may begin writing to these paths.
        </p>
      </Modal>

      {/* ── New file lock modal ── */}
      <Modal
        open={createLeaseOpen}
        onClose={() => setCreateLeaseOpen(false)}
        title="Reserve file lock"
        subtitle="Create an advisory file lock for agent coordination."
        footer={
          <>
            <button type="button" className="ghost" onClick={() => setCreateLeaseOpen(false)}>
              Cancel
            </button>
            <button
              type="button"
              disabled={createLeaseBusy}
              onClick={() => void handleCreateLease()}
            >
              {createLeaseBusy ? "Reserving..." : "Reserve file lock"}
            </button>
          </>
        }
      >
        <label className="mc-modal-field">
          Acting as
          <select
            value={props.leaseHolderPrincipal}
            onChange={(event) => props.onLeaseHolderPrincipalChange(event.target.value)}
          >
            <option value="">none (optional)</option>
            {props.agents.map((agent) => (
              <option key={agent.agent_id} value={agent.agent_id}>
                {agent.name || agent.agent_id}
              </option>
            ))}
          </select>
        </label>
        <label className="mc-modal-field">
          Glob Pattern
          <select
            value={GLOB_PRESETS.some((p) => p.value === props.leaseGlobPattern) ? props.leaseGlobPattern : ""}
            onChange={(event) => props.onLeaseGlobPatternChange(event.target.value)}
          >
            {GLOB_PRESETS.map((preset) => (
              <option key={preset.label} value={preset.value}>
                {preset.label}{preset.value ? ` (${preset.value})` : ""}
              </option>
            ))}
          </select>
          {(!GLOB_PRESETS.some((p) => p.value === props.leaseGlobPattern) || props.leaseGlobPattern === "") ? (
            <input
              value={props.leaseGlobPattern}
              onChange={(event) => props.onLeaseGlobPatternChange(event.target.value)}
              placeholder="custom glob pattern"
            />
          ) : null}
        </label>
        <label className="mc-modal-field">
          Lock duration
          <div className="mc-ttl-presets">
            {TTL_PRESETS.map((preset) => (
              <button
                key={preset.ms}
                type="button"
                className={clsx("mc-ttl-preset", props.leaseTtlMs === preset.ms && "mc-ttl-preset-active")}
                onClick={() => props.onLeaseTtlMsChange(preset.ms)}
              >
                {preset.label}
              </button>
            ))}
          </div>
        </label>
        <div className="mc-field-grid">
          <label className="mc-modal-field">
            Note
            <input
              value={props.leaseNote}
              onChange={(event) => props.onLeaseNoteChange(event.target.value)}
              placeholder="optional"
            />
          </label>
          <label className="mc-checkbox">
            <input
              type="checkbox"
              checked={props.leaseExclusive}
              onChange={(event) => props.onLeaseExclusiveChange(event.target.checked)}
            />
            Exclusive
          </label>
        </div>
      </Modal>
    </section>
  );
}
