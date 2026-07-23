/**
 * The Assistant's Desk: the deliberate slide-over conversation destination
 * on Floor 4. Walking over never leaves the Office - the desk slides over
 * the canvas with the ExecAss conversation, the focused decision looked up
 * live in the summary, and the current plan over the shoulder. The log is
 * honest about its lifetime: this sitting only; durable work is a
 * delegation with receipts.
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";

import { useGlassSurfaceTheme } from "../../glass/useGlassSurfaceTheme";
import {
  appendEntry,
  askEntry,
  delegationCreatedEntry,
  noteEntry,
  replyEntry,
  resolveFocusAttention,
  revisionSentEntry,
  type DeskEntry,
  type DeskState,
} from "../../glass/execass/desk";
import type {
  AttentionItem,
  DelegationDetail,
} from "../../glass/execass/types";
import type { ExecassOfficeController } from "./useExecassOfficeController";

function newId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `desk-${Math.random().toString(16).slice(2)}`;
}

type ShoulderState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "loaded"; detail: DelegationDetail }
  | { kind: "error" };

function DeskLogEntry(props: { entry: DeskEntry }) {
  const { entry } = props;
  switch (entry.kind) {
    case "owner_ask":
      return (
        <li className="mc-desk-line is-owner">
          <span className="mc-desk-who">You</span>
          <span>{entry.text}</span>
        </li>
      );
    case "execass_reply":
      return (
        <li className="mc-desk-line is-execass">
          <span className="mc-desk-who">ExecAss</span>
          <span>{entry.text}</span>
        </li>
      );
    case "delegation_created":
      return (
        <li className="mc-desk-line is-durable">
          <span className="mc-desk-who">ExecAss</span>
          <span>
            On it - this became durable work: {entry.summary}
            <em className="mc-desk-tag">delegation</em>
          </span>
        </li>
      );
    case "revision_sent":
      return (
        <li className="mc-desk-line is-owner">
          <span className="mc-desk-who">You</span>
          <span>
            {entry.text}
            <em className="mc-desk-tag">revision</em>
          </span>
        </li>
      );
    case "desk_note":
      return (
        <li className="mc-desk-line is-note">
          <span>{entry.text}</span>
        </li>
      );
  }
}

function OverTheShoulder(props: { state: ShoulderState }) {
  const { state } = props;
  return (
    <aside className="mc-desk-shoulder" aria-label="Over the shoulder">
      <h4>Over the shoulder</h4>
      {state.kind === "idle" ? (
        <p className="mc-desk-soft">
          No file open on the desk. Walk over from a decision to see its plan,
          or just talk.
        </p>
      ) : state.kind === "loading" ? (
        <p className="mc-desk-soft">Pulling the working file...</p>
      ) : state.kind === "error" ? (
        <p className="mc-desk-soft" role="alert">
          The plan could not be fetched right now. The conversation still
          works; receipts remain the proof of record.
        </p>
      ) : (
        <div className="mc-desk-file">
          <p className="mc-desk-intent">
            <span>Asked for</span>
            {state.detail.original_intent}
          </p>
          <p className="mc-desk-plan">
            <span>The plan</span>
            {state.detail.plan_summary}
          </p>
          {state.detail.continuations.length > 0 ? (
            <ul className="mc-desk-steps">
              {state.detail.continuations.map((continuation) => (
                <li key={continuation.continuation_id}>
                  <span
                    className={`mc-desk-step-status is-${continuation.status}`}
                  >
                    {continuation.status.replace("_", " ")}
                  </span>
                  {continuation.safe_summary}
                </li>
              ))}
            </ul>
          ) : null}
          {state.detail.actions.length > 0 ? (
            <p className="mc-desk-boundary">
              {state.detail.actions[0]?.safe_boundary_description}
            </p>
          ) : null}
        </div>
      )}
    </aside>
  );
}

export function AssistantDesk(props: {
  controller: ExecassOfficeController;
  state: DeskState;
  /** Functional updates only: appends land on the latest state, never a stale snapshot. */
  onStateChange: (updater: (current: DeskState) => DeskState) => void;
  onClose: () => void;
}) {
  const { controller, state, onStateChange, onClose } = props;
  const [draft, setDraft] = useState("");
  const [revisionDraft, setRevisionDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [shoulder, setShoulder] = useState<ShoulderState>({ kind: "idle" });
  const composerRef = useRef<HTMLInputElement | null>(null);
  // Portaled out of the office subtree, the desk carries its own glass scope.
  const deskRef = useRef<HTMLElement | null>(null);
  useGlassSurfaceTheme(deskRef);

  const focusItem: AttentionItem | null = resolveFocusAttention(
    controller.summary,
    state.focus,
  );
  const focusVanished = state.focus !== null && focusItem === null;
  const focusedDelegationId = state.focus?.delegation_id ?? null;

  useEffect(() => {
    composerRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!focusedDelegationId) {
      setShoulder({ kind: "idle" });
      return;
    }
    let cancelled = false;
    setShoulder({ kind: "loading" });
    controller
      .loadDelegationDetail(focusedDelegationId)
      .then((detail) => {
        if (!cancelled) setShoulder({ kind: "loaded", detail });
      })
      .catch(() => {
        if (!cancelled) setShoulder({ kind: "error" });
      });
    return () => {
      cancelled = true;
    };
  }, [controller, focusedDelegationId]);

  const append = useCallback(
    (entry: DeskEntry) => {
      onStateChange((current) => appendEntry(current, entry));
    },
    [onStateChange],
  );

  const send = useCallback(
    async (event: FormEvent) => {
      event.preventDefault();
      const text = draft.trim();
      if (!text || sending) return;
      setSending(true);
      setDraft("");
      append(askEntry(text, { id: newId(), atMs: Date.now() }));
      const outcome = await controller.converse(text, focusedDelegationId);
      if (outcome.kind === "conversational") {
        append(replyEntry(outcome.text, { id: newId(), atMs: Date.now() }));
      } else if (outcome.kind === "delegation") {
        append(
          delegationCreatedEntry(outcome.delegation, {
            id: newId(),
            atMs: Date.now(),
          }),
        );
      } else {
        append(noteEntry(outcome.message, { id: newId(), atMs: Date.now() }));
      }
      setSending(false);
      composerRef.current?.focus();
    },
    [append, controller, draft, focusedDelegationId, sending],
  );

  const sendRevision = useCallback(
    async (event: FormEvent) => {
      event.preventDefault();
      const text = revisionDraft.trim();
      if (!text || !focusItem) return;
      setRevisionDraft("");
      append(revisionSentEntry(text, { id: newId(), atMs: Date.now() }));
      await controller.resolveAttention(focusItem, "revise", text);
    },
    [append, controller, focusItem, revisionDraft],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    },
    [onClose],
  );

  const isDangerous =
    focusItem?.decision_kind === "dangerous_action_confirmation";

  return createPortal(
    <div className="mc-desk-backdrop" onClick={onClose}>
      <aside
        ref={deskRef}
        role="dialog"
        aria-modal="true"
        aria-label="The Assistant's Desk"
        data-testid="assistant-desk"
        className="mc-desk"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <header className="mc-desk-header">
          <span className="mc-desk-title">The Assistant&apos;s Desk</span>
          <span className="mc-desk-presence">● ExecAss is at the desk</span>
          <button
            type="button"
            className="mc-desk-close"
            aria-label="Back to the office"
            onClick={onClose}
          >
            ✕
          </button>
        </header>

        <div className="mc-desk-columns">
          <section
            className="mc-desk-conversation"
            aria-label="Conversation with ExecAss"
          >
            {focusVanished ? (
              <p className="mc-desk-banner" role="status">
                That decision moved on while you were walking over - the
                office is current again.
              </p>
            ) : null}

            {focusItem ? (
              <article
                className={`mc-desk-decision${isDangerous ? " is-dangerous" : ""}`}
                data-testid="desk-decision"
              >
                <span className="mc-desk-kind">
                  {isDangerous
                    ? "One confirmation - consequence stated"
                    : "Talking it through"}
                </span>
                <p className="mc-desk-reason">{focusItem.reason}</p>
                <p className="mc-desk-recommendation">
                  ExecAss recommends: {focusItem.recommendation}
                </p>
                <form
                  className="mc-desk-revise"
                  data-testid="desk-revision"
                  onSubmit={(event) => void sendRevision(event)}
                >
                  <input
                    type="text"
                    value={revisionDraft}
                    placeholder="Tell ExecAss how to change it..."
                    aria-label="Revise this decision"
                    onChange={(event) => setRevisionDraft(event.target.value)}
                  />
                  <button type="submit" disabled={!revisionDraft.trim()}>
                    Send revision
                  </button>
                </form>
              </article>
            ) : null}

            <ol className="mc-desk-log" aria-label="Desk conversation log">
              {state.entries.map((entry) => (
                <DeskLogEntry key={entry.id} entry={entry} />
              ))}
            </ol>

            <p className="mc-desk-lifetime">
              This conversation lives on the desk for this sitting. Anything
              durable becomes a delegation with receipts.
            </p>

            <form
              className="mc-desk-composer"
              data-testid="desk-composer"
              onSubmit={(event) => void send(event)}
            >
              <input
                ref={composerRef}
                type="text"
                value={draft}
                placeholder={
                  focusedDelegationId
                    ? "Talk about this work..."
                    : "Talk to ExecAss..."
                }
                aria-label="Say something to ExecAss"
                disabled={sending}
                onChange={(event) => setDraft(event.target.value)}
              />
              <button
                type="submit"
                aria-label="Send to ExecAss"
                disabled={sending || !draft.trim()}
              >
                {sending ? "..." : "Send"}
              </button>
            </form>
          </section>

          <OverTheShoulder state={shoulder} />
        </div>
      </aside>
    </div>,
    document.body,
  );
}
