import { describe, expect, test } from "vitest";

import { composeBriefing } from "./briefing";
import {
  fixtureAttentionItem,
  fixtureDelegationSummary,
  fixtureNextItem,
  fixtureSummaryResponse,
} from "./fixtures";
import type { SummaryResponse } from "./types";

const empty = (): SummaryResponse => ({
  needs_you: [],
  in_motion: [],
  done: [],
  next: [],
  receipts: [],
  displayed: { cursor: "c0", displayed_at_ms: 0, delivered: [] },
});

describe("composeBriefing", () => {
  test("a quiet summary reads as reassuring, with nothing needed", () => {
    const brief = composeBriefing(empty());
    expect(brief.headline.toLowerCase()).toContain("quiet");
    expect(brief.needsCount).toBe(0);
    expect(brief.paragraph).toContain("Nothing needs you");
  });

  test("attention items lead the briefing and set the count", () => {
    const brief = composeBriefing(fixtureSummaryResponse());
    expect(brief.needsCount).toBe(2);
    expect(brief.headline).toContain("2");
    expect(brief.paragraph).toContain("permanent");
  });

  test("ordinary progress is mentioned without inventing detail", () => {
    const summary = empty();
    summary.in_motion = [fixtureDelegationSummary()];
    const brief = composeBriefing(summary);
    expect(brief.paragraph).toContain("Plan the October team retreat");
    expect(brief.paragraph).not.toContain("undefined");
  });

  test("external waits are described as not on the user", () => {
    const summary = empty();
    summary.in_motion = [
      fixtureDelegationSummary({
        delegation_id: "dlg-ext",
        phase: "waiting_external",
        intent_summary: "Get the contract signed",
        outcome_summary: "Waiting on their signature",
        pending_external_wait: "Awaiting countersignature",
      }),
    ];
    const brief = composeBriefing(summary);
    expect(brief.paragraph.toLowerCase()).toContain("waiting on the world");
  });

  test("failures are stated plainly, never hidden behind reassurance", () => {
    const summary = empty();
    summary.done = [
      fixtureDelegationSummary({
        delegation_id: "dlg-fail",
        phase: "failed",
        intent_summary: "Migrate the wiki",
        outcome_summary: "Failed: export was corrupted",
        terminal_at_ms: 5,
      }),
    ];
    const brief = composeBriefing(summary);
    expect(brief.paragraph).toContain("Failed: export was corrupted");
    expect(brief.tone).toBe("attention");
  });

  test("partial completion is called out as partial", () => {
    const summary = empty();
    summary.done = [
      fixtureDelegationSummary({
        delegation_id: "dlg-part",
        phase: "partially_completed",
        intent_summary: "Clean the list",
        outcome_summary: "96% done - 12 bounces re-verifying",
        terminal_at_ms: 5,
      }),
    ];
    const brief = composeBriefing(summary);
    expect(brief.paragraph.toLowerCase()).toContain("partly");
  });

  test("recovering work is described honestly", () => {
    const summary = empty();
    summary.in_motion = [
      fixtureDelegationSummary({
        delegation_id: "dlg-rec",
        phase: "recovering",
        intent_summary: "Newsletter cleanup",
        outcome_summary: "Re-verifying 12 bounced addresses",
      }),
    ];
    const brief = composeBriefing(summary);
    expect(brief.paragraph.toLowerCase()).toContain("recover");
  });

  test("upcoming commitments get a next mention", () => {
    const summary = empty();
    summary.next = [
      fixtureNextItem({
        next_item_id: "n1",
        kind: "commitment",
        summary: "Promised reply to Nadia",
      }),
    ];
    const brief = composeBriefing(summary);
    expect(brief.paragraph).toContain("Promised reply to Nadia");
  });

  test("only restates projection facts - no fabricated activity on empty buckets", () => {
    const brief = composeBriefing(empty());
    expect(brief.paragraph).not.toMatch(/working on|in motion|progress/i);
  });

  test("an attention-only summary keeps a single decision singular", () => {
    const summary = empty();
    summary.needs_you = [fixtureAttentionItem()];
    const brief = composeBriefing(summary);
    expect(brief.headline).toMatch(/one thing|1 thing/i);
  });
});
