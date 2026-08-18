// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import {
  GLASS_CONFIG_EVENT,
  GLASS_CONFIG_STORAGE_KEY,
  loadGlassConfig,
} from "../../glass/config";
import { OFFICE_BLOCK_REGISTRY } from "./officeBlocks";
import { pinRoomBlocksToOffice } from "./pinToOffice";

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
});

describe("OFFICE_BLOCK_REGISTRY room shortcuts", () => {
  test("registers the Boards room shortcut hidden by default", () => {
    const boards = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "boards");
    expect(boards).toBeDefined();
    expect(boards?.rendererKey).toBe("room-shortcut");
    expect(boards?.roomId).toBe("boards");
    expect(boards?.defaultVisible).toBe(false);
  });

  test("registers the Calendar room shortcut hidden by default", () => {
    const calendar = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "calendar");
    expect(calendar).toBeDefined();
    expect(calendar?.rendererKey).toBe("room-shortcut");
    expect(calendar?.roomId).toBe("calendar");
    expect(calendar?.defaultVisible).toBe(false);
  });

  test("registers the Plan room shortcut under its declared strategy block id", () => {
    const strategy = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "strategy");
    expect(strategy).toBeDefined();
    expect(strategy?.rendererKey).toBe("room-shortcut");
    expect(strategy?.roomId).toBe("plan");
    expect(strategy?.defaultVisible).toBe(false);
  });

  test("registers the Staff Directory room shortcut hidden by default", () => {
    const staff = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "staff");
    expect(staff).toBeDefined();
    expect(staff?.rendererKey).toBe("room-shortcut");
    expect(staff?.roomId).toBe("staff");
    expect(staff?.defaultVisible).toBe(false);
  });

  test("registers the History & Receipts room shortcut hidden by default", () => {
    const history = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "history");
    expect(history).toBeDefined();
    expect(history?.rendererKey).toBe("room-shortcut");
    expect(history?.roomId).toBe("history");
    expect(history?.defaultVisible).toBe(false);
  });

  test("registers the Connectors room shortcut hidden by default", () => {
    const connectors = OFFICE_BLOCK_REGISTRY.find(
      (def) => def.id === "connectors",
    );
    expect(connectors).toBeDefined();
    expect(connectors?.rendererKey).toBe("room-shortcut");
    expect(connectors?.roomId).toBe("connectors");
    expect(connectors?.defaultVisible).toBe(false);
  });

  test("registers the Models & Providers room shortcut hidden by default", () => {
    const models = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "models");
    expect(models).toBeDefined();
    expect(models?.rendererKey).toBe("room-shortcut");
    expect(models?.roomId).toBe("models");
    expect(models?.defaultVisible).toBe(false);
  });

  test("registers the Event stream room shortcut hidden by default", () => {
    const events = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "events");
    expect(events).toBeDefined();
    expect(events?.rendererKey).toBe("room-shortcut");
    expect(events?.roomId).toBe("events");
    expect(events?.defaultVisible).toBe(false);
  });

  test("registers the Breakers & Scheduler room shortcut hidden by default", () => {
    const breakers = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "breakers");
    expect(breakers).toBeDefined();
    expect(breakers?.rendererKey).toBe("room-shortcut");
    expect(breakers?.roomId).toBe("breakers");
    expect(breakers?.defaultVisible).toBe(false);
  });

  test("registers the Directory / Front Desk room shortcut hidden by default", () => {
    const directory = OFFICE_BLOCK_REGISTRY.find(
      (def) => def.id === "directory",
    );
    expect(directory).toBeDefined();
    expect(directory?.rendererKey).toBe("room-shortcut");
    expect(directory?.roomId).toBe("directory");
    expect(directory?.defaultVisible).toBe(false);
  });

  test("registers the Memory plant room shortcut hidden by default", () => {
    const memory = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "memory");
    expect(memory).toBeDefined();
    expect(memory?.rendererKey).toBe("room-shortcut");
    expect(memory?.roomId).toBe("memory");
    expect(memory?.defaultVisible).toBe(false);
  });

  test("registers the File locks room shortcut hidden by default", () => {
    const locks = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "locks");
    expect(locks).toBeDefined();
    expect(locks?.rendererKey).toBe("room-shortcut");
    expect(locks?.roomId).toBe("locks");
    expect(locks?.defaultVisible).toBe(false);
  });

  test("registers the Setup room shortcut hidden by default", () => {
    const setup = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "setup");
    expect(setup).toBeDefined();
    expect(setup?.rendererKey).toBe("room-shortcut");
    expect(setup?.roomId).toBe("setup");
    expect(setup?.defaultSize).toBe("s");
    expect(setup?.defaultVisible).toBe(false);
  });

  test("registers the Policy room shortcut hidden by default", () => {
    const policy = OFFICE_BLOCK_REGISTRY.find((def) => def.id === "policy");
    expect(policy).toBeDefined();
    expect(policy?.rendererKey).toBe("room-shortcut");
    expect(policy?.roomId).toBe("policy");
    expect(policy?.defaultSize).toBe("s");
    expect(policy?.defaultVisible).toBe(false);
  });
});

