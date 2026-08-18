// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { EventsPage, type EventsPageEventItem } from "./EventsPage";

const NOW = Date.UTC(2026, 6, 24, 12, 0, 0);

function eventFixture(
  overrides: Partial<EventsPageEventItem> & { event_id: string },
): EventsPageEventItem {
  return {
    event_type: "gateway.notice",
    entity: "system",
    ts_unix_ms: NOW - 3 * 60_000,
    payload: { summary: "fixture" },
    ...overrides,
  };
}

/** One event per domain chip, plus a heartbeat the parent may pass through. */
function domainFixtures(): EventsPageEventItem[] {
  return [
    eventFixture({
      event_id: "evt-board",
      event_type: "board.card.created",
      entity: "board",
      payload: { action: "created", title: "Ship the basement" },
    }),
    eventFixture({
      event_id: "evt-job",
      event_type: "job.updated",
      entity: "job",
      payload: { job_id: "job-42" },
    }),
    eventFixture({
      event_id: "evt-approval",
      event_type: "approval.resolved",
      entity: "approval",
      payload: { decision: "approved" },
    }),
    eventFixture({
      event_id: "evt-channel",
      event_type: "channel.message",
      entity: "channel",
      payload: { summary: "channel says hi" },
    }),
    eventFixture({
      event_id: "evt-mail",
      event_type: "agent_mail.delivered",
      entity: "mail",
      payload: { summary: "mail moved" },
    }),
  ];
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  vi.useFakeTimers({ now: NOW });
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
  localStorage.clear();
  vi.useRealTimers();
});

async function render(
  visibleEvents: EventsPageEventItem[],
  options?: {
    showRawEvents?: boolean;
    onShowRawEventsChange?: (next: boolean) => void;
  },
) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(
      <EventsPage
        showRawEvents={options?.showRawEvents ?? false}
        onShowRawEventsChange={options?.onShowRawEventsChange ?? (() => {})}
        visibleEvents={visibleEvents}
      />,
    );
  });
}

function chip(label: string): HTMLButtonElement | undefined {
  return Array.from(
    container.querySelectorAll<HTMLButtonElement>(
      ".mc-event-filters .mc-filter-chip",
    ),
  ).find((button) => button.textContent === label);
}

function rows(): HTMLElement[] {
  return Array.from(container.querySelectorAll(".mc-event-item"));
}

function paginationInfo(): string | null {
  return (
    container.querySelector(".mc-pagination-info")?.textContent ?? null
  );
}

async function click(element: Element | undefined | null) {
  expect(element, "expected element to click").toBeTruthy();
  await act(async () => (element as HTMLElement).click());
}

