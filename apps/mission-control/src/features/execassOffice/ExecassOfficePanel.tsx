/**
 * The ExecAss Office: briefing, ask-box, Needs You, In Motion,
 * Done Since You Checked, Next, the desk tray, and the freeze switch.
 * Renders only the authoritative projection; internal machinery stays a
 * deliberate click away (receipt inspection), never ambient.
 */

import {
  useCallback,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";

import { useGlassSurfaceTheme } from "../../glass/useGlassSurfaceTheme";

import type {
  AttentionItem,
  DelegationSummary,
  NextItem,
  ReceiptSummary,
} from "../../glass/execass/types";
import { OFFICE_BLOCK_REGISTRY } from "./officeBlocks";
import { useOfficeLayout } from "./useOfficeLayout";
import type { ExecassOfficeController } from "./useExecassOfficeController";

function formatClock(ms: number | null | undefined): string {
  if (!ms) {
    return "";
  }
  try {
    return new Date(ms).toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

function AttentionCard(props: {
  item: AttentionItem;
  busy: boolean;
  onResolve: (
    item: AttentionItem,
    result: "confirm_and_continue" | "revise" | "decline" | "stop",
    revisionText?: string,
  ) => void;
}) {
  const { item, busy, onResolve } = props;
  const [talkOpen, setTalkOpen] = useState(false);
  const [revisionDraft, setRevisionDraft] = useState("");
  const isDecision = Boolean(item.decision_id);
  const isDangerous = item.decision_kind === "dangerous_action_confirmation";
  const isClarification = item.decision_kind === "clarification";

  const kindLabel = isDangerous
    ? "One confirmation - consequence stated"
    : isClarification
      ? "Quick question"
      : item.kind === "recovery_choice"
        ? "Recovery choice"
        : item.kind === "reply"
          ? "Reply wanted"
          : item.kind === "runtime_paused"
            ? "Runtime paused"
            : "Needs your word";

  return (
    <article
      className={`mc-execass-decision${isDangerous ? " is-dangerous" : ""}`}
      data-testid="execass-attention-card"
    >
      <span className={`mc-execass-kind${isDangerous ? " is-gold" : ""}`}>
        {kindLabel}
      </span>
      <p className="mc-execass-reason">{item.reason}</p>
      <p className="mc-execass-recommendation">
        ExecAss recommends: {item.recommendation}
      </p>
      {isClarification ? (
        <div className="mc-execass-options">
          {item.alternatives_or_actions.map((option) => (
            <button
              key={option}
              type="button"
              className="mc-execass-option"
              disabled={busy}
              onClick={() => onResolve(item, "confirm_and_continue", option)}
            >
              {option}
            </button>
          ))}
        </div>
      ) : isDecision ? (
        <div className="mc-execass-actions">
          <button
            type="button"
            className="mc-execass-yes"
            disabled={busy}
            onClick={() => onResolve(item, "confirm_and_continue")}
          >
            {busy ? "Working..." : "Yep - go ahead"}
          </button>
          <button
            type="button"
            className="mc-execass-talk"
            disabled={busy}
            onClick={() => setTalkOpen((open) => !open)}
          >
            Let's talk it through
          </button>
          <button
            type="button"
            className="mc-execass-quiet"
            disabled={busy}
            onClick={() => onResolve(item, "decline")}
          >
            Decline
          </button>
          <button
            type="button"
            className="mc-execass-danger"
            disabled={busy}
            onClick={() => onResolve(item, "stop")}
          >
            Stop
          </button>
        </div>
      ) : (
        <p className="mc-execass-informational">
          Informational - handled where it lives.
        </p>
      )}
      {talkOpen && isDecision ? (
        <form
          className="mc-execass-revise"
          onSubmit={(event: FormEvent) => {
            event.preventDefault();
            if (revisionDraft.trim()) {
              onResolve(item, "revise", revisionDraft.trim());
              setTalkOpen(false);
              setRevisionDraft("");
            }
          }}
        >
          <input
            type="text"
            value={revisionDraft}
            placeholder="Tell ExecAss how to change it..."
            aria-label="Revise this decision"
            onChange={(event) => setRevisionDraft(event.target.value)}
          />
          <button type="submit" disabled={busy || !revisionDraft.trim()}>
            Send revision
          </button>
        </form>
      ) : null}
    </article>
  );
}

function MotionRow(props: { delegation: DelegationSummary }) {
  const { delegation } = props;
  const external = delegation.phase === "waiting_external";
  const recovering = delegation.phase === "recovering";
  return (
    <div className="mc-execass-row" data-testid="execass-motion-row">
      <span
        className={`mc-execass-pulse${external ? " is-ext" : ""}${recovering ? " is-rec" : ""}`}
        aria-hidden="true"
      />
      <span className="mc-execass-row-main">
        {delegation.intent_summary}
        <small>{delegation.outcome_summary}</small>
      </span>
      <span className={`mc-execass-phase${external ? " is-ext" : ""}`}>
        {external
          ? "waiting on the world"
          : recovering
            ? "recovering"
            : delegation.run_control !== "running"
              ? delegation.run_control.replace("_", " ")
              : "working"}
      </span>
    </div>
  );
}

function DoneRow(props: {
  delegation: DelegationSummary;
  onLoadReceipts: (delegationId: string) => Promise<ReceiptSummary[]>;
}) {
  const { delegation, onLoadReceipts } = props;
  const [receipts, setReceipts] = useState<ReceiptSummary[] | null>(null);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const partial = delegation.phase === "partially_completed";
  const failed = delegation.phase === "failed";

  const toggleProof = useCallback(async () => {
    if (open) {
      setOpen(false);
      return;
    }
    setOpen(true);
    if (receipts === null && !loading) {
      setLoading(true);
      try {
        setReceipts(await onLoadReceipts(delegation.delegation_id));
      } catch {
        setReceipts([]);
      } finally {
        setLoading(false);
      }
    }
  }, [delegation.delegation_id, loading, onLoadReceipts, open, receipts]);

  return (
    <div className="mc-execass-done" data-testid="execass-done-row">
      <div className="mc-execass-row">
        <span
          className={`mc-execass-tick${partial ? " is-part" : ""}${failed ? " is-fail" : ""}`}
        >
          {failed ? "✕" : partial ? "◐" : "✓"}
        </span>
        <span className="mc-execass-row-main">
          {delegation.intent_summary}
          <small>{delegation.outcome_summary}</small>
        </span>
        <button
          type="button"
          className="mc-execass-receipt"
          onClick={() => void toggleProof()}
        >
          {open ? "hide proof" : "see the proof"}
        </button>
      </div>
      {open ? (
        <div className="mc-execass-proof" data-testid="execass-proof">
          {loading ? (
            <span>Fetching receipts...</span>
          ) : receipts && receipts.length > 0 ? (
            receipts.map((receipt) => (
              <div key={receipt.receipt_id} className="mc-execass-proof-row">
                <span>{receipt.safe_summary}</span>
                <code>{receipt.receipt_id}</code>
              </div>
            ))
          ) : (
            <span>No receipts recorded for this delegation yet.</span>
          )}
        </div>
      ) : null}
    </div>
  );
}

function NextRow(props: { item: NextItem }) {
  const { item } = props;
  const when = item.due_at_ms ?? item.scheduled_for_ms;
  return (
    <div className="mc-execass-row" data-testid="execass-next-row">
      <span className="mc-execass-when">{when ? formatClock(when) : "soon"}</span>
      <span className="mc-execass-row-main">{item.summary}</span>
      <span className="mc-execass-tag">{item.kind.replace("_", " ")}</span>
    </div>
  );
}

export function ExecassOfficePanel(props: {
  controller: ExecassOfficeController;
}) {
  const { controller } = props;
  const [askDraft, setAskDraft] = useState("");
  const [trayOpen, setTrayOpen] = useState(false);
  const [freezeArmed, setFreezeArmed] = useState(false);
  const layout = useOfficeLayout();
  const surfaceRef = useRef<HTMLElement | null>(null);
  useGlassSurfaceTheme(surfaceRef);
  const summary = controller.summary;
  const briefing = controller.briefing;
  const stopEngaged = controller.stopAll?.engaged === true;

  const submitAsk = useCallback(
    (event: FormEvent) => {
      event.preventDefault();
      if (askDraft.trim()) {
        void controller.delegate(askDraft);
        setAskDraft("");
      }
    },
    [askDraft, controller],
  );

  return (
    <section
      ref={surfaceRef}
      className="mc-execass-office"
      aria-label="ExecAss Office"
      data-testid="execass-office"
    >
      <header className="mc-execass-header">
        <span className="mc-execass-title">ExecAss</span>
        <span className="mc-execass-status">
          {stopEngaged
            ? "⏸ everybody froze - work is holding at a safe boundary"
            : "● on duty"}
        </span>
        <span className="mc-execass-spacer" />
        {stopEngaged ? (
          <button
            type="button"
            className="mc-execass-yes"
            disabled={controller.freezeBusy}
            onClick={() => void controller.resumeAllWork()}
          >
            {controller.freezeBusy ? "Resuming..." : "Resume the floor"}
          </button>
        ) : freezeArmed ? (
          <span className="mc-execass-freeze-confirm">
            Stop all work at the next safe boundary?
            <button
              type="button"
              className="mc-execass-danger"
              disabled={controller.freezeBusy}
              onClick={() => {
                setFreezeArmed(false);
                void controller.freezeAll();
              }}
            >
              {controller.freezeBusy ? "Freezing..." : "Yes - everybody freeze"}
            </button>
            <button
              type="button"
              className="mc-execass-quiet"
              onClick={() => setFreezeArmed(false)}
            >
              Cancel
            </button>
          </span>
        ) : (
          <button
            type="button"
            className="mc-execass-quiet"
            onClick={() => setFreezeArmed(true)}
            title="Stop all ExecAss work at a safe boundary"
          >
            🧊 Freeze
          </button>
        )}
        <button
          type="button"
          className="mc-execass-quiet"
          onClick={() => setTrayOpen((open) => !open)}
        >
          🗒 While you were out
          {controller.trayNotes.length > 0
            ? ` (${controller.trayNotes.length})`
            : ""}
        </button>
        <button
          type="button"
          className={`mc-execass-quiet${layout.arranging ? " is-on" : ""}`}
          aria-label="Arrange office"
          aria-pressed={layout.arranging}
          onClick={() => layout.setArranging(!layout.arranging)}
          title="Reorder, resize, hide, or add office blocks"
        >
          {layout.arranging ? "Done arranging" : "⌗ Arrange"}
        </button>
      </header>

      {trayOpen ? (
        <div className="mc-execass-tray" data-testid="execass-tray">
          {controller.trayNotes.length === 0 ? (
            <p>No notes on your desk. It was a quiet stretch.</p>
          ) : (
            controller.trayNotes.map((note) => (
              <div key={note.id} className="mc-execass-tray-note">
                <span>{note.text}</span>
                <button
                  type="button"
                  onClick={() => controller.dismissTrayNote(note.id)}
                  aria-label="Dismiss note"
                >
                  ✕
                </button>
              </div>
            ))
          )}
        </div>
      ) : null}

      {controller.summaryError ? (
        <p className="mc-execass-error" role="alert">
          {controller.summaryError}
        </p>
      ) : null}

      {briefing ? (
        <div className="mc-execass-briefing" data-tone={briefing.tone}>
          <h2>{briefing.headline}</h2>
          <p>{briefing.paragraph}</p>
        </div>
      ) : controller.summaryLoading ? (
        <div className="mc-execass-briefing">
          <p>Pulling this morning's brief...</p>
        </div>
      ) : null}

      <form className="mc-execass-ask" onSubmit={submitAsk}>
        <input
          type="text"
          value={askDraft}
          placeholder="What do you need? Say it like you'd say it out loud..."
          aria-label="Delegate an outcome to ExecAss"
          disabled={controller.intakeBusy}
          onChange={(event) => setAskDraft(event.target.value)}
        />
        <button type="submit" disabled={controller.intakeBusy || !askDraft.trim()}>
          {controller.intakeBusy ? "Handing off..." : "Hand it off"}
        </button>
      </form>
      {controller.conversationalReply ? (
        <div className="mc-execass-reply" data-testid="execass-reply">
          <span>{controller.conversationalReply}</span>
          <button
            type="button"
            onClick={controller.clearConversationalReply}
            aria-label="Dismiss reply"
          >
            ✕
          </button>
        </div>
      ) : null}

      {summary
        ? (() => {
            const blockBodies: Record<
              import("./officeBlocks").OfficeBlockRendererKey,
              () => ReactNode
            > = {
              "needs-you": () => (
                <>
                  <h3>
                    Needs you <em>{summary.needs_you.length}</em>
                  </h3>
                  {summary.needs_you.length === 0 ? (
                    <p className="mc-execass-empty">
                      Nothing needs you. Reassuringly quiet.
                    </p>
                  ) : (
                    summary.needs_you.map((item) => (
                      <AttentionCard
                        key={item.attention_id}
                        item={item}
                        busy={
                          item.decision_id !== null &&
                          item.decision_id !== undefined &&
                          controller.resolvingDecisionIds.includes(
                            item.decision_id,
                          )
                        }
                        onResolve={(target, result, revisionText) =>
                          void controller.resolveAttention(
                            target,
                            result,
                            revisionText,
                          )
                        }
                      />
                    ))
                  )}
                </>
              ),
              "in-motion": () => (
                <>
                  <h3>
                    In motion <em>{summary.in_motion.length}</em>
                  </h3>
                  {summary.in_motion.length === 0 ? (
                    <p className="mc-execass-empty">The floor is idle.</p>
                  ) : (
                    summary.in_motion.map((delegation) => (
                      <MotionRow
                        key={delegation.delegation_id}
                        delegation={delegation}
                      />
                    ))
                  )}
                </>
              ),
              done: () => (
                <>
                  <h3>Done since you checked</h3>
                  {summary.done.length === 0 ? (
                    <p className="mc-execass-empty">Nothing new finished yet.</p>
                  ) : (
                    summary.done.map((delegation) => (
                      <DoneRow
                        key={delegation.delegation_id}
                        delegation={delegation}
                        onLoadReceipts={controller.loadReceipts}
                      />
                    ))
                  )}
                </>
              ),
              next: () => (
                <>
                  <h3>Next</h3>
                  {summary.next.length === 0 ? (
                    <p className="mc-execass-empty">
                      No commitments on the horizon.
                    </p>
                  ) : (
                    summary.next.map((item) => (
                      <NextRow key={item.next_item_id} item={item} />
                    ))
                  )}
                </>
              ),
            };
            return (
              <>
                <div
                  className={`mc-execass-buckets${layout.arranging ? " is-arranging" : ""}`}
                >
                  {layout.placements
                    .filter((placement) => placement.visible)
                    .map((placement) => {
                      const def = OFFICE_BLOCK_REGISTRY.find(
                        (entry) => entry.id === placement.id,
                      );
                      if (!def) return null;
                      const body = blockBodies[def.rendererKey];
                      return (
                        <section
                          key={placement.id}
                          aria-label={def.title}
                          className={`mc-office-block mc-block-${placement.size}`}
                          data-testid={`office-block-${placement.id}`}
                        >
                          {layout.arranging ? (
                            <div
                              className="mc-arrange-controls"
                              role="group"
                              aria-label={`Arrange ${def.title}`}
                            >
                              <button
                                type="button"
                                aria-label={`Move ${def.title} earlier`}
                                onClick={() => layout.move(placement.id, -1)}
                              >
                                ←
                              </button>
                              <button
                                type="button"
                                aria-label={`Move ${def.title} later`}
                                onClick={() => layout.move(placement.id, 1)}
                              >
                                →
                              </button>
                              <button
                                type="button"
                                aria-label={`Resize ${def.title}`}
                                title="Cycle size: small, medium, large"
                                onClick={() => layout.resize(placement.id)}
                              >
                                {placement.size.toUpperCase()}
                              </button>
                              <button
                                type="button"
                                aria-label={`Hide ${def.title}`}
                                onClick={() => layout.hide(placement.id)}
                              >
                                ✕
                              </button>
                            </div>
                          ) : null}
                          {body()}
                        </section>
                      );
                    })}
                </div>
                {layout.arranging ? (
                  <div
                    className="mc-office-library"
                    data-testid="office-library"
                  >
                    <span className="mc-office-library-label">
                      Block library
                    </span>
                    {layout.library.length === 0 ? (
                      <em>Every block is already on the canvas.</em>
                    ) : (
                      layout.library.map((def) => (
                        <button
                          key={def.id}
                          type="button"
                          aria-label={`Pin ${def.title} to Office`}
                          onClick={() => layout.pin(def.id)}
                        >
                          + {def.title}
                        </button>
                      ))
                    )}
                  </div>
                ) : null}
                {layout.error ? (
                  <p className="mc-theme-errors" role="alert">
                    {layout.error}
                  </p>
                ) : null}
              </>
            );
          })()
        : null}
    </section>
  );
}