describe("pinRoomBlocksToOffice", () => {
  test("pins the room's registered blocks as visible placements and persists them", () => {
    const result = pinRoomBlocksToOffice("boards");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["boards"]);
    const layout = loadGlassConfig().layout ?? [];
    const placement = layout.find((entry) => entry.id === "boards");
    expect(placement?.visible).toBe(true);
  });

  test("pins the Calendar room and both shortcuts fit the default canvas together", () => {
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    const result = pinRoomBlocksToOffice("calendar");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["calendar"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "calendar")?.visible).toBe(true);
    expect(layout.find((entry) => entry.id === "boards")?.visible).toBe(true);
  });

  test("is idempotent: pinning again succeeds without duplicating the placement", () => {
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    const again = pinRoomBlocksToOffice("boards");
    expect(again.ok).toBe(true);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.filter((entry) => entry.id === "boards")).toHaveLength(1);
  });

  test("announces the config change so live consumers re-read", () => {
    const listener = vi.fn();
    window.addEventListener(GLASS_CONFIG_EVENT, listener);
    try {
      pinRoomBlocksToOffice("boards");
      expect(listener).toHaveBeenCalled();
    } finally {
      window.removeEventListener(GLASS_CONFIG_EVENT, listener);
    }
  });

  test("fails closed for unknown rooms without touching config", () => {
    const result = pinRoomBlocksToOffice("haunted-room");
    expect(result.ok).toBe(false);
    expect(loadGlassConfig().layout).toBeUndefined();
  });

  test("pins the Plan room through its declared strategy block", () => {
    const result = pinRoomBlocksToOffice("plan");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["strategy"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "strategy")?.visible).toBe(true);
  });

  test("pins the Staff room through its staff block", () => {
    const result = pinRoomBlocksToOffice("staff");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["staff"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "staff")?.visible).toBe(true);
  });

  test("the fourth Trenches shortcut refuses honestly when the default canvas is full", () => {
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    expect(pinRoomBlocksToOffice("calendar").ok).toBe(true);
    expect(pinRoomBlocksToOffice("plan").ok).toBe(true);
    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("staff");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
  });

  test("all four Trenches shortcuts fit once the boss hides a default block", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: true },
          { id: "in-motion", size: "m", visible: true },
          { id: "done", size: "m", visible: true },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    expect(pinRoomBlocksToOffice("calendar").ok).toBe(true);
    expect(pinRoomBlocksToOffice("plan").ok).toBe(true);
    expect(pinRoomBlocksToOffice("staff").ok).toBe(true);
    const layout = loadGlassConfig().layout ?? [];
    for (const id of ["boards", "calendar", "strategy", "staff"]) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }
  });

  test("pins the History room through its history block", () => {
    const result = pinRoomBlocksToOffice("history");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["history"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "history")?.visible).toBe(true);
  });

  test("the fifth Trenches shortcut refuses honestly while only one block is freed", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: true },
          { id: "in-motion", size: "m", visible: true },
          { id: "done", size: "m", visible: true },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    expect(pinRoomBlocksToOffice("calendar").ok).toBe(true);
    expect(pinRoomBlocksToOffice("plan").ok).toBe(true);
    expect(pinRoomBlocksToOffice("staff").ok).toBe(true);
    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("history");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
  });

  test("all five Trenches shortcuts fit once the boss hides two default blocks", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: true },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: true },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    expect(pinRoomBlocksToOffice("calendar").ok).toBe(true);
    expect(pinRoomBlocksToOffice("plan").ok).toBe(true);
    expect(pinRoomBlocksToOffice("staff").ok).toBe(true);
    expect(pinRoomBlocksToOffice("history").ok).toBe(true);
    const layout = loadGlassConfig().layout ?? [];
    for (const id of ["boards", "calendar", "strategy", "staff", "history"]) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }
  });

  test("pins the Connectors room through its connectors block", () => {
    const result = pinRoomBlocksToOffice("connectors");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["connectors"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "connectors")?.visible).toBe(
      true,
    );
  });

  test("the Basement Connectors shortcut joins the freed canvas and repeats honestly", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: true },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: true },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    expect(pinRoomBlocksToOffice("calendar").ok).toBe(true);
    expect(pinRoomBlocksToOffice("plan").ok).toBe(true);
    expect(pinRoomBlocksToOffice("staff").ok).toBe(true);
    expect(pinRoomBlocksToOffice("history").ok).toBe(true);
    expect(pinRoomBlocksToOffice("connectors").ok).toBe(true);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "connectors")?.visible).toBe(
      true,
    );
    // Six shortcuts fill the freed canvas exactly; a seventh block would not
    // fit, so an already-pinned repeat must stay honest instead of moving.
    const beforeRepeat = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const again = pinRoomBlocksToOffice("connectors");
    expect(again.ok).toBe(true);
    expect(again.pinned).toEqual([]);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(beforeRepeat);
    expect(
      (loadGlassConfig().layout ?? []).filter(
        (entry) => entry.id === "connectors",
      ),
    ).toHaveLength(1);
  });

  test("pins the Models room through its models block", () => {
    const result = pinRoomBlocksToOffice("models");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["models"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "models")?.visible).toBe(true);
  });

  test("pins the Event stream room through its events block", () => {
    const result = pinRoomBlocksToOffice("events");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["events"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "events")?.visible).toBe(true);
  });

  test("pins the Breakers & Scheduler room through its breakers block", () => {
    const result = pinRoomBlocksToOffice("breakers");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["breakers"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "breakers")?.visible).toBe(true);
  });

  test("the ninth shortcut refuses byte-for-byte when all eight earlier shortcuts fill the canvas", () => {
    // needs-you (l = 8 cells) plus eight s shortcuts (16 cells) fill the
    // 6x4 canvas exactly; every earlier room shortcut is genuinely pinned.
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: true },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    for (const roomId of [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
    ]) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    for (const id of [
      "boards",
      "calendar",
      "strategy",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
    ]) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }

    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("breakers");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
    expect(
      (loadGlassConfig().layout ?? []).find((entry) => entry.id === "breakers")
        ?.visible,
    ).not.toBe(true);
  });

  test("the Breakers shortcut joins a fully freed canvas and repeats honestly", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    for (const roomId of [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
    ]) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "breakers")?.visible).toBe(true);

    const beforeRepeat = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const again = pinRoomBlocksToOffice("breakers");
    expect(again.ok).toBe(true);
    expect(again.pinned).toEqual([]);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(beforeRepeat);
    expect(
      (loadGlassConfig().layout ?? []).filter(
        (entry) => entry.id === "breakers",
      ),
    ).toHaveLength(1);
  });

  test("pins the Directory room through its directory block", () => {
    const result = pinRoomBlocksToOffice("directory");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["directory"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "directory")?.visible).toBe(
      true,
    );
  });

  test("the tenth shortcut refuses byte-for-byte when all nine earlier shortcuts fill the canvas", () => {
    // in-motion (m = 4 cells) plus next (s = 2 cells) plus nine s shortcuts
    // (18 cells) fill the 6x4 canvas exactly; every earlier room shortcut is
    // genuinely pinned before Directory asks for a slot.
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: true },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: true },
        ],
      }),
    );
    const earlierRooms = [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
    ];
    for (const roomId of earlierRooms) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    for (const id of [
      "boards",
      "calendar",
      "strategy",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
    ]) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }

    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("directory");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
    expect(
      (loadGlassConfig().layout ?? []).find(
        (entry) => entry.id === "directory",
      )?.visible,
    ).not.toBe(true);
  });

  test("the Directory shortcut joins a fully freed canvas and repeats honestly", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    for (const roomId of [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
    ]) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "directory")?.visible).toBe(
      true,
    );

    const beforeRepeat = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const again = pinRoomBlocksToOffice("directory");
    expect(again.ok).toBe(true);
    expect(again.pinned).toEqual([]);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(beforeRepeat);
    expect(
      (loadGlassConfig().layout ?? []).filter(
        (entry) => entry.id === "directory",
      ),
    ).toHaveLength(1);
  });

  test("pins the Memory room through its memory block", () => {
    const result = pinRoomBlocksToOffice("memory");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["memory"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "memory")?.visible).toBe(true);
  });

  test("the eleventh shortcut refuses byte-for-byte when all ten earlier shortcuts fill the canvas", () => {
    // in-motion (m = 4 cells) plus ten s shortcuts (20 cells) fill the 6x4
    // canvas exactly; every earlier room shortcut is genuinely pinned before
    // Memory plant asks for a slot.
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: true },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    const earlierRooms = [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
    ];
    for (const roomId of earlierRooms) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    for (const id of [
      "boards",
      "calendar",
      "strategy",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
    ]) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }

    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("memory");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
    expect(
      (loadGlassConfig().layout ?? []).find((entry) => entry.id === "memory")
        ?.visible,
    ).not.toBe(true);
  });

  test("the Memory shortcut joins a fully freed canvas and repeats honestly", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    for (const roomId of [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
    ]) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "memory")?.visible).toBe(true);

    const beforeRepeat = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const again = pinRoomBlocksToOffice("memory");
    expect(again.ok).toBe(true);
    expect(again.pinned).toEqual([]);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(beforeRepeat);
    expect(
      (loadGlassConfig().layout ?? []).filter((entry) => entry.id === "memory"),
    ).toHaveLength(1);
  });

  test("pins the File locks room through its locks block", () => {
    const result = pinRoomBlocksToOffice("locks");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["locks"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "locks")?.visible).toBe(true);
  });

  test("the twelfth shortcut refuses byte-for-byte when all eleven earlier shortcuts fill the canvas", () => {
    // next (s = 2 cells) plus eleven s shortcuts (22 cells) fill the 6x4
    // canvas exactly; every earlier room shortcut through Memory plant is
    // genuinely pinned before File locks asks for a slot.
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: true },
        ],
      }),
    );
    const earlierRooms = [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
    ];
    for (const roomId of earlierRooms) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    for (const id of [
      "boards",
      "calendar",
      "strategy",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
    ]) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }

    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("locks");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
    expect(
      (loadGlassConfig().layout ?? []).find((entry) => entry.id === "locks")
        ?.visible,
    ).not.toBe(true);
  });

  test("the File locks shortcut joins a fully freed canvas and repeats honestly", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    for (const roomId of [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
      "locks",
    ]) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "locks")?.visible).toBe(true);

    const beforeRepeat = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const again = pinRoomBlocksToOffice("locks");
    expect(again.ok).toBe(true);
    expect(again.pinned).toEqual([]);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(beforeRepeat);
    expect(
      (loadGlassConfig().layout ?? []).filter((entry) => entry.id === "locks"),
    ).toHaveLength(1);
  });

  test("pins the Setup room through its setup block", () => {
    const result = pinRoomBlocksToOffice("setup");
    expect(result.ok).toBe(true);
    expect(result.pinned).toEqual(["setup"]);
    const layout = loadGlassConfig().layout ?? [];
    expect(layout.find((entry) => entry.id === "setup")?.visible).toBe(true);
  });

  test("the thirteenth shortcut refuses byte-for-byte when all twelve earlier shortcuts fill the canvas", () => {
    // Twelve s shortcuts (24 cells) fill the 6x4 canvas exactly with every
    // core Office block hidden; every earlier room shortcut through File
    // locks is genuinely pinned before Setup asks for a slot.
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    const earlierRooms = [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
      "locks",
    ];
    for (const roomId of earlierRooms) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    const layout = loadGlassConfig().layout ?? [];
    const earlierBlocks = [
      "boards",
      "calendar",
      "strategy",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
      "locks",
    ];
    for (const id of earlierBlocks) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }
    // Prove the 24/24 precondition, not merely the resulting refusal copy.
    expect(
      layout.filter((entry) => entry.visible && entry.size === "s"),
    ).toHaveLength(12);

    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("setup");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
    expect(
      (loadGlassConfig().layout ?? []).find((entry) => entry.id === "setup")
        ?.visible,
    ).not.toBe(true);

    // Freeing one small shortcut admits Setup, and repeating is honest.
    const stored = JSON.parse(before ?? "{}") as {
      layout?: { id: string; size: string; visible: boolean }[];
    };
    stored.layout = (stored.layout ?? []).map((entry) =>
      entry.id === "boards" ? { ...entry, visible: false } : entry,
    );
    localStorage.setItem(GLASS_CONFIG_STORAGE_KEY, JSON.stringify(stored));
    const admitted = pinRoomBlocksToOffice("setup");
    expect(admitted.ok).toBe(true);
    expect(admitted.pinned).toEqual(["setup"]);
    expect(
      (loadGlassConfig().layout ?? []).find((entry) => entry.id === "setup")
        ?.visible,
    ).toBe(true);

    const beforeRepeat = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const again = pinRoomBlocksToOffice("setup");
    expect(again.ok).toBe(true);
    expect(again.pinned).toEqual([]);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(beforeRepeat);
    expect(
      (loadGlassConfig().layout ?? []).filter((entry) => entry.id === "setup"),
    ).toHaveLength(1);
  });

  test("the fourteenth shortcut refuses byte-for-byte against thirteen earlier placements with twelve visible", () => {
    // Thirteen earlier small shortcuts cannot all be visible beside a full
    // 6x4 canvas, so the honest capacity fixture holds every placement
    // through Setup while exactly one earlier shortcut is explicitly hidden:
    // twelve visible s blocks fill 24/24 cells before Policy asks for a slot.
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: false },
          { id: "in-motion", size: "m", visible: false },
          { id: "done", size: "m", visible: false },
          { id: "next", size: "s", visible: false },
        ],
      }),
    );
    const roomsThroughLocks = [
      "boards",
      "calendar",
      "plan",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
      "locks",
    ];
    for (const roomId of roomsThroughLocks) {
      expect(pinRoomBlocksToOffice(roomId).ok).toBe(true);
    }
    // Explicitly hide one earlier placement so Setup - the thirteenth - can
    // genuinely join the persisted layout.
    const withTwelve = JSON.parse(
      localStorage.getItem(GLASS_CONFIG_STORAGE_KEY) ?? "{}",
    ) as { layout?: { id: string; size: string; visible: boolean }[] };
    withTwelve.layout = (withTwelve.layout ?? []).map((entry) =>
      entry.id === "boards" ? { ...entry, visible: false } : entry,
    );
    localStorage.setItem(GLASS_CONFIG_STORAGE_KEY, JSON.stringify(withTwelve));
    expect(pinRoomBlocksToOffice("setup").ok).toBe(true);

    // Prove the precondition: all thirteen earlier placements exist, exactly
    // one is hidden, and twelve visible s shortcuts occupy 24/24 cells.
    const layout = loadGlassConfig().layout ?? [];
    const earlierBlocks = [
      "boards",
      "calendar",
      "strategy",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
      "directory",
      "memory",
      "locks",
      "setup",
    ];
    for (const id of earlierBlocks) {
      expect(layout.some((entry) => entry.id === id)).toBe(true);
    }
    expect(layout.find((entry) => entry.id === "boards")?.visible).toBe(false);
    expect(
      layout.filter((entry) => entry.visible && entry.size === "s"),
    ).toHaveLength(12);

    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("policy");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
    expect(
      (loadGlassConfig().layout ?? []).find((entry) => entry.id === "policy")
        ?.visible,
    ).not.toBe(true);

    // Freeing one visible shortcut admits Policy, and repeating is honest.
    const stored = JSON.parse(before ?? "{}") as {
      layout?: { id: string; size: string; visible: boolean }[];
    };
    stored.layout = (stored.layout ?? []).map((entry) =>
      entry.id === "calendar" ? { ...entry, visible: false } : entry,
    );
    localStorage.setItem(GLASS_CONFIG_STORAGE_KEY, JSON.stringify(stored));
    const admitted = pinRoomBlocksToOffice("policy");
    expect(admitted.ok).toBe(true);
    expect(admitted.pinned).toEqual(["policy"]);
    expect(
      (loadGlassConfig().layout ?? []).find((entry) => entry.id === "policy")
        ?.visible,
    ).toBe(true);

    const beforeRepeat = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const again = pinRoomBlocksToOffice("policy");
    expect(again.ok).toBe(true);
    expect(again.pinned).toEqual([]);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(beforeRepeat);
    expect(
      (loadGlassConfig().layout ?? []).filter((entry) => entry.id === "policy"),
    ).toHaveLength(1);
  });

  test("fails honestly for rooms with no registered Office block yet", () => {
    const result = pinRoomBlocksToOffice("reef");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/pin/i);
    expect(loadGlassConfig().layout).toBeUndefined();
  });

  test("reports storage failure without announcing or changing config", () => {
    const listener = vi.fn();
    window.addEventListener(GLASS_CONFIG_EVENT, listener);
    const storage = {
      getItem: () => null,
      setItem: () => {
        throw new Error("disk full");
      },
    } as unknown as Storage;
    try {
      const result = pinRoomBlocksToOffice("boards", storage);
      expect(result.ok).toBe(false);
      expect(result.error).toMatch(/nothing was changed/i);
      expect(listener).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener(GLASS_CONFIG_EVENT, listener);
    }
  });

  test("rejects a pin that would exceed the fixed Office canvas", () => {
    localStorage.setItem(
      GLASS_CONFIG_STORAGE_KEY,
      JSON.stringify({
        themeId: "auto",
        customThemes: [],
        layout: [
          { id: "needs-you", size: "l", visible: true },
          { id: "in-motion", size: "l", visible: true },
          { id: "done", size: "l", visible: true },
          { id: "next", size: "s", visible: false },
          { id: "boards", size: "s", visible: false },
        ],
      }),
    );
    const before = localStorage.getItem(GLASS_CONFIG_STORAGE_KEY);
    const result = pinRoomBlocksToOffice("boards");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exceed/i);
    expect(localStorage.getItem(GLASS_CONFIG_STORAGE_KEY)).toBe(before);
  });
});
