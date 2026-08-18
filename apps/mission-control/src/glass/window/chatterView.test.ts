import { describe, expect, it } from "vitest";

import {
  formatChatterTime,
  groupChatterMessages,
  roomHasUnread,
  sortRoomsByActivity,
} from "./chatterView";
import type { OfficeChatterMessage, OfficeChatterRoom } from "./types";

const NOW = 1_800_000_000_000;

function room(overrides: Partial<OfficeChatterRoom> = {}): OfficeChatterRoom {
  return {
    thread_id: "t-1",
    workstream_id: "w-1",
    label: "launch",
    unread_count: null,
    last_activity_at_ms: null,
    ...overrides,
  };
}

function message(
  overrides: Partial<OfficeChatterMessage> = {},
): OfficeChatterMessage {
  return {
    message_id: "m-1",
    thread_id: "t-1",
    author: { kind: "execass", display_name: "ExecAss" },
    text: "The launch brief moved into active work.",
    created_at_ms: NOW - 60_000,
    source: {
      kind: "execass_event",
      event_name: "execass.v1.delegation.transitioned",
      workstream_id: "w-1",
      revision: 1,
    },
    ...overrides,
  };
}

describe("sortRoomsByActivity", () => {
  it("orders by most recent activity with quiet rooms last, stably", () => {
    const rooms = [
      room({ thread_id: "quiet-a" }),
      room({ thread_id: "old", last_activity_at_ms: NOW - 60_000 }),
      room({ thread_id: "quiet-b" }),
      room({ thread_id: "new", last_activity_at_ms: NOW - 1_000 }),
    ];
    expect(sortRoomsByActivity(rooms).map((entry) => entry.thread_id)).toEqual([
      "new",
      "old",
      "quiet-a",
      "quiet-b",
    ]);
    expect(rooms[0]?.thread_id).toBe("quiet-a");
  });
});

describe("roomHasUnread", () => {
  it("is quiet unless the server reports a positive count", () => {
    expect(roomHasUnread(room({ unread_count: null }))).toBe(false);
    expect(roomHasUnread(room({ unread_count: 0 }))).toBe(false);
    expect(roomHasUnread(room({ unread_count: 3 }))).toBe(true);
  });
});

describe("groupChatterMessages", () => {
  it("groups consecutive messages from the same author", () => {
    const groups = groupChatterMessages([
      message({ message_id: "m-1" }),
      message({ message_id: "m-2", text: "Section 3 drafted." }),
      message({
        message_id: "m-3",
        author: { kind: "owner", display_name: "You" },
        text: "Keep the caterer on hold.",
      }),
      message({ message_id: "m-4", text: "Holding the caterer." }),
    ]);
    expect(groups.map((group) => group.author.display_name)).toEqual([
      "ExecAss",
      "You",
      "ExecAss",
    ]);
    expect(groups[0]?.messages.map((entry) => entry.message_id)).toEqual([
      "m-1",
      "m-2",
    ]);
    expect(groups[0]?.startedAtMs).toBe(NOW - 60_000);
  });

  it("returns no groups for no messages", () => {
    expect(groupChatterMessages([])).toEqual([]);
  });
});

describe("formatChatterTime", () => {
  it("shows a clock time for today and a short date otherwise", () => {
    const today = formatChatterTime(NOW - 60_000, NOW);
    expect(today).toMatch(/\d{1,2}:\d{2}/);
    const lastWeek = formatChatterTime(NOW - 7 * 24 * 60 * 60_000, NOW);
    expect(lastWeek).not.toMatch(/^\d{1,2}:\d{2}/);
    expect(lastWeek.length).toBeGreaterThan(0);
  });
});
