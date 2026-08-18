/**
 * The guided tour is elevator-stop-by-stop guidance sourced from the
 * resolved floor registry. Steps are derived data: hidden or
 * capability-disabled floors and rooms are already absent from the
 * resolved registry, so they can never become tour stops, and each floor
 * stop lands by stable room id — never by tab label or array position.
 */

import type { FloorDef } from "./floors";

export interface GuidedTourRegistryStep {
  id: string;
  targetId: string;
  title: string;
  body: string;
  /** Stable room id to land on before the spotlight moves; shell stops omit it. */
  roomId?: string;
}

/** Owner-language lead sentence per known floor; the registry supplies the rest. */
const FLOOR_LEADS: Record<string, string> = {
  office:
    "Start your day here. Read the briefing, ask for what you need, decide the few things waiting on you, and watch delegated work move to Done.",
  window:
    "Glance at the floor without interrupting anyone. The Reef shows who is busy right now, and Office Chatter follows safe notes about work in motion.",
  trenches:
    "Where the day-to-day work lives: boards for tasks, the calendar for scheduled jobs, plans, your staff, and the receipts for finished work.",
  basement:
    "The building's machinery. Come down here to connect services, choose models, clear breakers, check memory, and change Setup or Policy.",
};

function floorLead(floor: FloorDef): string {
  const lead = FLOOR_LEADS[floor.id];
  if (lead) return lead;
  return floor.hint
    ? `${floor.label} — ${floor.hint}.`
    : `Take the elevator to ${floor.label}.`;
}

function floorBody(floor: FloorDef): string {
  const lead = floorLead(floor);
  if (floor.rooms.length <= 1) return lead;
  const stops = floor.rooms.map((room) => room.label).join(", ");
  return `${lead} Stops on this floor: ${stops}.`;
}

const SHELL_STEPS: GuidedTourRegistryStep[] = [
  {
    id: "help",
    targetId: "nav-help-shortcut",
    title: "Help & Docs",
    body: "Every floor is also explained in plain language here. The tour and the elevator are never the only way in — Help/Docs describes each page directly.",
  },
  {
    id: "config",
    targetId: "nav-config",
    title: "Config",
    body: "Reconnect the gateway, rerun setup, restart this tour, and turn optional pages on or off. The Basement's Setup room offers the same controls.",
  },
  {
    id: "command",
    targetId: "topbar-command",
    title: "Command palette",
    body: "Press Cmd/Ctrl + K to jump anywhere without riding the elevator.",
  },
];

/**
 * Build tour steps from the exact resolved elevator registry rendered to
 * the user. Floors with no remaining rooms are skipped: a stop with no
 * doors is not a stop.
 */
export function buildGuidedTourSteps(
  floors: readonly FloorDef[],
): GuidedTourRegistryStep[] {
  const floorSteps = floors
    .filter((floor) => floor.rooms.length > 0)
    .map((floor) => {
      const defaultRoom =
        floor.rooms.find((room) => room.id === floor.defaultRoom) ??
        floor.rooms[0];
      return {
        id: `floor-${floor.id}`,
        targetId: `floor-${floor.id}`,
        title: `${floor.lamp}F · ${floor.label}`,
        body: floorBody(floor),
        roomId: defaultRoom.id,
      };
    });
  return [...floorSteps, ...SHELL_STEPS];
}
