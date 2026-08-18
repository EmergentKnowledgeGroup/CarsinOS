import { useEffect, useMemo, useState } from "react";
import {
  COCKPIT_GRID_COLS,
  COCKPIT_WIDGET_PALETTE,
  WIDGET_SIZE_CONSTRAINTS,
  defaultCockpitPages,
  needsGridReflow,
  loadCockpitPagesFromStorage,
  opsDefaultTemplate,
  packWidgetsRowFirst,
  persistCockpitPagesToStorage,
  reflowWidgetsToGrid,
  sanitizeCockpitPages,
  type CockpitPageLayoutV2,
  type CockpitWidgetKind,
  type CockpitWidgetLayoutV2,
  type CockpitWidgetPosition,
} from "./cockpitLayout";

function rectanglesOverlap(
  a: Pick<CockpitWidgetPosition, "x" | "y" | "w" | "h">,
  b: Pick<CockpitWidgetPosition, "x" | "y" | "w" | "h">,
): boolean {
  return (
    a.x < b.x + b.w &&
    a.x + a.w > b.x &&
    a.y < b.y + b.h &&
    a.y + a.h > b.y
  );
}

function canPlaceWidgetAt(
  candidate: Pick<CockpitWidgetPosition, "x" | "y" | "w" | "h">,
  widgets: CockpitWidgetLayoutV2[],
  ignoreInstanceId?: string,
): boolean {
  return widgets.every((widget) => {
    if (ignoreInstanceId && widget.instance_id === ignoreInstanceId) {
      return true;
    }
    return !rectanglesOverlap(candidate, widget.position);
  });
}

function findFirstFitPosition(
  w: number,
  h: number,
  widgets: CockpitWidgetLayoutV2[],
  ignoreInstanceId?: string,
): Pick<CockpitWidgetPosition, "x" | "y"> {
  const bottom = widgets.reduce(
    (maxY, widget) => Math.max(maxY, widget.position.y + widget.position.h),
    0,
  );
  const maxSearchRow = Math.max(24, bottom + 24);
  for (let y = 0; y <= maxSearchRow; y += 1) {
    for (let x = 0; x <= COCKPIT_GRID_COLS - w; x += 1) {
      if (canPlaceWidgetAt({ x, y, w, h }, widgets, ignoreInstanceId)) {
        return { x, y };
      }
    }
  }
  return { x: 0, y: maxSearchRow + 1 };
}

function hasAnyOverlap(widgets: CockpitWidgetLayoutV2[]): boolean {
  for (let i = 0; i < widgets.length; i += 1) {
    const a = widgets[i];
    for (let j = i + 1; j < widgets.length; j += 1) {
      const b = widgets[j];
      if (rectanglesOverlap(a.position, b.position)) {
        return true;
      }
    }
  }
  return false;
}

