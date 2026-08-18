/**
 * The Assistant's Desk session model. The desk is a destination, not a
 * store: the conversation log lives for this sitting only (the contract
 * has no chat-history endpoint), durable work becomes delegations with
 * receipts, and the focused decision is always looked up live in the
 * authoritative summary — never rendered from a frozen copy.
 */

import type { AttentionItem, SummaryResponse } from "./types";

export const DESK_ENTRY_CAP = 100;

interface EntryMeta {
  id: string;
  atMs: number;
}

export type DeskEntry =
  | { kind: "owner_ask"; id: string; at_ms: number; text: string }
  | { kind: "execass_reply"; id: string; at_ms: number; text: string }
  | {
      kind: "delegation_created";
      id: string;
      at_ms: number;
      delegation_id: string;
      summary: string;
    }
  | { kind: "revision_sent"; id: string; at_ms: number; text: string }
  | { kind: "desk_note"; id: string; at_ms: number; text: string };

export interface DeskFocus {
  attention_id: string;
  delegation_id: string | null;
}

export interface DeskState {
  open: boolean;
  entries: DeskEntry[];
  focus: DeskFocus | null;
}

export function initialDeskState(): DeskState {
  return { open: false, entries: [], focus: null };
}

export function openDesk(state: DeskState): DeskState {
  return { ...state, open: true, focus: null };
}

export function openDeskForAttention(
  state: DeskState,
  item: AttentionItem,
): DeskState {
  return {
    ...state,
    open: true,
    focus: {
      attention_id: item.attention_id,
      delegation_id:
        item.subject.scope_kind === "delegation"
          ? item.subject.delegation_id
          : null,
    },
  };
}

/** Walking away: the visit's focus ends, the sitting's conversation stays. */
export function closeDesk(state: DeskState): DeskState {
  return { ...state, open: false, focus: null };
}

export function appendEntry(state: DeskState, entry: DeskEntry): DeskState {
  const entries = [...state.entries, entry];
  return {
    ...state,
    entries:
      entries.length > DESK_ENTRY_CAP
        ? entries.slice(entries.length - DESK_ENTRY_CAP)
        : entries,
  };
}

export function askEntry(text: string, meta: EntryMeta): DeskEntry {
  return { kind: "owner_ask", id: meta.id, at_ms: meta.atMs, text };
}

export function replyEntry(text: string, meta: EntryMeta): DeskEntry {
  return { kind: "execass_reply", id: meta.id, at_ms: meta.atMs, text };
}

export function delegationCreatedEntry(
  delegation: { delegation_id: string; intent_summary: string },
  meta: EntryMeta,
): Extract<DeskEntry, { kind: "delegation_created" }> {
  return {
    kind: "delegation_created",
    id: meta.id,
    at_ms: meta.atMs,
    delegation_id: delegation.delegation_id,
    summary: delegation.intent_summary,
  };
}

export function revisionSentEntry(text: string, meta: EntryMeta): DeskEntry {
  return { kind: "revision_sent", id: meta.id, at_ms: meta.atMs, text };
}

export function noteEntry(text: string, meta: EntryMeta): DeskEntry {
  return { kind: "desk_note", id: meta.id, at_ms: meta.atMs, text };
}

/** Live lookup: a focused decision that left the summary is gone, not stale. */
export function resolveFocusAttention(
  summary: SummaryResponse | null,
  focus: DeskFocus | null,
): AttentionItem | null {
  if (!summary || !focus) return null;
  return (
    summary.needs_you.find(
      (item) => item.attention_id === focus.attention_id,
    ) ?? null
  );
}
