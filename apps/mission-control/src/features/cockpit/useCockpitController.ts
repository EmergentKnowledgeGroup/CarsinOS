import { useEffect, useMemo, useState } from "react";
import {
  COCKPIT_WIDGET_PALETTE,
  WIDGET_SIZE_CONSTRAINTS,
  defaultCockpitPages,
  loadCockpitPagesFromStorage,
  opsDefaultTemplate,
  persistCockpitPagesToStorage,
  sanitizeCockpitPages,
  type CockpitPageLayoutV2,
  type CockpitWidgetKind,
  type CockpitWidgetLayoutV2,
  type CockpitWidgetPosition,
} from "./cockpitLayout";

export function useCockpitController() {
  const [initialPages] = useState<CockpitPageLayoutV2[]>(() => loadCockpitPagesFromStorage());
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

    updateActivePage((page) => ({
      ...page,
      widgets: [
        ...page.widgets,
        {
          instance_id: instanceId,
          widget: widgetKind,
          title: palette.title,
          position: {
            x: 0,
            y: Infinity, // RGL will compact this to the bottom
            w: constraints.defaultW,
            h: constraints.defaultH,
          },
        },
      ],
    }));
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

  const handleLayoutChange = (
    rglLayout: Array<{ i: string; x: number; y: number; w: number; h: number }>,
  ) => {
    updateActivePage((page) => ({
      ...page,
      widgets: page.widgets.map((w) => {
        const match = rglLayout.find((l) => l.i === w.instance_id);
        if (!match) return w;
        return {
          ...w,
          position: { x: match.x, y: match.y, w: match.w, h: match.h },
        };
      }),
    }));
  };

  const renameActivePage = (name: string) => {
    updateActivePage((page) => ({
      ...page,
      name: name || "Custom Page",
    }));
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
    const nextPages = cockpitPages.filter((page) => page.page_id !== pageId);
    const normalizedPages = nextPages.length > 0 ? nextPages : defaultCockpitPages();
    setCockpitPages(normalizedPages);
    if (
      activeCockpitPageId === pageId ||
      !normalizedPages.some((page) => page.page_id === activeCockpitPageId)
    ) {
      setActiveCockpitPageId(normalizedPages[0]?.page_id ?? "dashboard");
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
    updateWidgetPosition,
    handleLayoutChange,
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
