/**
 * The Window (3F): the Reef and Office Chatter. Everything here is a view
 * over coarse authoritative truth - crabs render only observed presence,
 * report cards deep-link to the authoritative detail, chatter is a
 * read-first projection of Agent Mail with quiet unreads. Nothing on this
 * floor invents activity, read-state, typing, or threading.
 */

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";

import {
  isExecassPresence,
  presenceFreshness,
  presenceTargetDestination,
  sortPresence,
} from "../../glass/window/presence";
import {
  formatChatterTime,
  groupChatterMessages,
  roomHasUnread,
  sortRoomsByActivity,
} from "../../glass/window/chatterView";
import type {
  FloorPresenceItem,
  FloorPresenceTarget,
} from "../../glass/window/types";
import { useGlassSurfaceTheme } from "../../glass/useGlassSurfaceTheme";
import type { GlassWindowController } from "./useGlassWindowController";

function useNarrowViewport(): boolean {
  const [narrow, setNarrow] = useState(
    () => window.matchMedia?.("(max-width: 700px)").matches ?? false,
  );
  useEffect(() => {
    const media = window.matchMedia?.("(max-width: 700px)");
    if (!media) return;
    const update = () => setNarrow(media.matches);
    media.addEventListener?.("change", update);
    return () => media.removeEventListener?.("change", update);
  }, []);
  return narrow;
}

function ReefCrab(props: {
  item: FloorPresenceItem;
  open: boolean;
  reportId: string;
  onToggle: (agentId: string) => void;
  buttonRef: (agentId: string, element: HTMLButtonElement | null) => void;
}) {
  const { item, open, reportId, onToggle, buttonRef } = props;
  const big = isExecassPresence(item);
  return (
    <button
      type="button"
      data-testid="reef-crab"
      className={`mc-reef-agent is-${item.mood}${big ? " is-execass" : ""}`}
      data-activity={item.activity}
      aria-label={`${item.display_name}'s report card`}
      aria-expanded={open}
      aria-controls={reportId}
      onClick={() => onToggle(item.agent_id)}
      ref={(element) => buttonRef(item.agent_id, element)}
    >
      <span className="mc-reef-crab" aria-hidden="true">
        🦀
      </span>
      <strong>{item.display_name}</strong>
      <span>{item.activity_label}</span>
    </button>
  );
}

function ReportCard(props: {
  item: FloorPresenceItem;
  reportId: string;
  nowMs: number;
  onClose: () => void;
  onOpenTarget?: (target: FloorPresenceTarget) => boolean;
}) {
  const { item, reportId, nowMs, onClose, onOpenTarget } = props;
  const [blocked, setBlocked] = useState(false);
  const cardRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    cardRef.current?.focus();
  }, []);
  const freshness = presenceFreshness(item.observed_at_ms, nowMs);
  const destination = presenceTargetDestination(item.target);
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
  };
  return (
    <div
      ref={cardRef}
      id={reportId}
      tabIndex={-1}
      className="mc-reef-report"
      data-testid="reef-report-card"
      role="group"
      aria-label={`${item.display_name}'s report card`}
      onKeyDown={onKeyDown}
    >
      <div className="mc-reef-report-head">
        <strong>{item.display_name}</strong>
        <span className={`mc-reef-mood is-${item.mood}`}>
          mood: {item.mood}
        </span>
        <button
          type="button"
          className="mc-reef-report-close"
          aria-label="Close report card"
          onClick={onClose}
        >
          ✕
        </button>
      </div>
      <p className="mc-reef-report-activity">{item.activity_label}</p>
      <p className={`mc-reef-report-observed is-${freshness.tone}`}>
        {freshness.label}
      </p>
      {destination && item.target && onOpenTarget ? (
        <>
          <button
            type="button"
            className="mc-reef-report-link"
            onClick={() => setBlocked(!onOpenTarget(item.target!))}
          >
            {destination.label}
          </button>
          {blocked ? (
            <p className="mc-reef-report-none" role="status">
              That room is switched off in Config.
            </p>
          ) : null}
        </>
      ) : (
        <p className="mc-reef-report-none">Nothing to open for this crab.</p>
      )}
    </div>
  );
}