describe("EventsPage parity", () => {
  it("filters each of the six domains through real chips with pressed state", async () => {
    await render(domainFixtures());

    const labels = Array.from(
      container.querySelectorAll(".mc-event-filters .mc-filter-chip"),
    ).map((button) => button.textContent);
    expect(labels).toEqual([
      "All",
      "Board",
      "Job",
      "Approval",
      "Channel",
      "Mail",
    ]);
    expect(chip("All")?.getAttribute("aria-pressed")).toBe("true");
    expect(rows()).toHaveLength(5);

    const byChip: Array<[string, string]> = [
      ["Board", "board.card.created"],
      ["Job", "job.updated"],
      ["Approval", "approval.resolved"],
      ["Channel", "channel.message"],
      ["Mail", "agent_mail.delivered"],
    ];
    for (const [label, expectedType] of byChip) {
      await click(chip(label));
      expect(chip(label)?.getAttribute("aria-pressed")).toBe("true");
      expect(chip("All")?.getAttribute("aria-pressed")).toBe("false");
      const visible = rows();
      expect(visible).toHaveLength(1);
      expect(visible[0]?.textContent).toContain(expectedType);
    }

    await click(chip("All"));
    expect(rows()).toHaveLength(5);
  });

  it("presents summaries, entities, domain tones, and deterministic relative time", async () => {
    await render(domainFixtures());

    const board = rows().find((row) =>
      row.textContent?.includes("board.card.created"),
    );
    expect(board?.className).toContain("mc-event-domain-accent");
    expect(board?.textContent).toContain("Card created: Ship the basement");
    expect(board?.textContent).toContain("board");
    expect(board?.querySelector(".mc-event-time")?.textContent).toBe("3m ago");
    expect(
      board?.querySelector(".mc-event-time")?.getAttribute("title"),
    ).toBeTruthy();

    const job = rows().find((row) => row.textContent?.includes("job.updated"));
    expect(job?.className).toContain("mc-event-domain-info");
    expect(job?.textContent).toContain("job-42");
    const approval = rows().find((row) =>
      row.textContent?.includes("approval.resolved"),
    );
    expect(approval?.className).toContain("mc-event-domain-warn");
    expect(approval?.textContent).toContain("approved");
  });

  it("drives heartbeat visibility through the real callback seam", async () => {
    const onShowRawEventsChange = vi.fn();
    await render(domainFixtures(), { onShowRawEventsChange });
    const checkbox = container.querySelector<HTMLInputElement>(
      ".mc-checkbox input[type=checkbox]",
    );
    expect(checkbox?.checked).toBe(false);

    await act(async () => {
      checkbox!.click();
    });
    expect(onShowRawEventsChange).toHaveBeenCalledWith(true);

    await render(domainFixtures(), {
      showRawEvents: true,
      onShowRawEventsChange,
    });
    const checked = container.querySelector<HTMLInputElement>(
      ".mc-checkbox input[type=checkbox]",
    );
    expect(checked?.checked).toBe(true);
    await act(async () => {
      checked!.click();
    });
    expect(onShowRawEventsChange).toHaveBeenLastCalledWith(false);
  });

  it("keeps both empty states honest and restores everything via Clear filter", async () => {
    await render([]);
    expect(container.textContent).toContain("Listening for events…");
    expect(container.querySelector(".mc-empty-drawer")).toBeNull();

    await render(domainFixtures().filter((e) => e.event_id !== "evt-mail"));
    await click(chip("Mail"));
    expect(container.textContent).toContain(
      "No events match your current filter.",
    );
    const clear = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Clear filter",
    );
    await click(clear);
    expect(chip("All")?.getAttribute("aria-pressed")).toBe("true");
    expect(rows()).toHaveLength(4);
  });

  it("paginates at exactly twelve items with honest boundaries", async () => {
    const events = Array.from({ length: 25 }, (_, index) =>
      eventFixture({
        event_id: `evt-${String(index + 1).padStart(2, "0")}`,
        event_type: "job.updated",
        payload: { job_id: `job-${index + 1}` },
      }),
    );
    await render(events);

    expect(rows()).toHaveLength(12);
    expect(rows()[0]?.textContent).toContain("job-1");
    expect(rows()[11]?.textContent).toContain("job-12");
    expect(paginationInfo()).toBe("1 / 3");
    const prev = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Prev",
    );
    const next = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Next",
    );
    expect((prev as HTMLButtonElement).disabled).toBe(true);

    await click(next);
    expect(paginationInfo()).toBe("2 / 3");
    expect(rows()[0]?.textContent).toContain("job-13");
    expect(rows()[11]?.textContent).toContain("job-24");

    await click(next);
    expect(paginationInfo()).toBe("3 / 3");
    expect(rows()).toHaveLength(1);
    expect(rows()[0]?.textContent).toContain("job-25");
    expect(
      (
        Array.from(container.querySelectorAll("button")).find(
          (button) => button.textContent === "Next",
        ) as HTMLButtonElement
      ).disabled,
    ).toBe(true);

    // Filtering resets to the first page.
    await click(chip("Job"));
    expect(paginationInfo()).toBe("1 / 3");
    expect(rows()[0]?.textContent).toContain("job-1");
  });

  it("never leaves a later page blank when a live update shrinks the feed", async () => {
    const events = Array.from({ length: 25 }, (_, index) =>
      eventFixture({
        event_id: `evt-${String(index + 1).padStart(2, "0")}`,
        event_type: "job.updated",
        payload: { job_id: `job-${index + 1}` },
      }),
    );
    await render(events);
    const next = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Next",
    );
    await click(next);
    await click(
      Array.from(container.querySelectorAll("button")).find(
        (button) => button.textContent === "Next",
      ),
    );
    expect(paginationInfo()).toBe("3 / 3");

    // The retained window shrinks under the current page: rows must clamp to
    // the last real page, not vanish.
    await render(events.slice(0, 13));
    expect(rows()).toHaveLength(1);
    expect(rows()[0]?.textContent).toContain("job-13");
    expect(paginationInfo()).toBe("2 / 2");

    await render(events.slice(0, 5));
    expect(rows()).toHaveLength(5);
    expect(container.querySelector(".mc-pagination")).toBeNull();

    // The clamp is real state, not just a temporary rendering trick: later
    // growth stays on page one instead of resurrecting the stale page three.
    await render(events.slice(0, 13));
    expect(paginationInfo()).toBe("1 / 2");
    expect(rows()).toHaveLength(12);
    expect(rows()[0]?.textContent).toContain("job-1");
    await render(events);
    expect(paginationInfo()).toBe("1 / 3");
    expect(rows()[0]?.textContent).toContain("job-1");
  });

  it("expands and collapses JSON with recursive secret redaction applied", async () => {
    await render([
      eventFixture({
        event_id: "evt-secret",
        event_type: "channel.message",
        payload: {
          api_key: "sk-live-very-secret",
          note: "Bearer abc.def.ghi",
          nested: { client_secret: "nested-secret" },
          list: [{ setup_token: "listed-secret" }],
          safe: "public-fact",
        },
      }),
    ]);

    const toggle = container.querySelector(".mc-event-expand");
    expect(toggle?.textContent).toContain("JSON");
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    expect(container.querySelector(".mc-event-payload")).toBeNull();

    await click(toggle);
    const payload = container.querySelector(".mc-event-payload");
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
    expect(payload?.textContent).toContain("[REDACTED]");
    expect(payload?.textContent).toContain("public-fact");
    expect(payload?.textContent).not.toContain("sk-live-very-secret");
    expect(payload?.textContent).not.toContain("abc.def.ghi");
    expect(payload?.textContent).not.toContain("nested-secret");
    expect(payload?.textContent).not.toContain("listed-secret");

    await click(container.querySelector(".mc-event-expand"));
    expect(container.querySelector(".mc-event-payload")).toBeNull();
    expect(
      container.querySelector(".mc-event-expand")?.getAttribute("aria-expanded"),
    ).toBe("false");
  });

  it("offers the Event stream pin without mutating filter, page, or disclosure state", async () => {
    const onShowRawEventsChange = vi.fn();
    const events = Array.from({ length: 25 }, (_, index) =>
      eventFixture({
        event_id: `evt-${String(index + 1).padStart(2, "0")}`,
        event_type: "job.updated",
        payload: { job_id: `job-${index + 1}` },
      }),
    );
    await render(events, { onShowRawEventsChange });

    await click(chip("Job"));
    await click(
      Array.from(container.querySelectorAll("button")).find(
        (button) => button.textContent === "Next",
      ),
    );
    await click(rows()[0]?.querySelector(".mc-event-expand"));
    expect(paginationInfo()).toBe("2 / 3");

    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) =>
        button.getAttribute("aria-label") === "Pin Event stream to Office",
    );
    expect(pin).toBeTruthy();
    await click(pin);
    expect(
      container.querySelector('[role="status"]')?.textContent,
    ).toContain("On the Office canvas");

    // Zero feed-surface mutations: filter, page, disclosure, and the
    // heartbeat seam are exactly where the operator left them.
    expect(chip("Job")?.getAttribute("aria-pressed")).toBe("true");
    expect(paginationInfo()).toBe("2 / 3");
    expect(
      rows()[0]?.querySelector(".mc-event-expand")?.getAttribute(
        "aria-expanded",
      ),
    ).toBe("true");
    expect(onShowRawEventsChange).not.toHaveBeenCalled();
  });
});
