// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { GLASS_CONFIG_EVENT, loadGlassConfig } from "../../glass/config";
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

  test("fails honestly for rooms with no registered Office block yet", () => {
    const result = pinRoomBlocksToOffice("calendar");
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/pin/i);
    expect(loadGlassConfig().layout).toBeUndefined();
  });
});
