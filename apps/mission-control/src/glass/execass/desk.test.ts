import { describe, expect, it } from "vitest";

import {
  DESK_ENTRY_CAP,
  appendEntry,
  askEntry,
  closeDesk,
  delegationCreatedEntry,
  initialDeskState,
  noteEntry,
  openDesk,
  openDeskForAttention,
  replyEntry,
  resolveFocusAttention,
  revisionSentEntry,
} from "./desk";
import { fixtureSummaryResponse } from "./fixtures";
import type { AttentionItem } from "./types";

function delegationAttention(): AttentionItem {
  const item = fixtureSummaryResponse().needs_you.find(
    (candidate) => candidate.subject.scope_kind === "delegation",
  );
  if (!item) throw new Error("fixture has no delegation attention");
  return item;
}

describe("desk open and close", () => {
  it("opens plain with no focus and keeps entries from earlier in the sitting", () => {
    let state = initialDeskState();
    state = appendEntry(state, askEntry("hello", { id: "e1", atMs: 1 }));
    state = closeDesk(state);
    expect(state.open).toBe(false);
    const reopened = openDesk(state);
    expect(reopened.open).toBe(true);
    expect(reopened.focus).toBeNull();
    expect(reopened.entries).toHaveLength(1);
  });

  it("opens focused on a delegation-scoped attention item", () => {
    const item = delegationAttention();
    const state = openDeskForAttention(initialDeskState(), item);
    expect(state.open).toBe(true);
    expect(state.focus?.attention_id).toBe(item.attention_id);
    expect(state.focus?.delegation_id).toBe(
      item.subject.scope_kind === "delegation"
        ? item.subject.delegation_id
        : null,
    );
  });

  it("focuses a runtime-scoped item without inventing a delegation", () => {
    const item: AttentionItem = {
      ...delegationAttention(),
      attention_id: "att-runtime",
      subject: {
        scope_kind: "runtime_host",
        runtime_host_generation: 1,
        runtime_host_instance_id: "host-1",
        runtime_fencing_token: 1,
        runtime_actual_state: "faulted",
        runtime_end_reason: "crash",
        active_work_binding_digest: "sha256:x",
      },
    };
    const state = openDeskForAttention(initialDeskState(), item);
    expect(state.focus?.attention_id).toBe("att-runtime");
    expect(state.focus?.delegation_id).toBeNull();
  });

  it("closing the desk clears focus but keeps the conversation", () => {
    const item = delegationAttention();
    let state = openDeskForAttention(initialDeskState(), item);
    state = appendEntry(state, askEntry("about this", { id: "e1", atMs: 1 }));
    state = closeDesk(state);
    expect(state.focus).toBeNull();
    expect(state.entries).toHaveLength(1);
  });
});

describe("desk entries", () => {
  it("builds typed entries", () => {
    expect(askEntry("hi", { id: "a", atMs: 1 }).kind).toBe("owner_ask");
    expect(replyEntry("yo", { id: "b", atMs: 2 }).kind).toBe("execass_reply");
    expect(revisionSentEntry("change it", { id: "c", atMs: 3 }).kind).toBe(
      "revision_sent",
    );
    expect(noteEntry("heads up", { id: "d", atMs: 4 }).kind).toBe("desk_note");
    const created = delegationCreatedEntry(
      { delegation_id: "dl-9", intent_summary: "Do the thing" },
      { id: "e", atMs: 5 },
    );
    expect(created.kind).toBe("delegation_created");
    expect(created.delegation_id).toBe("dl-9");
    expect(created.summary).toBe("Do the thing");
  });

  it("caps the log by dropping the oldest entries", () => {
    let state = initialDeskState();
    for (let index = 0; index < DESK_ENTRY_CAP + 5; index += 1) {
      state = appendEntry(
        state,
        askEntry(`ask ${index}`, { id: `e${index}`, atMs: index }),
      );
    }
    expect(state.entries).toHaveLength(DESK_ENTRY_CAP);
    expect(state.entries[0]?.kind).toBe("owner_ask");
    expect((state.entries[0] as { text: string }).text).toBe("ask 5");
  });
});

describe("resolveFocusAttention", () => {
  it("finds the live attention item by id", () => {
    const summary = fixtureSummaryResponse();
    const item = delegationAttention();
    const state = openDeskForAttention(initialDeskState(), item);
    expect(resolveFocusAttention(summary, state.focus)?.attention_id).toBe(
      item.attention_id,
    );
  });

  it("returns null when the decision left the summary or focus is empty", () => {
    const summary = fixtureSummaryResponse();
    const state = openDeskForAttention(initialDeskState(), {
      ...delegationAttention(),
      attention_id: "att-vanished",
    });
    expect(resolveFocusAttention(summary, state.focus)).toBeNull();
    expect(resolveFocusAttention(summary, null)).toBeNull();
  });
});
