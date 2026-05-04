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
import { Tabs } from "../../ui/Tabs";
import { Modal } from "../../ui/Modal";
import { usePagination } from "../../ui/usePagination";

const THREADS_PAGE_SIZE = 8;
const MESSAGES_PAGE_SIZE = 10;
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

interface MailPageProps {
  onRefresh: () => void;
  agents: Agent[];
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
  onReleaseFileLease: (leaseId: string) => Promise<boolean>;
}

export function MailPage(props: MailPageProps) {
  const [subTab, setSubTab] = useState<"messages" | "leases">("messages");
  const [createThreadOpen, setCreateThreadOpen] = useState(false);
  const [releaseLeaseId, setReleaseLeaseId] = useState<string | null>(null);
  const [composeOptionsOpen, setComposeOptionsOpen] = useState(false);
  const [createLeaseOpen, setCreateLeaseOpen] = useState(false);
  const [useCustomPrincipal, setUseCustomPrincipal] = useState(false);
  const [threadsPage, setThreadsPage] = useState(1);
  const [messagesPage, setMessagesPage] = useState(1);
  const [leasesPage, setLeasesPage] = useState(1);
  const [sending, setSending] = useState(false);
  const [createThreadBusy, setCreateThreadBusy] = useState(false);
  const [createLeaseBusy, setCreateLeaseBusy] = useState(false);
  const [releaseLeaseBusy, setReleaseLeaseBusy] = useState(false);
  const [busyActions, setBusyActions] = useState<Set<string>>(new Set());
  const busyActionRef = useRef<Set<string>>(new Set());

  const handleSend = async () => {
    if (sending) {
      return;
    }
    setSending(true);
    try {
      await props.onSendMessage();
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      setSending(false);
    }
  };

  const threadsPagination = usePagination(props.mailThreads, THREADS_PAGE_SIZE);
  const messagesPagination = usePagination(props.mailMessages, MESSAGES_PAGE_SIZE);
  const leasesPagination = usePagination(props.leases, LEASES_PAGE_SIZE);

  const visibleThreads = threadsPagination.getPage(threadsPage);
  const visibleMessages = messagesPagination.getPage(messagesPage);
  const visibleLeases = leasesPagination.getPage(leasesPage);

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
    if (createThreadBusy) {
      return;
    }
    setCreateThreadBusy(true);
    try {
      const created = await props.onCreateDirectThread();
      if (created) {
        setCreateThreadOpen(false);
      }
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
      setCreateThreadBusy(false);
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

  const handleCreateLease = async () => {
    if (createLeaseBusy) {
      return;
    }
    setCreateLeaseBusy(true);
    try {
      const created = await props.onCreateFileLease();
      if (created) {
        setCreateLeaseOpen(false);
      }
    } catch {
      // Upstream controller surfaces user-facing errors.
    } finally {
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
    <section className="mc-mail-page">
      <Tabs
        tabs={[
          { id: "messages", label: "Messages", count: props.mailThreads.length },
          { id: "leases", label: "Leases", count: props.leases.length },
        ]}
        activeTab={subTab}
        onTabChange={(id) => setSubTab(id as "messages" | "leases")}
      />

      {subTab === "messages" ? (
        <div className="mc-mail-grid mc-mail-grid-2col">
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
                Principal
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
                    placeholder="custom principal id"
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
                    setMessagesPage(1);
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
            <Pagination currentPage={threadsPage} totalPages={threadsPagination.totalPages} onPageChange={setThreadsPage} />
          </article>

          {/* ── Conversation + inline compose ── */}
          <article className="mc-surface mc-mail-thread-view">
            <header className="mc-surface-header">
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
            <div className="mc-mail-message-stream">
              <Pagination currentPage={messagesPage} totalPages={messagesPagination.totalPages} onPageChange={setMessagesPage} />
              {visibleMessages.map((message) => {
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
              {visibleMessages.length === 0 ? (
                <div className="mc-empty-drawer">No messages in this thread yet.</div>
              ) : null}
            </div>
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
      ) : (
        /* ── Leases tab ── */
        <div className="mc-lease-page">
          <article className="mc-surface">
            <header className="mc-surface-header">
              <h2>Advisory File Leases</h2>
              <p>{props.leases.length} active lease(s)</p>
            </header>
            <button type="button" onClick={() => setCreateLeaseOpen(true)}>
              + New Lease
            </button>
            <ul className="mc-mail-lease-list">
              {visibleLeases.map((lease) => (
                <li key={lease.lease_id}>
                  <div>
                    <strong>{lease.glob_pattern}</strong>
                    <p>
                      {lease.holder_principal} • expires <span title={formatDateTime(lease.expires_at)}>{formatRelative(lease.expires_at)}</span>
                      {lease.exclusive ? " • exclusive" : ""}
                    </p>
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
              {visibleLeases.length === 0 ? <li>No active leases.</li> : null}
            </ul>
            <Pagination currentPage={leasesPage} totalPages={leasesPagination.totalPages} onPageChange={setLeasesPage} />
          </article>
        </div>
      )}

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
            <button type="button" disabled={createThreadBusy} onClick={() => void handleCreateThread()}>
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

      {/* ── New Lease modal ── */}
      <Modal
        open={createLeaseOpen}
        onClose={() => setCreateLeaseOpen(false)}
        title="Reserve File Lease"
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
              {createLeaseBusy ? "Reserving..." : "Reserve Lease"}
            </button>
          </>
        }
      >
        <label className="mc-modal-field">
          Holder Principal
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
          TTL
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
