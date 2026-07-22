/**
 * Deterministic briefing prose composed from the authoritative summary
 * projection. Per the locked integration decisions there is no backend
 * composer and no fabrication: every sentence restates projection facts
 * (intent/outcome summaries, phases, waits) and nothing else.
 */

import type { DelegationSummary, SummaryResponse } from "./types";

export type BriefingTone = "quiet" | "normal" | "attention";

export interface Briefing {
  headline: string;
  paragraph: string;
  tone: BriefingTone;
  needsCount: number;
}

function countPhrase(count: number, singular: string, plural: string): string {
  if (count === 1) return `one ${singular}`;
  return `${count} ${plural}`;
}

function describeDone(delegation: DelegationSummary): string {
  if (delegation.phase === "failed") {
    return `${delegation.intent_summary}: ${delegation.outcome_summary}`;
  }
  if (delegation.phase === "partially_completed") {
    return `${delegation.intent_summary} is partly done (${delegation.outcome_summary})`;
  }
  return `${delegation.intent_summary}: ${delegation.outcome_summary}`;
}

export function composeBriefing(summary: SummaryResponse): Briefing {
  const needsCount = summary.needs_you.length;
  const failed = summary.done.filter((d) => d.phase === "failed");
  const partial = summary.done.filter((d) => d.phase === "partially_completed");
  const completed = summary.done.filter((d) => d.phase === "completed");
  const external = summary.in_motion.filter(
    (d) => d.phase === "waiting_external",
  );
  const recovering = summary.in_motion.filter((d) => d.phase === "recovering");
  const moving = summary.in_motion.filter(
    (d) => d.phase !== "waiting_external" && d.phase !== "recovering",
  );

  const sentences: string[] = [];

  if (failed.length > 0) {
    sentences.push(`Straight talk first - ${failed.map(describeDone).join("; ")}.`);
  }
  if (needsCount > 0) {
    const dangerous = summary.needs_you.some(
      (item) => item.decision_kind === "dangerous_action_confirmation",
    );
    sentences.push(
      `${countPhrase(needsCount, "thing", "things")[0]?.toUpperCase()}${countPhrase(
        needsCount,
        "thing",
        "things",
      ).slice(1)} ${needsCount === 1 ? "needs" : "need"} you below${
        dangerous ? " - one is permanent, so it waits for your word" : ""
      }.`,
    );
  }
  if (completed.length > 0) {
    sentences.push(
      `Done since you checked: ${completed.map(describeDone).join("; ")}.`,
    );
  }
  if (partial.length > 0) {
    sentences.push(`${partial.map(describeDone).join("; ")}.`);
  }
  if (moving.length > 0) {
    sentences.push(
      `Moving along: ${moving
        .map((d) => `${d.intent_summary} (${d.outcome_summary})`)
        .join("; ")}.`,
    );
  }
  if (recovering.length > 0) {
    sentences.push(
      `Recovering without drama: ${recovering
        .map((d) => `${d.intent_summary} - ${d.outcome_summary}`)
        .join("; ")}.`,
    );
  }
  if (external.length > 0) {
    sentences.push(
      `Waiting on the world, not on you: ${external
        .map((d) => d.pending_external_wait ?? d.outcome_summary)
        .join("; ")}.`,
    );
  }
  if (summary.next.length > 0) {
    sentences.push(
      `Coming up: ${summary.next.map((item) => item.summary).join("; ")}.`,
    );
  }
  if (sentences.length === 0) {
    sentences.push("Nothing needs you, and nothing is stuck.");
  }

  const tone: BriefingTone =
    failed.length > 0
      ? "attention"
      : needsCount > 0
        ? "normal"
        : summary.in_motion.length === 0 && summary.done.length === 0
          ? "quiet"
          : "normal";

  const headline =
    needsCount > 0
      ? `Morning, boss - ${countPhrase(needsCount, "thing", "things")} for you.`
      : failed.length > 0
        ? "Morning, boss - one thing went wrong overnight."
        : "Morning, boss. All quiet - nothing needs you.";

  return {
    headline,
    paragraph: sentences.join(" "),
    tone,
    needsCount,
  };
}
