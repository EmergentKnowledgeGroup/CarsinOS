/**
 * Office Chatter presentation helpers. Chatter is a read-first projection
 * of Agent Mail: rooms order themselves by real activity, unreads are a
 * quiet fact (never a computed claim), and grouping only folds consecutive
 * messages - it never reorders or merges across authors.
 */

import type { OfficeChatterMessage, OfficeChatterRoom } from "./types";

export function sortRoomsByActivity(
  rooms: readonly OfficeChatterRoom[],
): OfficeChatterRoom[] {
  return [...rooms].sort((a, b) => {
    const left = a.last_activity_at_ms ?? Number.NEGATIVE_INFINITY;
    const right = b.last_activity_at_ms ?? Number.NEGATIVE_INFINITY;
    return right - left;
  });
}

export function roomHasUnread(room: OfficeChatterRoom): boolean {
  return typeof room.unread_count === "number" && room.unread_count > 0;
}

export interface ChatterMessageGroup {
  author: OfficeChatterMessage["author"];
  startedAtMs: number;
  messages: OfficeChatterMessage[];
}

export function groupChatterMessages(
  messages: readonly OfficeChatterMessage[],
): ChatterMessageGroup[] {
  const groups: ChatterMessageGroup[] = [];
  for (const message of messages) {
    const last = groups[groups.length - 1];
    if (
      last &&
      last.author.kind === message.author.kind &&
      last.author.display_name === message.author.display_name
    ) {
      last.messages.push(message);
      continue;
    }
    groups.push({
      author: message.author,
      startedAtMs: message.created_at_ms,
      messages: [message],
    });
  }
  return groups;
}

export function formatChatterTime(atMs: number, nowMs: number): string {
  const at = new Date(atMs);
  const now = new Date(nowMs);
  const sameDay =
    at.getFullYear() === now.getFullYear() &&
    at.getMonth() === now.getMonth() &&
    at.getDate() === now.getDate();
  if (sameDay) {
    return at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  }
  return at.toLocaleDateString([], { month: "short", day: "numeric" });
}