export function useCockpitController() {
  const [initialPages] = useState<CockpitPageLayoutV2[]>(() => {
    const pages = loadCockpitPagesFromStorage();
    // Auto-fix single-column layouts (old default or failed migration)
    const needsFix = pages.some(needsGridReflow);
    if (!needsFix) return pages;
    const fixed = pages.map((page) =>
      needsGridReflow(page)
        ? { ...page, widgets: reflowWidgetsToGrid(page.widgets) }
        : page,
    );
    persistCockpitPagesToStorage(fixed);
    return fixed;
  });
  const [incidentMode, setIncidentMode] = useState(false);
  const [cockpitPages, setCockpitPages] = useState<CockpitPageLayoutV2[]>(initialPages);
  const [activeCockpitPageId, setActiveCockpitPageId] = useState(
    initialPages[0]?.page_id ?? "dashboard"
  );

  const activeCockpitPage = useMemo(() => {
    return (
      cockpitPages.find((page) => page.page_id === activeCockpitPageId) ??
      cockpitPages[0] ??
      defaultCockpitPages()[0]
    );
  }, [activeCockpitPageId, cockpitPages]);

  useEffect(() => {
    persistCockpitPagesToStorage(cockpitPages);
  }, [cockpitPages]);

  /* ── Mutations ──────────────────────────────────────────────────────────── */

  const updateActivePage = (
    updater: (page: CockpitPageLayoutV2) => CockpitPageLayoutV2,
  ) => {
    setCockpitPages((prev) =>
      prev.map((page) =>
        page.page_id === activeCockpitPageId ? updater(page) : page,
      ),
    );
  };

  const addCockpitWidget = (widgetKind: CockpitWidgetKind) => {
    const palette = COCKPIT_WIDGET_PALETTE.find((item) => item.widget === widgetKind);
    if (!palette) return;

    const constraints = WIDGET_SIZE_CONSTRAINTS[widgetKind];
    const instanceId = `${widgetKind}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    updateActivePage((page) => {
      const w = constraints.defaultW;
      const h = constraints.defaultH;
      const position = findFirstFitPosition(w, h, page.widgets);
      return {
        ...page,
        widgets: [
          ...page.widgets,
          {
            instance_id: instanceId,
            widget: widgetKind,
            title: palette.title,
            position: {
              ...position,
              w,
              h,
            },
          },
        ],
      };
    });
  };

  const addCustomWidget = (widget: CockpitWidgetLayoutV2) => {
    updateActivePage((page) => ({
      ...page,
      widgets: [...page.widgets, widget],
    }));
  };

  const removeCockpitWidget = (instanceId: string) => {
    updateActivePage((page) => ({
      ...page,
      widgets: page.widgets.filter((w) => w.instance_id !== instanceId),
    }));
  };

  const autoFitActivePage = () => {
    updateActivePage((page) => ({
      ...page,
      widgets: packWidgetsRowFirst(page.widgets),
    }));
  };

  const updateWidgetPosition = (
    instanceId: string,
    pos: Partial<CockpitWidgetPosition>,
  ) => {
    updateActivePage((page) => ({
      ...page,
      widgets: page.widgets.map((w) =>
        w.instance_id === instanceId
          ? { ...w, position: { ...w.position, ...pos } }
          : w,
      ),
    }));
  };

  const nudgeCockpitWidget = (
    instanceId: string,
    delta: Partial<Pick<CockpitWidgetPosition, "x" | "y">>,
  ) => {
    updateActivePage((page) => ({
      ...page,
      widgets: page.widgets.map((widget) => {
        if (widget.instance_id !== instanceId) {
          return widget;
        }
        const nextX = Math.max(
          0,
          Math.min(
            COCKPIT_GRID_COLS - widget.position.w,
            widget.position.x + (delta.x ?? 0),
          ),
        );
        const nextY = Math.max(0, widget.position.y + (delta.y ?? 0));
        if (
          nextX === widget.position.x &&
          nextY === widget.position.y
        ) {
          return widget;
        }
        const canMove = canPlaceWidgetAt(
          {
            x: nextX,
            y: nextY,
            w: widget.position.w,
            h: widget.position.h,
          },
          page.widgets,
          widget.instance_id,
        );
        if (!canMove) {
          return widget;
        }
        return {
          ...widget,
          position: {
            ...widget.position,
            x: nextX,
            y: nextY,
          },
        };
      }),
    }));
  };

  const handleLayoutChange = (
    rglLayout: Array<{ i: string; x: number; y: number; w: number; h: number }>,
  ) => {
    const byId = new Map(rglLayout.map((entry) => [entry.i, entry] as const));
    updateActivePage((page) => {
      let changed = false;
      const widgets = page.widgets.map((w) => {
        const match = byId.get(w.instance_id);
        if (!match) return w;
        if (
          w.position.x === match.x &&
          w.position.y === match.y &&
          w.position.w === match.w &&
          w.position.h === match.h
        ) {
          return w;
        }
        changed = true;
        const clampedX = Math.max(
          0,
          Math.min(COCKPIT_GRID_COLS - match.w, match.x),
        );
        return {
          ...w,
          position: {
            x: clampedX,
            y: Math.max(0, match.y),
            w: match.w,
            h: match.h,
          },
        };
      });
      const visibleWidgets = widgets.filter((widget) => byId.has(widget.instance_id));
      if (hasAnyOverlap(visibleWidgets)) {
        return page;
      }
      return changed ? { ...page, widgets } : page;
    });
  };

  const renameCockpitPage = (pageId: string, name: string) => {
    const trimmed = name.trim();
    setCockpitPages((prev) =>
      prev.map((page) =>
        page.page_id === pageId
          ? {
              ...page,
              name: trimmed || "Custom Page",
            }
          : page,
      ),
    );
  };

  const renameActivePage = (name: string) => {
    renameCockpitPage(activeCockpitPageId, name);
  };

  const resetCockpitLayout = () => {
    const defaults = defaultCockpitPages();
    setCockpitPages(defaults);
    setActiveCockpitPageId(defaults[0].page_id);
  };

  const addCockpitPage = () => {
    const nextPageId = `page-${Date.now()}`;
    setCockpitPages((prev) => [
      ...prev,
      {
        page_id: nextPageId,
        name: `Page ${prev.length + 1}`,
        widgets: [],
      },
    ]);
    setActiveCockpitPageId(nextPageId);
  };

  const deleteCockpitPage = (pageId: string) => {
    const fallbackActiveId =
      cockpitPages.find((page) => page.page_id !== pageId)?.page_id ??
      defaultCockpitPages()[0].page_id;

    setCockpitPages((prev) => {
      const next = prev.filter((p) => p.page_id !== pageId);
      if (next.length === 0) return defaultCockpitPages();
      return next;
    });
    if (activeCockpitPageId === pageId) {
      setActiveCockpitPageId(fallbackActiveId ?? "dashboard");
    }
  };

  const duplicateCockpitPage = (pageId: string) => {
    const source = cockpitPages.find((p) => p.page_id === pageId);
    if (!source) return;
    const newPageId = `page-${Date.now()}`;
    const duplicated: CockpitPageLayoutV2 = {
      ...source,
      page_id: newPageId,
      name: `${source.name} Copy`,
      widgets: source.widgets.map((w) => ({
        ...w,
        instance_id: `${w.widget}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      })),
    };
    setCockpitPages((prev) => [...prev, duplicated]);
    setActiveCockpitPageId(newPageId);
  };

  const loadTemplate = () => {
    const template = opsDefaultTemplate();
    updateActivePage(() => ({
      ...template,
      page_id: activeCockpitPageId,
      name: activeCockpitPage.name,
    }));
  };

  const exportCockpitLayout = () => {
    if (typeof window === "undefined") return;
    const payload = JSON.stringify(cockpitPages, null, 2);
    const blob = new Blob([payload], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `mission-control-cockpit-${Date.now()}.json`;
    document.body.appendChild(anchor);
    anchor.click();
    document.body.removeChild(anchor);
    URL.revokeObjectURL(url);
  };

  const importCockpitLayout = async (file: File) => {
    const raw = await file.text();
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw) as unknown;
    } catch {
      throw new Error("invalid cockpit layout JSON");
    }
    const sanitized = sanitizeCockpitPages(parsed);
    setCockpitPages(sanitized);
    setActiveCockpitPageId(sanitized[0].page_id);
  };

  return {
    incidentMode,
    setIncidentMode,
    cockpitPages,
    setCockpitPages,
    activeCockpitPageId,
    setActiveCockpitPageId,
    activeCockpitPage,
    addCockpitWidget,
    addCustomWidget,
    removeCockpitWidget,
    autoFitActivePage,
    updateWidgetPosition,
    nudgeCockpitWidget,
    handleLayoutChange,
    renameCockpitPage,
    renameActivePage,
    resetCockpitLayout,
    addCockpitPage,
    deleteCockpitPage,
    duplicateCockpitPage,
    loadTemplate,
    exportCockpitLayout,
    importCockpitLayout,
  };
}
