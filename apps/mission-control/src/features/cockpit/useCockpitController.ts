import { useEffect, useMemo, useState } from "react";
import {
  COCKPIT_WIDGET_PALETTE,
  defaultCockpitPages,
  loadCockpitPagesFromStorage,
  normalizeWidgetSpan,
  persistCockpitPagesToStorage,
  sanitizeCockpitPages,
  type CockpitPageLayout,
  type CockpitWidgetKind,
} from "./cockpitLayout";

export function useCockpitController() {
  const [initialPages] = useState<CockpitPageLayout[]>(() => loadCockpitPagesFromStorage());
  const [incidentMode, setIncidentMode] = useState(false);
  const [cockpitPages, setCockpitPages] = useState<CockpitPageLayout[]>(initialPages);
  const [activeCockpitPageId, setActiveCockpitPageId] = useState(
    initialPages[0]?.page_id ?? "ops-default"
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

  const addCockpitWidget = (widgetKind: CockpitWidgetKind) => {
    const palette = COCKPIT_WIDGET_PALETTE.find((item) => item.widget === widgetKind);
    if (!palette) {
      return;
    }
    setCockpitPages((previous) =>
      previous.map((page) => {
        if (page.page_id !== activeCockpitPage.page_id) {
          return page;
        }
        const instanceId = `${widgetKind}-${Date.now()}-${Math.random()
          .toString(36)
          .slice(2, 8)}`;
        return {
          ...page,
          widgets: [
            ...page.widgets,
            {
              instance_id: instanceId,
              widget: widgetKind,
              title: palette.title,
              span: palette.defaultSpan,
            },
          ],
        };
      })
    );
  };

  const removeCockpitWidget = (instanceId: string) => {
    setCockpitPages((previous) =>
      previous.map((page) =>
        page.page_id === activeCockpitPage.page_id
          ? {
              ...page,
              widgets: page.widgets.filter((widget) => widget.instance_id !== instanceId),
            }
          : page
      )
    );
  };

  const moveCockpitWidget = (instanceId: string, delta: number) => {
    setCockpitPages((previous) =>
      previous.map((page) => {
        if (page.page_id !== activeCockpitPage.page_id) {
          return page;
        }
        const index = page.widgets.findIndex((widget) => widget.instance_id === instanceId);
        if (index < 0) {
          return page;
        }
        const target = Math.max(0, Math.min(page.widgets.length - 1, index + delta));
        if (target === index) {
          return page;
        }
        const nextWidgets = [...page.widgets];
        const [entry] = nextWidgets.splice(index, 1);
        nextWidgets.splice(target, 0, entry);
        return { ...page, widgets: nextWidgets };
      })
    );
  };

  const resizeCockpitWidget = (instanceId: string, delta: number) => {
    setCockpitPages((previous) =>
      previous.map((page) => {
        if (page.page_id !== activeCockpitPage.page_id) {
          return page;
        }
        return {
          ...page,
          widgets: page.widgets.map((widget) =>
            widget.instance_id === instanceId
              ? { ...widget, span: normalizeWidgetSpan(widget.span + delta) }
              : widget
          ),
        };
      })
    );
  };

  const resetCockpitLayout = () => {
    const defaults = defaultCockpitPages();
    setCockpitPages(defaults);
    setActiveCockpitPageId(defaults[0].page_id);
  };

  const addCockpitPage = () => {
    const nextPageId = `custom-${Date.now()}`;
    setCockpitPages((previous) => [
      ...previous,
      {
        page_id: nextPageId,
        name: `Custom ${previous.length + 1}`,
        widgets: [],
      },
    ]);
    setActiveCockpitPageId(nextPageId);
  };

  const exportCockpitLayout = () => {
    if (typeof window === "undefined") {
      return;
    }
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
    removeCockpitWidget,
    moveCockpitWidget,
    resizeCockpitWidget,
    resetCockpitLayout,
    addCockpitPage,
    exportCockpitLayout,
    importCockpitLayout,
  };
}