export function GlassWindowPage(props: {
  controller: GlassWindowController;
  /** Returns true when the authoritative destination could be opened. */
  onOpenTarget?: (target: FloorPresenceTarget) => boolean;
}) {
  const { controller, onOpenTarget } = props;
  const [selectedRoomId, setSelectedRoomId] = useState<string | null>(null);
  const [openCrabId, setOpenCrabId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [nowMs, setNowMs] = useState(() => Date.now());
  const narrow = useNarrowViewport();
  const [reefExpanded, setReefExpanded] = useState(false);
  const crabRefs = useRef(new Map<string, HTMLButtonElement>());
  const floorRef = useRef<HTMLElement | null>(null);
  useGlassSurfaceTheme(floorRef);
  useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  const presenceItems = useMemo(
    () => sortPresence(controller.presence?.items ?? []),
    [controller.presence?.items],
  );
  const openCrab =
    presenceItems.find((item) => item.agent_id === openCrabId) ?? null;
  const unobservedCount = presenceItems.filter(
    (item) => item.observed_at_ms === null || item.mood === "unknown",
  ).length;

  const rooms = useMemo(
    () => sortRoomsByActivity(controller.chatter?.rooms ?? []),
    [controller.chatter?.rooms],
  );
  const activeRoomId =
    selectedRoomId && rooms.some((room) => room.thread_id === selectedRoomId)
      ? selectedRoomId
      : (rooms[0]?.thread_id ?? null);
  const messageGroups = useMemo(
    () =>
      groupChatterMessages(
        (controller.chatter?.messages ?? []).filter(
          (message) => message.thread_id === activeRoomId,
        ),
      ),
    [activeRoomId, controller.chatter?.messages],
  );

  const registerCrabRef = (agentId: string, element: HTMLButtonElement | null) => {
    if (element) crabRefs.current.set(agentId, element);
    else crabRefs.current.delete(agentId);
  };
  const toggleCrab = (agentId: string) => {
    setOpenCrabId((current) => (current === agentId ? null : agentId));
  };
  const closeCard = () => {
    const owner = openCrabId ? crabRefs.current.get(openCrabId) : null;
    setOpenCrabId(null);
    owner?.focus();
  };

  const reefBody = (
    <>
      <div className="mc-reef-water">
        {presenceItems.length === 0 ? (
          <div className="mc-window-empty">
            No authoritative presence observation yet.
          </div>
        ) : (
          presenceItems.map((item) => (
            <ReefCrab
              key={item.agent_id}
              item={item}
              open={item.agent_id === openCrabId}
              reportId={`reef-report-${encodeURIComponent(item.agent_id)}`}
              onToggle={toggleCrab}
              buttonRef={registerCrabRef}
            />
          ))
        )}
      </div>
      {openCrab ? (
        <ReportCard
          key={openCrab.agent_id}
          item={openCrab}
          reportId={`reef-report-${encodeURIComponent(openCrab.agent_id)}`}
          nowMs={nowMs}
          onClose={closeCard}
          onOpenTarget={onOpenTarget}
        />
      ) : null}
    </>
  );

  return (
    <section className="mc-window-floor" aria-label="The Window" ref={floorRef}>
      <header className="mc-window-header">
        <div>
          <span className="mc-window-kicker">3F · THE WINDOW</span>
          <h2>Reef &amp; Office Chatter</h2>
          <p>Coarse operational truth and deliberately safe working notes.</p>
        </div>
        <button
          type="button"
          className="ghost"
          disabled={controller.loading}
          onClick={() => void controller.refresh()}
        >
          {controller.loading ? "Observing…" : "Refresh"}
        </button>
      </header>

      {controller.error ? (
        <div className="mc-window-unavailable" role="status">
          <strong>Window unavailable</strong>
          <span>{controller.error}</span>
        </div>
      ) : null}

      <div className="mc-window-grid">
        <section className="mc-reef-panel" aria-label="Reef presence">
          <div className="mc-window-section-title">
            <div>
              <span>REEF</span>
              <h3>The crew, from a distance</h3>
            </div>
            <small>Never message content</small>
          </div>
          {narrow ? (
            <details
              className="mc-reef-collapse"
              open={reefExpanded}
              onToggle={(event) => setReefExpanded(event.currentTarget.open)}
            >
              <summary>
                {presenceItems.length} on the floor
                {unobservedCount > 0
                  ? ` · ${unobservedCount} without a recent observation`
                  : ""}
              </summary>
              {reefBody}
            </details>
          ) : (
            reefBody
          )}
        </section>

        <section className="mc-chatter-panel" aria-label="Office Chatter">
          <div className="mc-window-section-title">
            <div>
              <span>OFFICE CHATTER</span>
              <h3>Safe workstream notes</h3>
            </div>
            <small>Agent Mail</small>
          </div>
          <div className="mc-chatter-layout">
            <nav className="mc-chatter-rooms" aria-label="Chatter rooms">
              {rooms.length === 0 ? (
                <span className="mc-window-empty">No safe rooms yet.</span>
              ) : (
                rooms.map((room) => (
                  <button
                    type="button"
                    key={room.thread_id}
                    className={room.thread_id === activeRoomId ? "is-active" : ""}
                    onClick={() => setSelectedRoomId(room.thread_id)}
                  >
                    <span># {room.label}</span>
                    {roomHasUnread(room) ? (
                      <i
                        className="mc-chatter-dot"
                        data-testid="chatter-unread"
                        aria-label="unread notes"
                        role="img"
                      />
                    ) : null}
                  </button>
                ))
              )}
            </nav>
            <div className="mc-chatter-stream">
              <div className="mc-chatter-messages">
                {messageGroups.length === 0 ? (
                  <div className="mc-window-empty">
                    Nothing safe to overhear yet.
                  </div>
                ) : (
                  messageGroups.map((group) => (
                    <article key={group.messages[0]?.message_id}>
                      <div>
                        <strong>{group.author.display_name}</strong>
                        <time>
                          {formatChatterTime(group.startedAtMs, nowMs)}
                        </time>
                      </div>
                      {group.messages.map((message) => (
                        <p key={message.message_id}>{message.text}</p>
                      ))}
                    </article>
                  ))
                )}
              </div>
              <form
                className="mc-chatter-compose"
                onSubmit={(event: FormEvent) => {
                  event.preventDefault();
                  const submitted = draft.trim();
                  if (!activeRoomId || !submitted || controller.sending) return;
                  void controller
                    .sendMessage(activeRoomId, submitted)
                    .then(
                      (sent) =>
                        sent &&
                        setDraft((current) =>
                          current.trim() === submitted ? "" : current,
                        ),
                    );
                }}
              >
                <input
                  value={draft}
                  disabled={!activeRoomId || controller.sending}
                  maxLength={1_000}
                  aria-label="Add a safe owner note"
                  placeholder={
                    activeRoomId
                      ? "Add a note to this workstream…"
                      : "A workstream room will appear here"
                  }
                  onChange={(event) => setDraft(event.target.value)}
                />
                <button
                  type="submit"
                  disabled={
                    !activeRoomId || controller.sending || !draft.trim()
                  }
                >
                  {controller.sending ? "Sending…" : "Send"}
                </button>
              </form>
            </div>
          </div>
        </section>
      </div>
    </section>
  );
}
