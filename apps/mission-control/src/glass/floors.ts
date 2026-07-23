/**
 * The elevator is a registry renderer, not hardcoded route branches.
 * Floors and rooms are data: stable ids with data-driven label, order,
 * lamp, shortcut, default room, capability requirements, and visibility.
 */

import type { MissionControlTab } from "../app/useAppController";

export interface RoomDef {
  id: string;
  label: string;
  floorId: string;
  route: MissionControlTab;
  capabilityRequirements?: string[];
  blocks: string[];
}

export interface FloorDef {
  id: string;
  /** Elevator lamp text, e.g. "4" or "B". */
  lamp: string;
  icon: string;
  label: string;
  hint?: string;
  /** Bare keyboard key that jumps to this floor. */
  shortcut: string;
  order: number;
  rooms: RoomDef[];
  defaultRoom?: string;
  requiresCapabilities?: string[];
  hidden?: boolean;
}

export interface FloorOverride {
  hidden?: boolean;
  label?: string;
  order?: number;
}

export interface ElevatorContext {
  capabilities: readonly string[];
  overrides?: Partial<Record<string, FloorOverride>>;
}

export const DEFAULT_FLOORS: FloorDef[] = [
  {
    id: "office",
    lamp: "4",
    icon: "bot",
    label: "The Office",
    hint: "brief · delegate · decide",
    shortcut: "4",
    order: 1,
    rooms: [
      { id: "desk", floorId: "office", label: "Your desk", route: "assistant", blocks: ["briefing", "ask", "needs-you", "in-motion", "done", "next"] },
    ],
    defaultRoom: "desk",
    requiresCapabilities: ["execass"],
  },
  {
    id: "window",
    lamp: "3",
    icon: "waves",
    label: "The Window",
    hint: "the floor · the chatter",
    shortcut: "3",
    order: 2,
    rooms: [
      { id: "reef", floorId: "window", label: "Reef", route: "window", blocks: ["presence"] },
      { id: "chatter", floorId: "window", label: "Office chatter", route: "window", capabilityRequirements: ["agent-mail"], blocks: ["chatter"] },
    ],
    defaultRoom: "reef",
  },
  {
    id: "trenches",
    lamp: "2",
    icon: "kanban",
    label: "The Trenches",
    hint: "boards · calendar · staff",
    shortcut: "2",
    order: 3,
    rooms: [
      { id: "boards", floorId: "trenches", label: "Boards", route: "boards", blocks: ["boards"] },
      { id: "calendar", floorId: "trenches", label: "Calendar", route: "calendar", blocks: ["calendar"] },
      { id: "plan", floorId: "trenches", label: "Plan", route: "strategy", blocks: ["strategy"] },
      { id: "staff", floorId: "trenches", label: "Staff Directory", route: "team", blocks: ["staff"] },
      { id: "history", floorId: "trenches", label: "History & Receipts", route: "runbook", blocks: ["history"] },
    ],
    defaultRoom: "boards",
  },
  {
    id: "basement",
    lamp: "B",
    icon: "cable",
    label: "The Basement",
    hint: "machinery · setup",
    shortcut: "b",
    order: 4,
    rooms: [
      { id: "connectors", floorId: "basement", label: "Connectors", route: "connectors", blocks: ["connectors"] },
      { id: "models", floorId: "basement", label: "Models & Providers", route: "team", blocks: ["models"] },
      { id: "breakers", floorId: "basement", label: "Breakers & Scheduler", route: "focus", blocks: ["breakers"] },
      { id: "cockpit", floorId: "basement", label: "Cockpit", route: "cockpit", blocks: ["cockpit"] },
      { id: "events", floorId: "basement", label: "Event stream", route: "events", blocks: ["events"] },
      { id: "directory", floorId: "basement", label: "Directory / Front Desk", route: "mail", blocks: ["directory"] },
      { id: "rooms", floorId: "basement", label: "Agent rooms", route: "chatrooms", blocks: ["rooms"] },
      { id: "memory", floorId: "basement", label: "Memory plant", route: "memory", blocks: ["memory"] },
      { id: "setup", floorId: "basement", label: "Setup", route: "connectors", blocks: ["setup"] },
    ],
    defaultRoom: "setup",
  },
];

export function resolveElevator(
  floors: readonly FloorDef[],
  ctx?: ElevatorContext,
): FloorDef[] {
  const capabilities = new Set(ctx?.capabilities ?? []);
  const overrides = ctx?.overrides ?? {};
  return floors
    .map((floor) => {
      const override = overrides[floor.id];
      const resolved = override ? { ...floor, ...override } : { ...floor };
      const rooms = resolved.rooms.filter((room) =>
        (room.capabilityRequirements ?? []).every((capability) =>
          capabilities.has(capability),
        ),
      );
      const defaultRoom = rooms.some((room) => room.id === resolved.defaultRoom)
        ? resolved.defaultRoom
        : rooms[0]?.id;
      return { ...resolved, rooms, defaultRoom };
    })
    .filter((floor) => {
      if (floor.hidden) return false;
      const required = floor.requiresCapabilities ?? [];
      return (
        required.every((cap) => capabilities.has(cap)) &&
        floor.rooms.length > 0
      );
    })
    .sort((a, b) => a.order - b.order);
}

export function floorForShortcut(
  resolvedFloors: readonly FloorDef[],
  key: string,
): FloorDef | undefined {
  const wanted = key.toLowerCase();
  return resolvedFloors.find(
    (floor) => floor.shortcut.toLowerCase() === wanted,
  );
}
