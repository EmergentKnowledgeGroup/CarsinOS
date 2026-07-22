/**
 * The elevator is a registry renderer, not hardcoded route branches.
 * Floors and rooms are data: stable ids with data-driven label, order,
 * lamp, shortcut, default room, capability requirements, and visibility.
 */

export interface RoomDef {
  id: string;
  label: string;
}

export interface FloorDef {
  id: string;
  /** Elevator lamp text, e.g. "4" or "B". */
  lamp: string;
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
    label: "The Office",
    hint: "brief · delegate · decide",
    shortcut: "4",
    order: 1,
    rooms: [
      { id: "desk", label: "Your desk" },
      { id: "assistant-desk", label: "The Assistant's Desk" },
    ],
    defaultRoom: "desk",
    requiresCapabilities: ["execass"],
  },
  {
    id: "window",
    lamp: "3",
    label: "The Window",
    hint: "the floor · the chatter",
    shortcut: "3",
    order: 2,
    rooms: [
      { id: "reef", label: "Reef" },
      { id: "chatter", label: "Office chatter" },
    ],
    defaultRoom: "reef",
  },
  {
    id: "trenches",
    lamp: "2",
    label: "The Trenches",
    hint: "boards · calendar · staff",
    shortcut: "2",
    order: 3,
    rooms: [
      { id: "boards", label: "Boards" },
      { id: "calendar", label: "Calendar" },
      { id: "plan", label: "Plan" },
      { id: "staff", label: "Staff Directory" },
      { id: "history", label: "History & Receipts" },
    ],
    defaultRoom: "boards",
  },
  {
    id: "basement",
    lamp: "B",
    label: "The Basement",
    hint: "machinery · setup",
    shortcut: "b",
    order: 4,
    rooms: [
      { id: "connectors", label: "Connectors" },
      { id: "models", label: "Models & Providers" },
      { id: "breakers", label: "Breakers & Scheduler" },
      { id: "events", label: "Event stream" },
      { id: "directory", label: "Directory / Front Desk" },
      { id: "memory", label: "Memory plant" },
      { id: "setup", label: "Setup" },
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
      return override ? { ...floor, ...override } : { ...floor };
    })
    .filter((floor) => {
      if (floor.hidden) return false;
      const required = floor.requiresCapabilities ?? [];
      return required.every((cap) => capabilities.has(cap));
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
