import { describe, expect, test } from "vitest";

import {
  cycleSize,
  layoutFitsCanvas,
  moveBlock,
  normalizeLayout,
  pinBlockToOffice,
  setBlockSize,
  setBlockVisible,
  spanFor,
  type BlockDef,
  type BlockPlacement,
} from "./blocks";

const REGISTRY: BlockDef[] = [
  { id: "needs-you", title: "Needs you", defaultSize: "l", defaultVisible: true },
  { id: "in-motion", title: "In motion", defaultSize: "m", defaultVisible: true },
  { id: "done", title: "Done since you checked", defaultSize: "m", defaultVisible: true },
  { id: "next", title: "Next", defaultSize: "s", defaultVisible: true },
  { id: "chatter", title: "Office chatter", defaultSize: "s", defaultVisible: false },
];

describe("normalizeLayout", () => {
  test("with no saved layout, returns registry defaults in registry order", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    expect(layout.map((p) => p.id)).toEqual([
      "needs-you",
      "in-motion",
      "done",
      "next",
      "chatter",
    ]);
    expect(layout[0]).toEqual({ id: "needs-you", size: "l", visible: true });
    expect(layout[4]).toEqual({ id: "chatter", size: "s", visible: false });
  });

  test("preserves saved order, sizes, and visibility for known blocks", () => {
    const saved: BlockPlacement[] = [
      { id: "next", size: "l", visible: true },
      { id: "needs-you", size: "m", visible: false },
    ];
    const layout = normalizeLayout(saved, REGISTRY);
    expect(layout[0]).toEqual({ id: "next", size: "l", visible: true });
    expect(layout[1]).toEqual({ id: "needs-you", size: "m", visible: false });
  });

  test("drops blocks the registry no longer knows", () => {
    const saved: BlockPlacement[] = [
      { id: "haunted-block", size: "m", visible: true },
      { id: "next", size: "s", visible: true },
    ];
    const layout = normalizeLayout(saved, REGISTRY);
    expect(layout.map((p) => p.id)).not.toContain("haunted-block");
  });

  test("appends newly registered blocks after saved ones, using their defaults", () => {
    const saved: BlockPlacement[] = [{ id: "next", size: "s", visible: true }];
    const layout = normalizeLayout(saved, REGISTRY);
    expect(layout[0]?.id).toBe("next");
    const appended = layout.slice(1).map((p) => p.id);
    expect(appended).toEqual(["needs-you", "in-motion", "done", "chatter"]);
    expect(layout.find((p) => p.id === "chatter")?.visible).toBe(false);
  });

  test("coerces invalid saved sizes back to the registry default", () => {
    const saved = [
      { id: "next", size: "xxl", visible: true },
    ] as unknown as BlockPlacement[];
    const layout = normalizeLayout(saved, REGISTRY);
    expect(layout[0]?.size).toBe("s");
  });
});

describe("cycleSize", () => {
  test("cycles s -> m -> l -> s", () => {
    expect(cycleSize("s")).toBe("m");
    expect(cycleSize("m")).toBe("l");
    expect(cycleSize("l")).toBe("s");
  });
});

describe("spanFor", () => {
  test("maps sizes to grid spans on the six-column office canvas", () => {
    expect(spanFor("s")).toEqual({ cols: 2, rows: 1 });
    expect(spanFor("m")).toEqual({ cols: 2, rows: 2 });
    expect(spanFor("l")).toEqual({ cols: 4, rows: 2 });
  });
});

describe("layoutFitsCanvas", () => {
  test("accepts the default office layout inside six columns and four rows", () => {
    expect(layoutFitsCanvas(normalizeLayout(undefined, REGISTRY))).toBe(true);
  });

  test("rejects visible placements that would create implicit rows", () => {
    const oversized: BlockPlacement[] = ["a", "b", "c", "d"].map((id) => ({
      id,
      size: "l",
      visible: true,
    }));
    expect(layoutFitsCanvas(oversized)).toBe(false);
  });
});

describe("moveBlock", () => {
  test("moves a block one slot toward the front", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    const moved = moveBlock(layout, "in-motion", -1);
    expect(moved.map((p) => p.id).slice(0, 2)).toEqual(["in-motion", "needs-you"]);
  });

  test("moves a block one slot toward the back", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    const moved = moveBlock(layout, "needs-you", 1);
    expect(moved.map((p) => p.id).slice(0, 2)).toEqual(["in-motion", "needs-you"]);
  });

  test("does nothing at the edges or for unknown ids", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    expect(moveBlock(layout, "needs-you", -1)).toBe(layout);
    expect(moveBlock(layout, "chatter", 1)).toBe(layout);
    expect(moveBlock(layout, "haunted-block", 1)).toBe(layout);
  });
});

describe("setBlockSize", () => {
  test("sets the size of one block only", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    const next = setBlockSize(layout, "next", "l");
    expect(next.find((p) => p.id === "next")?.size).toBe("l");
    expect(next.find((p) => p.id === "done")?.size).toBe("m");
  });

  test("is a no-op for unknown ids", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    expect(setBlockSize(layout, "haunted-block", "l")).toBe(layout);
  });
});

describe("setBlockVisible", () => {
  test("hides and shows a block without reordering", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    const hidden = setBlockVisible(layout, "done", false);
    expect(hidden.find((p) => p.id === "done")?.visible).toBe(false);
    expect(hidden.map((p) => p.id)).toEqual(layout.map((p) => p.id));
    const shown = setBlockVisible(hidden, "done", true);
    expect(shown.find((p) => p.id === "done")?.visible).toBe(true);
  });
});

describe("pinBlockToOffice", () => {
  test("makes an existing hidden placement visible", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    const pinned = pinBlockToOffice(layout, REGISTRY, "chatter");
    expect(pinned.find((p) => p.id === "chatter")?.visible).toBe(true);
  });

  test("appends a registered block that has no placement yet", () => {
    const layout = normalizeLayout(undefined, REGISTRY).filter(
      (p) => p.id !== "chatter",
    );
    const pinned = pinBlockToOffice(layout, REGISTRY, "chatter");
    expect(pinned[pinned.length - 1]).toEqual({
      id: "chatter",
      size: "s",
      visible: true,
    });
  });

  test("fails closed for ids the registry does not know", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    expect(pinBlockToOffice(layout, REGISTRY, "haunted-block")).toBe(layout);
  });

  test("leaves an already visible placement untouched", () => {
    const layout = normalizeLayout(undefined, REGISTRY);
    expect(pinBlockToOffice(layout, REGISTRY, "needs-you")).toBe(layout);
  });
});
