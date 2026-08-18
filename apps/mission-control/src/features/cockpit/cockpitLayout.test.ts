import { describe, expect, it } from "vitest";
import {
  needsGridReflow,
  opsDefaultTemplate,
  packWidgetsRowFirst,
  type CockpitPageLayoutV2,
} from "./cockpitLayout";

function withWidgets(
  widgets: CockpitPageLayoutV2["widgets"],
  pageId = "dashboard",
): CockpitPageLayoutV2 {
  return {
    page_id: pageId,
    name: "Dashboard",
    widgets,
  };
}

describe("needsGridReflow", () => {
  it("keeps the row-first default template unchanged", () => {
    const template = opsDefaultTemplate();
    expect(needsGridReflow(withWidgets(template.widgets))).toBe(false);
  });

  it("repairs collapsed legacy template layouts that stack mostly at x=0", () => {
    const collapsedWidgets = opsDefaultTemplate().widgets.map((widget, index) => ({
      ...widget,
      position: {
        x: 0,
        y: index * 4,
        w: index % 2 === 0 ? 10 : 5,
        h: widget.position.h,
      },
    }));

    expect(needsGridReflow(withWidgets(collapsedWidgets))).toBe(true);
  });

  it("repairs collapsed dashboard layouts even without template instance IDs", () => {
    const collapsedWidgets = opsDefaultTemplate().widgets.map((widget, index) => ({
      ...widget,
      instance_id: `custom-${index}`,
      position: {
        x: 0,
        y: index * 4,
        w: 10,
        h: widget.position.h,
      },
    }));

    expect(needsGridReflow(withWidgets(collapsedWidgets))).toBe(true);
  });

  it("repairs collapsed full-width stacks on non-dashboard page ids", () => {
    const collapsedWidgets = opsDefaultTemplate().widgets.map((widget, index) => ({
      ...widget,
      instance_id: `legacy-${index}`,
      position: {
        x: 0,
        y: index * 3,
        w: 10,
        h: widget.position.h,
      },
    }));

    expect(needsGridReflow(withWidgets(collapsedWidgets, "page-1731455337123"))).toBe(true);
  });

  it("repairs overlapping layouts", () => {
    const template = opsDefaultTemplate();
    const [first, second, ...rest] = template.widgets;
    const overlapping = [
      first,
      {
        ...second,
        position: { ...first.position },
      },
      ...rest,
    ];

    expect(needsGridReflow(withWidgets(overlapping))).toBe(true);
  });
});

describe("packWidgetsRowFirst", () => {
  it("packs a tall single-column layout back into row-first geometry", () => {
    const stackedWidgets = opsDefaultTemplate().widgets.map((widget, index) => ({
      ...widget,
      position: {
        ...widget.position,
        x: 0,
        y: index * 5,
      },
    }));
    const packed = packWidgetsRowFirst(stackedWidgets);

    const distinctColumns = new Set(packed.map((widget) => widget.position.x));
    const bottomEdge = packed.reduce(
      (maxRows, widget) => Math.max(maxRows, widget.position.y + widget.position.h),
      0,
    );

    expect(distinctColumns.size).toBeGreaterThan(1);
    expect(bottomEdge).toBeLessThanOrEqual(10);
  });

  it("preserves each widget's current size while repacking", () => {
    const [first, second, ...rest] = opsDefaultTemplate().widgets;
    const mutated = [
      {
        ...first,
        position: {
          ...first.position,
          w: 4,
          h: 6,
          x: 0,
          y: 0,
        },
      },
      {
        ...second,
        position: {
          ...second.position,
          w: 3,
          h: 5,
          x: 0,
          y: 8,
        },
      },
      ...rest.map((widget, index) => ({
        ...widget,
        position: {
          ...widget.position,
          x: 0,
          y: 13 + index * 4,
        },
      })),
    ];
    const packed = packWidgetsRowFirst(mutated);
    const packedFirst = packed.find((widget) => widget.instance_id === first.instance_id);
    const packedSecond = packed.find((widget) => widget.instance_id === second.instance_id);

    expect(packedFirst?.position.w).toBe(4);
    expect(packedFirst?.position.h).toBe(6);
    expect(packedSecond?.position.w).toBe(3);
    expect(packedSecond?.position.h).toBe(5);
  });
});
