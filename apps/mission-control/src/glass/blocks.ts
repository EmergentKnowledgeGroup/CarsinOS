/**
 * The Office canvas is config, not code: an ordered list of block
 * placements resolved against a block registry. Blocks change density
 * with size; the canvas itself never grows.
 */

export type BlockSize = "s" | "m" | "l";

export interface BlockDef {
  id: string;
  title: string;
  defaultSize: BlockSize;
  defaultVisible: boolean;
}

export interface BlockPlacement {
  id: string;
  size: BlockSize;
  visible: boolean;
}

const SIZES: readonly BlockSize[] = ["s", "m", "l"];

function isBlockSize(value: unknown): value is BlockSize {
  return typeof value === "string" && (SIZES as readonly string[]).includes(value);
}

function defaultPlacement(def: BlockDef): BlockPlacement {
  return { id: def.id, size: def.defaultSize, visible: def.defaultVisible };
}

/**
 * Reconcile a saved layout with the current block registry:
 * saved order/size/visibility win for known blocks, unknown blocks are
 * dropped, and newly registered blocks are appended with their defaults.
 */
export function normalizeLayout(
  saved: readonly BlockPlacement[] | undefined,
  registry: readonly BlockDef[],
): BlockPlacement[] {
  const byId = new Map(registry.map((def) => [def.id, def]));
  const result: BlockPlacement[] = [];
  const seen = new Set<string>();
  for (const placement of saved ?? []) {
    const def = byId.get(placement.id);
    if (!def || seen.has(placement.id)) continue;
    seen.add(placement.id);
    result.push({
      id: placement.id,
      size: isBlockSize(placement.size) ? placement.size : def.defaultSize,
      visible:
        typeof placement.visible === "boolean"
          ? placement.visible
          : def.defaultVisible,
    });
  }
  for (const def of registry) {
    if (!seen.has(def.id)) result.push(defaultPlacement(def));
  }
  return result;
}

/** Move a block one slot forward (-1) or backward (+1); no-op at the edges. */
export function moveBlock(
  layout: readonly BlockPlacement[],
  id: string,
  delta: -1 | 1,
): BlockPlacement[] {
  const from = layout.findIndex((placement) => placement.id === id);
  const to = from + delta;
  if (from < 0 || to < 0 || to >= layout.length) {
    return layout as BlockPlacement[];
  }
  const next = [...layout];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved as BlockPlacement);
  return next;
}

export function setBlockSize(
  layout: readonly BlockPlacement[],
  id: string,
  size: BlockSize,
): BlockPlacement[] {
  if (!layout.some((placement) => placement.id === id)) {
    return layout as BlockPlacement[];
  }
  return layout.map((placement) =>
    placement.id === id ? { ...placement, size } : placement,
  );
}

export function setBlockVisible(
  layout: readonly BlockPlacement[],
  id: string,
  visible: boolean,
): BlockPlacement[] {
  if (!layout.some((placement) => placement.id === id)) {
    return layout as BlockPlacement[];
  }
  return layout.map((placement) =>
    placement.id === id ? { ...placement, visible } : placement,
  );
}

/**
 * "Pin to Office": ensure a registered block has a visible placement.
 * Adds a registry entry to the layout config — never copies data and never
 * creates a second source of truth. Unknown ids fail closed.
 */
export function pinBlockToOffice(
  layout: readonly BlockPlacement[],
  registry: readonly BlockDef[],
  id: string,
): BlockPlacement[] {
  const def = registry.find((entry) => entry.id === id);
  if (!def) return layout as BlockPlacement[];
  const existing = layout.find((placement) => placement.id === id);
  if (existing) {
    if (existing.visible) return layout as BlockPlacement[];
    return setBlockVisible(layout, id, true);
  }
  return [...layout, { ...defaultPlacement(def), visible: true }];
}

export function cycleSize(size: BlockSize): BlockSize {
  const index = SIZES.indexOf(size);
  return SIZES[(index + 1) % SIZES.length] as BlockSize;
}

export interface BlockSpan {
  cols: number;
  rows: number;
}

/** Spans on the six-column, four-row office canvas. */
export function spanFor(size: BlockSize): BlockSpan {
  switch (size) {
    case "s":
      return { cols: 2, rows: 1 };
    case "m":
      return { cols: 2, rows: 2 };
    case "l":
      return { cols: 4, rows: 2 };
  }
}
