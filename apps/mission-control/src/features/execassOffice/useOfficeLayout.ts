/**
 * Office layout state: placements come from persisted Glass config resolved
 * against the block registry; every arrange action edits that data and saves
 * it back, preserving whatever else lives in the config blob.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import {
  moveBlock,
  layoutFitsCanvas,
  normalizeLayout,
  pinBlockToOffice,
  setBlockSize,
  setBlockVisible,
  cycleSize,
  type BlockDef,
  type BlockPlacement,
} from "../../glass/blocks";
import {
  GLASS_CONFIG_EVENT,
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
  error: string | null;
}

export function useOfficeLayout(): OfficeLayoutController {
  const [placements, setPlacements] = useState<BlockPlacement[]>(() =>
    normalizeLayout(loadGlassConfig().layout, OFFICE_BLOCK_REGISTRY),
  );
  const placementsRef = useRef(placements);
  const [arranging, setArranging] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const syncFromPersistedConfig = () => {
      const next = normalizeLayout(
        loadGlassConfig().layout,
        OFFICE_BLOCK_REGISTRY,
      );
      placementsRef.current = next;
      setPlacements(next);
      setError(null);
    };
    window.addEventListener(GLASS_CONFIG_EVENT, syncFromPersistedConfig);
    return () =>
      window.removeEventListener(GLASS_CONFIG_EVENT, syncFromPersistedConfig);
  }, []);

  const commit = useCallback(
    (op: (current: BlockPlacement[]) => BlockPlacement[]) => {
      const current = placementsRef.current;
      const next = op(current);
      if (next === current) return;
      if (!layoutFitsCanvas(next)) {
        setError("That change would exceed the six-column, four-row Office canvas.");
        return;
      }
      const result = saveGlassConfig({ ...loadGlassConfig(), layout: next });
      if (!result.ok) {
        setError("Office arrangement could not be saved. Nothing was changed.");
        return;
      }
      placementsRef.current = next;
      setPlacements(next);
      setError(null);
      notifyGlassConfigChanged();
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
    error,
  };
}
