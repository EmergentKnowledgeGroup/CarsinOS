import { describe, expect, test } from "vitest";

import {
  cycleSize,
  normalizeLayout,
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
