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

  test("all three Trenches shortcuts fit the default canvas together", () => {
    expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    expect(pinRoomBlocksToOffice("calendar").ok).toBe(true);
    expect(pinRoomBlocksToOffice("plan").ok).toBe(true);
    const layout = loadGlassConfig().layout ?? [];
    for (const id of ["boards", "calendar", "strategy"]) {
      expect(layout.find((entry) => entry.id === id)?.visible).toBe(true);
    }
  });

  test("fails honestly for rooms with no registered Office block yet", () => {
    const result = pinRoomBlocksToOffice("staff");
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
