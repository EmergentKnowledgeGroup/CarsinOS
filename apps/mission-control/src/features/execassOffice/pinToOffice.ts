/**
 * "Pin to Office" from a floor room: turn the room's registered Office
 * blocks visible in the persisted layout config. Adds registry entries to
 * config data only — never copies room data and never creates a second
 * source of truth. Unknown rooms and unregistered blocks fail closed.
 */

import {
  layoutFitsCanvas,
  normalizeLayout,
  pinBlockToOffice,
} from "../../glass/blocks";
import {
  loadGlassConfig,
  notifyGlassConfigChanged,
  saveGlassConfig,
} from "../../glass/config";
import { DEFAULT_FLOORS, findRoom } from "../../glass/floors";
import { OFFICE_BLOCK_REGISTRY } from "./officeBlocks";

export interface PinRoomResult {
  ok: boolean;
  /** Block ids newly made visible; empty when everything was already pinned. */
  pinned: string[];
  error?: string;
}

export function pinRoomBlocksToOffice(
  roomId: string,
  storage: Storage = localStorage,
): PinRoomResult {
  const found = findRoom(DEFAULT_FLOORS, roomId);
  if (!found) {
    return { ok: false, pinned: [], error: "That room is not registered." };
  }
  const registered = found.room.blocks.filter((blockId) =>
    OFFICE_BLOCK_REGISTRY.some((def) => def.id === blockId),
  );
  if (registered.length === 0) {
    return {
      ok: false,
      pinned: [],
      error: `${found.room.label} cannot be pinned to the Office yet.`,
    };
  }
  const config = loadGlassConfig(storage);
  const current = normalizeLayout(config.layout, OFFICE_BLOCK_REGISTRY);
  let next = current;
  const pinned: string[] = [];
  for (const blockId of registered) {
    const candidate = pinBlockToOffice(next, OFFICE_BLOCK_REGISTRY, blockId);
    if (candidate !== next) pinned.push(blockId);
    next = candidate;
  }
  if (pinned.length === 0) {
    return { ok: true, pinned: [] };
  }
  if (!layoutFitsCanvas(next)) {
    return {
      ok: false,
      pinned: [],
      error:
        "Pinning this would exceed the six-column, four-row Office canvas.",
    };
  }
  const saved = saveGlassConfig({ ...config, layout: next }, storage);
  if (!saved.ok) {
    return {
      ok: false,
      pinned: [],
      error: "The pin could not be saved. Nothing was changed.",
    };
  }
  notifyGlassConfigChanged();
  return { ok: true, pinned };
}
