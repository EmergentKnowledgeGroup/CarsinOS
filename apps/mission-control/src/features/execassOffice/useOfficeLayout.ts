/**
 * Office layout state: placements come from persisted Glass config resolved
 * against the block registry; every arrange action edits that data and saves
 * it back, preserving whatever else lives in the config blob.
 */

import { useCallback, useState } from "react";

import {
  moveBlock,
  normalizeLayout,
  pinBlockToOffice,
  setBlockSize,
  setBlockVisible,
  cycleSize,
  type BlockDef,
  type BlockPlacement,
} from "../../glass/blocks";
import {
  loadGlassConfig,
  notifyGlassConfigChanged,
  saveGlassConfig,
} from "../../glass/config";
import { OFFICE_BLOCK_REGISTRY } from "./officeBlocks";

export interface OfficeLayoutController {
  placements: BlockPlacement[];
  arranging: boolean;
  setArranging: (value: boolean) => void;
  /** Registry blocks with no visible placement — the add-from-library list. */
  library: BlockDef[];
  move: (id: string, delta: -1 | 1) => void;
  resize: (id: string) => void;
  hide: (id: string) => void;
  pin: (id: string) => void;
}

export function useOfficeLayout(): OfficeLayoutController {
  const [placements, setPlacements] = useState<BlockPlacement[]>(() =>
    normalizeLayout(loadGlassConfig().layout, OFFICE_BLOCK_REGISTRY),
  );
  const [arranging, setArranging] = useState(false);

  const commit = useCallback(
    (op: (current: BlockPlacement[]) => BlockPlacement[]) => {
      setPlacements((current) => {
        const next = op(current);
        if (next === current) return current;
        saveGlassConfig({ ...loadGlassConfig(), layout: next });
        notifyGlassConfigChanged();
        return next;
      });
    },
    [],
  );

  const move = useCallback(
    (id: string, delta: -1 | 1) =>
      commit((current) => {
        // Hop over hidden placements so one click always changes what the
        // user can actually see.
        let next = moveBlock(current, id, delta);
        while (next !== current) {
          const index = next.findIndex((p) => p.id === id);
          const passed = next[index - delta];
          if (!passed || passed.visible) break;
          const further = moveBlock(next, id, delta);
          if (further === next) break;
          next = further;
        }
        return next;
      }),
    [commit],
  );
  const resize = useCallback(
    (id: string) =>
      commit((c) => {
        const placement = c.find((p) => p.id === id);
        if (!placement) return c;
        return setBlockSize(c, id, cycleSize(placement.size));
      }),
    [commit],
  );
  const hide = useCallback(
    (id: string) => commit((c) => setBlockVisible(c, id, false)),
    [commit],
  );
  const pin = useCallback(
    (id: string) => commit((c) => pinBlockToOffice(c, OFFICE_BLOCK_REGISTRY, id)),
    [commit],
  );

  const visible = new Set(
    placements.filter((p) => p.visible).map((p) => p.id),
  );
  const library = OFFICE_BLOCK_REGISTRY.filter((def) => !visible.has(def.id));

  return {
    placements,
    arranging,
    setArranging,
    library,
    move,
    resize,
    hide,
    pin,
  };
}
