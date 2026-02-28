const COCKPIT_LAYOUT_STORAGE_KEY = "mission_control.cockpit.pages.v1";

export type CockpitWidgetKind =
  | "health"
  | "focus"
  | "breakers"
  | "jobs"
  | "channels"
  | "profiles"
  | "skills"
  | "plugins"
  | "events";

export interface CockpitWidgetLayout {
  instance_id: string;
  widget: CockpitWidgetKind;
  title: string;
  span: number;
}

export interface CockpitPageLayout {
  page_id: string;
  name: string;
  widgets: CockpitWidgetLayout[];
}

export interface CockpitWidgetPaletteEntry {
  widget: CockpitWidgetKind;
  title: string;
  description: string;
  defaultSpan: number;
}

export function defaultCockpitPages(): CockpitPageLayout[] {
  return [
    {
      page_id: "ops-default",
      name: "Ops Default",
      widgets: [
        {
          instance_id: "health-default",
          widget: "health",
          title: "Pinned Health Strip",
          span: 4,
        },
        {
          instance_id: "focus-default",
          widget: "focus",
          title: "Incident Queue",
          span: 2,
        },
        {
          instance_id: "breakers-default",
          widget: "breakers",
          title: "Breaker Radar",
          span: 2,
        },
        {
          instance_id: "jobs-default",
          widget: "jobs",
          title: "Scheduler Matrix",
          span: 2,
        },
        {
          instance_id: "channels-default",
          widget: "channels",
          title: "Channel Control",
          span: 2,
        },
        {
          instance_id: "profiles-default",
          widget: "profiles",
          title: "Agent Provider Routing",
          span: 3,
        },
        {
          instance_id: "skills-default",
          widget: "skills",
          title: "Skills Control",
          span: 3,
        },
        {
          instance_id: "plugins-default",
          widget: "plugins",
          title: "Plugins Control",
          span: 3,
        },
        {
          instance_id: "events-default",
          widget: "events",
          title: "Event Tail",
          span: 3,
        },
      ],
    },
  ];
}

export function normalizeWidgetSpan(span: number): number {
  return Math.max(1, Math.min(4, Math.round(span)));
}

export function sanitizeCockpitPages(input: unknown): CockpitPageLayout[] {
  if (!Array.isArray(input)) {
    return defaultCockpitPages();
  }
  const pages = input
    .map((item) => {
      const raw = item as Partial<CockpitPageLayout>;
      if (typeof raw.page_id !== "string" || !raw.page_id.trim()) {
        return null;
      }
      const pageName =
        typeof raw.name === "string" && raw.name.trim()
          ? raw.name.trim()
          : "Custom Page";
      const widgets = Array.isArray(raw.widgets)
        ? raw.widgets
            .map((widget) => {
              const entry = widget as Partial<CockpitWidgetLayout>;
              if (
                typeof entry.instance_id !== "string" ||
                !entry.instance_id.trim() ||
                typeof entry.widget !== "string" ||
                typeof entry.title !== "string"
              ) {
                return null;
              }
              return {
                instance_id: entry.instance_id.trim(),
                widget: entry.widget as CockpitWidgetKind,
                title: entry.title.trim() || "Widget",
                span: normalizeWidgetSpan(Number(entry.span ?? 2)),
              } satisfies CockpitWidgetLayout;
            })
            .filter((widget): widget is CockpitWidgetLayout => widget !== null)
        : [];
      return {
        page_id: raw.page_id.trim(),
        name: pageName,
        widgets: widgets.length > 0 ? widgets : defaultCockpitPages()[0].widgets,
      } satisfies CockpitPageLayout;
    })
    .filter((page): page is CockpitPageLayout => page !== null);
  return pages.length > 0 ? pages : defaultCockpitPages();
}

export function loadCockpitPagesFromStorage(): CockpitPageLayout[] {
  if (typeof window === "undefined") {
    return defaultCockpitPages();
  }
  const raw = window.localStorage.getItem(COCKPIT_LAYOUT_STORAGE_KEY);
  if (!raw) {
    return defaultCockpitPages();
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    return sanitizeCockpitPages(parsed);
  } catch {
    return defaultCockpitPages();
  }
}

export function persistCockpitPagesToStorage(pages: CockpitPageLayout[]): void {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(COCKPIT_LAYOUT_STORAGE_KEY, JSON.stringify(pages));
}

export const COCKPIT_WIDGET_PALETTE: CockpitWidgetPaletteEntry[] = [
  {
    widget: "health",
    title: "Pinned Health Strip",
    description: "Gateway status, approvals, channels, and scheduler safety posture.",
    defaultSpan: 4,
  },
  {
    widget: "focus",
    title: "Focus Queue",
    description: "Operator attention queue with approvals, failures, and incident actions.",
    defaultSpan: 2,
  },
  {
    widget: "breakers",
    title: "Breaker Radar",
    description: "Circuit breaker and plugin breaker state with cooldown windows.",
    defaultSpan: 2,
  },
  {
    widget: "jobs",
    title: "Scheduler Matrix",
    description: "Upcoming jobs and direct run/pause controls.",
    defaultSpan: 2,
  },
  {
    widget: "channels",
    title: "Channel Ops",
    description: "Adapter health and one-click reconnect operations.",
    defaultSpan: 2,
  },
  {
    widget: "profiles",
    title: "Agent Routing",
    description: "Edit per-agent provider profile order without shell access.",
    defaultSpan: 3,
  },
  {
    widget: "skills",
    title: "Skills",
    description: "Toggle skills and inspect source paths/status.",
    defaultSpan: 3,
  },
  {
    widget: "plugins",
    title: "Plugins",
    description: "Inspect plugin runtime health and enable/disable safely.",
    defaultSpan: 3,
  },
  {
    widget: "events",
    title: "Event Tail",
    description: "Live operational event stream with noise control.",
    defaultSpan: 3,
  },
];
