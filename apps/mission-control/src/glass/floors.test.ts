import { describe, expect, test } from "vitest";

import {
  DEFAULT_FLOORS,
  floorForShortcut,
  resolveElevator,
  type FloorDef,
} from "./floors";

const CAPS_ALL = ["execass", "boards", "calendar", "agent-mail", "gateway"];

describe("DEFAULT_FLOORS registry", () => {
  test("ships the four Glass Office floors in elevator order with stable ids", () => {
    const ids = DEFAULT_FLOORS.map((f) => f.id);
    expect(ids).toEqual(["office", "window", "trenches", "basement"]);
  });

  test("every floor and room declares the complete shell descriptor", () => {
    for (const floor of DEFAULT_FLOORS) {
      expect(floor.lamp.length).toBeGreaterThan(0);
      expect(floor.icon.length).toBeGreaterThan(0);
      expect(floor.label.length).toBeGreaterThan(0);
      expect(floor.shortcut.length).toBeGreaterThan(0);
      expect(floor.rooms.length).toBeGreaterThan(0);
      for (const room of floor.rooms) {
        expect(room.floorId).toBe(floor.id);
        expect(room.route.length).toBeGreaterThan(0);
        expect(Array.isArray(room.blocks)).toBe(true);
      }
    }
  });

  test("default room resolves to a declared room on every floor", () => {
    for (const floor of DEFAULT_FLOORS) {
      const roomIds = floor.rooms.map((r) => r.id);
      const resolved = floor.defaultRoom ?? floor.rooms[0]?.id;
      expect(roomIds).toContain(resolved);
    }
  });

  test("keeps every routable product surface reachable from registry data", () => {
    const routes = new Set(
      DEFAULT_FLOORS.flatMap((floor) => floor.rooms.map((room) => room.route)),
    );
    expect(routes).toEqual(
      new Set([
        "assistant",
        "boards",
        "calendar",
        "chatrooms",
        "cockpit",
        "connectors",
        "events",
        "focus",
        "mail",
        "memory",
        "runbook",
        "strategy",
        "team",
        "window",
      ]),
    );
  });

  test("keeps distinct room identities even when they share a product surface", () => {
    const window = DEFAULT_FLOORS.find((floor) => floor.id === "window");
    const basement = DEFAULT_FLOORS.find((floor) => floor.id === "basement");
    expect(window?.rooms.map((room) => room.id)).toEqual(["reef", "chatter"]);
    expect(basement?.defaultRoom).toBe("setup");
    expect(basement?.rooms.find((room) => room.id === "setup")?.route).toBe(
      "connectors",
    );
  });
});

describe("resolveElevator", () => {
  test("returns floors sorted by order for a fully capable context", () => {
    const resolved = resolveElevator(DEFAULT_FLOORS, { capabilities: CAPS_ALL });
    expect(resolved.map((f) => f.id)).toEqual([
      "office",
      "window",
      "trenches",
      "basement",
    ]);
  });

  test("hides a floor whose required capability is missing", () => {
    const floors: FloorDef[] = [
      ...DEFAULT_FLOORS,
      {
        id: "penthouse",
        lamp: "5",
        label: "The Penthouse",
        shortcut: "5",
        order: 5,
        icon: "sparkles",
        rooms: [{ id: "spa", floorId: "penthouse", label: "Spa", route: "cockpit", blocks: [] }],
        requiresCapabilities: ["jacuzzi"],
      },
    ];
    const resolved = resolveElevator(floors, { capabilities: CAPS_ALL });
    expect(resolved.map((f) => f.id)).not.toContain("penthouse");
  });

  test("config overrides can hide, rename, and reorder floors without mutating the registry", () => {
    const resolved = resolveElevator(DEFAULT_FLOORS, {
      capabilities: CAPS_ALL,
      overrides: {
        window: { hidden: true },
        trenches: { label: "The Pit", order: 0 },
      },
    });
    expect(resolved.map((f) => f.id)).toEqual(["trenches", "office", "basement"]);
    expect(resolved[0]?.label).toBe("The Pit");
    // registry untouched
    expect(DEFAULT_FLOORS.find((f) => f.id === "trenches")?.label).not.toBe(
      "The Pit",
    );
    expect(DEFAULT_FLOORS.map((f) => f.id)).toContain("window");
  });

  test("filters unavailable rooms and moves a hidden default to a reachable room", () => {
    const resolved = resolveElevator(DEFAULT_FLOORS, {
      capabilities: ["execass"],
    });
    const window = resolved.find((floor) => floor.id === "window");
    expect(window?.rooms.map((room) => room.id)).toEqual(["reef"]);
    expect(window?.defaultRoom).toBe("reef");
  });

  test("preserves every visible room instead of deduplicating equal routes", () => {
    const resolved = resolveElevator(DEFAULT_FLOORS, { capabilities: CAPS_ALL });
    const window = resolved.find((floor) => floor.id === "window");
    const basement = resolved.find((floor) => floor.id === "basement");
    expect(window?.rooms.map((room) => room.id)).toEqual(["reef", "chatter"]);
    expect(basement?.rooms.map((room) => room.id)).toContain("setup");
  });
});

describe("floorForShortcut", () => {
  test("maps the elevator keys to their floors, case-insensitively", () => {
    const resolved = resolveElevator(DEFAULT_FLOORS, { capabilities: CAPS_ALL });
    expect(floorForShortcut(resolved, "4")?.id).toBe("office");
    expect(floorForShortcut(resolved, "3")?.id).toBe("window");
    expect(floorForShortcut(resolved, "2")?.id).toBe("trenches");
    expect(floorForShortcut(resolved, "b")?.id).toBe("basement");
    expect(floorForShortcut(resolved, "B")?.id).toBe("basement");
  });

  test("returns undefined for keys no visible floor claims", () => {
    const resolved = resolveElevator(DEFAULT_FLOORS, {
      capabilities: CAPS_ALL,
      overrides: { window: { hidden: true } },
    });
    expect(floorForShortcut(resolved, "3")).toBeUndefined();
    expect(floorForShortcut(resolved, "9")).toBeUndefined();
  });
});
