import { useMemo, useState, type FormEvent } from "react";

import type { GlassWindowController } from "./useGlassWindowController";

export function GlassWindowPage(props: {
  controller: GlassWindowController;
}) {
  const { controller } = props;
  const [selectedRoomId, setSelectedRoomId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const rooms = controller.chatter?.rooms ?? [];
  const activeRoomId =
    selectedRoomId && rooms.some((room) => room.thread_id === selectedRoomId)
      ? selectedRoomId
      : (rooms[0]?.thread_id ?? null);
  const messages = useMemo(
    () =>
      (controller.chatter?.messages ?? []).filter(
        (message) => message.thread_id === activeRoomId,
      ),
    [activeRoomId, controller.chatter?.messages],
  );

  return (
    <section className="mc-window-floor" aria-label="The Window">
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
          <div className="mc-reef-water">
            {(controller.presence?.items ?? []).length === 0 ? (
              <div className="mc-window-empty">
                No authoritative presence observation yet.
              </div>
            ) : (
              controller.presence?.items.map((item) => (
                <article
                  key={item.agent_id}
                  className={`mc-reef-agent is-${item.mood}`}
                  data-activity={item.activity}
                >
                  <span className="mc-reef-crab" aria-hidden="true">🦀</span>
                  <strong>{item.display_name}</strong>
                  <span>{item.activity_label}</span>
                </article>
              ))
            )}
          </div>
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
                    {room.unread_count ? <b>{room.unread_count}</b> : null}
                  </button>
                ))
              )}
            </nav>
            <div className="mc-chatter-stream">
              <div className="mc-chatter-messages">
                {messages.length === 0 ? (
                  <div className="mc-window-empty">
                    Nothing safe to overhear yet.
                  </div>
                ) : (
                  messages.map((message) => (
                    <article key={message.message_id}>
                      <div>
                        <strong>{message.author.display_name}</strong>
                        <time>
                          {new Date(message.created_at_ms).toLocaleTimeString(
                            [],
                            { hour: "numeric", minute: "2-digit" },
                          )}
                        </time>
                      </div>
                      <p>{message.text}</p>
                    </article>
                  ))
                )}
              </div>
              <form
                className="mc-chatter-compose"
                onSubmit={(event: FormEvent) => {
                  event.preventDefault();
                  if (!activeRoomId) return;
                  void controller
                    .sendMessage(activeRoomId, draft)
                    .then((sent) => sent && setDraft(""));
                }}
              >
                <input
                  value={draft}
                  disabled={!activeRoomId}
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
                  disabled={!activeRoomId || !draft.trim()}
                >
                  Send
                </button>
              </form>
            </div>
          </div>
        </section>
      </div>
    </section>
  );
}
