/**
 * Reef presentation helpers. Presence is coarse authoritative truth: these
 * helpers only reshape what the backend observed - they never invent
 * activity, and a missing observation stays visibly unknown.
 */

import type { FloorPresenceItem, FloorPresenceTarget } from "./types";

const STALE_AFTER_MS = 2 * 60_000;

export interface PresenceFreshness {
  label: string;
  tone: "fresh" | "stale" | "unknown";
}

export function presenceFreshness(
  observedAtMs: number | null,
  nowMs: number,
): PresenceFreshness {
  if (observedAtMs === null) {
    return { label: "No recent observation", tone: "unknown" };
  }
  const age = Math.max(0, nowMs - observedAtMs);
  if (age < 60_000) {
    return { label: "Observed just now", tone: "fresh" };
  }
  const tone = age >= STALE_AFTER_MS ? "stale" : "fresh";
  if (age < 60 * 60_000) {
    return { label: `Observed ${Math.floor(age / 60_000)}m ago`, tone };
  }
  return { label: `Observed ${Math.floor(age / (60 * 60_000))}h ago`, tone };
}

export interface PresenceDestination {
  tab: "assistant" | "runbook";
  label: string;
}

/**
 * Honest deep-link mapping: delegations and sessions live on the Office
 * floor; runs live in the run history. No destination is invented for
 * targets the app cannot authoritatively show.
 */
export function presenceTargetDestination(
  target: FloorPresenceTarget | null,
): PresenceDestination | null {
  if (!target) return null;
  switch (target.kind) {
    case "delegation":
      return { tab: "assistant", label: "Open in the Office" };
    case "session":
      return { tab: "assistant", label: "Open the conversation" };
    case "run":
      return { tab: "runbook", label: "Open the run history" };
  }
}

/** Presentation-only prominence for the executive assistant's crab. */
export function isExecassPresence(item: FloorPresenceItem): boolean {
  return item.display_name === "ExecAss";
}

export function sortPresence(
  items: readonly FloorPresenceItem[],
): FloorPresenceItem[] {
  return [
    ...items.filter((item) => isExecassPresence(item)),
    ...items.filter((item) => !isExecassPresence(item)),
  ];
}
