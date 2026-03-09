import { STORAGE_KEYS } from "../../storageKeys";

/* ── Cockpit Layout — v2 free-form grid model ─────────────────────────────── */

const COCKPIT_V1_KEY = STORAGE_KEYS.cockpitPagesV1;
const COCKPIT_V2_KEY = STORAGE_KEYS.cockpitPagesV2;

/* ── Widget kinds ─────────────────────────────────────────────────────────── */

export const STRATEGY_COCKPIT_WIDGET_KINDS = [
  "strategy_summary",
  "blocked_work",
  "stale_work",
  "goal_progress",
  "project_spend",
  "approval_backlog",
] as const;

export type StrategyCockpitWidgetKind =
  (typeof STRATEGY_COCKPIT_WIDGET_KINDS)[number];

export type CockpitWidgetKind =
  | "health"
  | "focus"
  | "breakers"
  | "jobs"
  | "channels"
  | "profiles"
  | "skills"
  | "plugins"
  | "events"
  | StrategyCockpitWidgetKind;

const COCKPIT_WIDGET_KINDS: CockpitWidgetKind[] = [
  "health",
  "focus",
  "breakers",
  "jobs",
  "channels",
  "profiles",
  "skills",
  "plugins",
  "events",
  ...STRATEGY_COCKPIT_WIDGET_KINDS,
];

/* ── v2 position model ────────────────────────────────────────────────────── */

export interface CockpitWidgetPosition {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface WidgetSizeConstraint {
  minW: number;
  minH: number;
  defaultW: number;
  defaultH: number;
}

export const WIDGET_SIZE_CONSTRAINTS: Record<CockpitWidgetKind, WidgetSizeConstraint> = {
  health:            { minW: 6, minH: 2, defaultW: 12, defaultH: 2 },
  focus:             { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
  breakers:          { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
  jobs:              { minW: 4, minH: 3, defaultW: 6, defaultH: 5 },
  channels:          { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
  profiles:          { minW: 6, minH: 4, defaultW: 8, defaultH: 5 },
  skills:            { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
  plugins:           { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
  events:            { minW: 4, minH: 3, defaultW: 6, defaultH: 5 },
  strategy_summary:  { minW: 6, minH: 3, defaultW: 12, defaultH: 3 },
  blocked_work:      { minW: 4, minH: 3, defaultW: 6, defaultH: 5 },
  stale_work:        { minW: 4, minH: 3, defaultW: 6, defaultH: 5 },
  goal_progress:     { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
  project_spend:     { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
  approval_backlog:  { minW: 4, minH: 3, defaultW: 6, defaultH: 4 },
};

const CUSTOM_WIDGET_SIZE_CONSTRAINTS: WidgetSizeConstraint = {
  minW: 3,
  minH: 2,
  defaultW: 4,
  defaultH: 3,
};

/* ── Custom widget config (Phase 5) ───────────────────────────────────────── */

export interface CustomWidgetConfig {
  data_source: string;
  display_mode: "stat-card" | "table" | "list" | "kv-pairs";
  title: string;
  refresh_interval_ms: number;
  response_path?: string;
  params?: Record<string, string>;
}

/* ── v2 types ─────────────────────────────────────────────────────────────── */

export interface CockpitWidgetLayoutV2 {
  instance_id: string;
  widget: CockpitWidgetKind | "custom";
  title: string;
  position: CockpitWidgetPosition;
  custom_config?: CustomWidgetConfig;
}

export interface CockpitPageLayoutV2 {
  page_id: string;
  name: string;
  widgets: CockpitWidgetLayoutV2[];
}

/* ── v1 types (legacy, for migration) ─────────────────────────────────────── */

interface CockpitWidgetLayoutV1 {
  instance_id: string;
  widget: CockpitWidgetKind;
  title: string;
  span: number;
}

interface CockpitPageLayoutV1 {
  page_id: string;
  name: string;
  widgets: CockpitWidgetLayoutV1[];
}

/* ── Palette entry ────────────────────────────────────────────────────────── */

export interface CockpitWidgetPaletteEntry {
  widget: CockpitWidgetKind;
  title: string;
  description: string;
  icon: string;
}

export const COCKPIT_WIDGET_PALETTE: CockpitWidgetPaletteEntry[] = [
  {
    widget: "health",
    title: "Pinned Health Strip",
    description: "Gateway status, approvals, channels, and scheduler safety posture.",
    icon: "heart-pulse",
  },
  {
    widget: "focus",
    title: "Focus Queue",
    description: "Operator attention queue with approvals, failures, and incident actions.",
    icon: "target",
  },
  {
    widget: "breakers",
    title: "Breaker Radar",
    description: "Circuit breaker and plugin breaker state with cooldown windows.",
    icon: "zap-off",
  },
  {
    widget: "jobs",
    title: "Scheduler Matrix",
    description: "Upcoming jobs and direct run/pause controls.",
    icon: "calendar-clock",
  },
  {
    widget: "channels",
    title: "Channel Ops",
    description: "Adapter health and one-click reconnect operations.",
    icon: "radio",
  },
  {
    widget: "profiles",
    title: "Agent Routing",
    description: "Edit per-agent provider profile order without shell access.",
    icon: "route",
  },
  {
    widget: "skills",
    title: "Skills",
    description: "Toggle skills and inspect source paths/status.",
    icon: "brain",
  },
  {
    widget: "plugins",
    title: "Plugins",
    description: "Inspect plugin runtime health and enable/disable safely.",
    icon: "plug",
  },
  {
    widget: "events",
    title: "Event Tail",
    description: "Live operational event stream with noise control.",
    icon: "activity",
  },
  {
    widget: "strategy_summary",
    title: "Strategy Summary",
    description: "Blocked work, stale work, approvals, and spend from the strategy layer.",
    icon: "compass",
  },
  {
    widget: "blocked_work",
    title: "Blocked Work",
    description: "Track blocked strategy tasks and jump directly into the task detail view.",
    icon: "octagon-alert",
  },
  {
    widget: "stale_work",
    title: "Stale Work",
    description: "Surface stale strategy tasks before execution drifts too far.",
    icon: "clock-3",
  },
  {
    widget: "goal_progress",
    title: "Goal Progress",
    description: "Compare goal progress, open task counts, and blocked work at a glance.",
    icon: "flag",
  },
  {
    widget: "project_spend",
    title: "Project Spend",
    description: "Inspect project spend rollups and jump into the associated goal or project.",
    icon: "coins",
  },
  {
    widget: "approval_backlog",
    title: "Approval Backlog",
    description: "Watch critical approvals and open the linked task when one is attached.",
    icon: "badge-alert",
  },
];

/* ── Defaults ─────────────────────────────────────────────────────────────── */

export function defaultCockpitPages(): CockpitPageLayoutV2[] {
  return [
    {
      page_id: `page-${Date.now()}`,
      name: "Dashboard",
      widgets: [],
    },
  ];
}

export function opsDefaultTemplate(): CockpitPageLayoutV2 {
  const widgets: CockpitWidgetLayoutV2[] = [];
  let cursorX = 0;
  let cursorY = 0;
  let rowMaxH = 0;

  const templateItems: { kind: CockpitWidgetKind; title: string }[] = [
    { kind: "health", title: "Pinned Health Strip" },
    { kind: "focus", title: "Incident Queue" },
    { kind: "breakers", title: "Breaker Radar" },
    { kind: "jobs", title: "Scheduler Matrix" },
    { kind: "channels", title: "Channel Control" },
    { kind: "profiles", title: "Agent Provider Routing" },
    { kind: "skills", title: "Skills Control" },
    { kind: "plugins", title: "Plugins Control" },
    { kind: "events", title: "Event Tail" },
  ];

  for (const item of templateItems) {
    const constraints = WIDGET_SIZE_CONSTRAINTS[item.kind];
    const w = constraints.defaultW;
    const h = constraints.defaultH;

    if (cursorX + w > 12) {
      cursorX = 0;
      cursorY += rowMaxH || h;
      rowMaxH = 0;
    }

    widgets.push({
      instance_id: `${item.kind}-template`,
      widget: item.kind,
      title: item.title,
      position: { x: cursorX, y: cursorY, w, h },
    });

    cursorX += w;
    rowMaxH = Math.max(rowMaxH, h);
    if (cursorX >= 12) {
      cursorX = 0;
      cursorY += rowMaxH;
      rowMaxH = 0;
    }
  }

  return {
    page_id: "ops-default",
    name: "Ops Default",
    widgets,
  };
}

/* ── v1→v2 migration ──────────────────────────────────────────────────────── */

function migrateV1ToV2(v1Pages: CockpitPageLayoutV1[]): CockpitPageLayoutV2[] {
  return v1Pages.map((page) => {
    const widgets: CockpitWidgetLayoutV2[] = [];
    let cursorX = 0;
    let cursorY = 0;
    let rowMaxH = 0;

    for (const w of page.widgets) {
      const constraints = WIDGET_SIZE_CONSTRAINTS[w.widget];
      if (!constraints) continue;

      const colSpan = Math.min(12, Math.max(1, Math.round(w.span * 3)));
      const widgetW = Math.max(constraints.minW, colSpan);
      const widgetH = constraints.defaultH;

      if (cursorX + widgetW > 12) {
        cursorX = 0;
        cursorY += rowMaxH || widgetH;
        rowMaxH = 0;
      }

      widgets.push({
        instance_id: w.instance_id,
        widget: w.widget,
        title: w.title,
        position: { x: cursorX, y: cursorY, w: widgetW, h: widgetH },
      });

      cursorX += widgetW;
      rowMaxH = Math.max(rowMaxH, widgetH);
    }

    return {
      page_id: page.page_id,
      name: page.name,
      widgets,
    };
  });
}

/* ── Validation helpers ───────────────────────────────────────────────────── */

function isCockpitWidgetKind(value: string): value is CockpitWidgetKind {
  return COCKPIT_WIDGET_KINDS.includes(value as CockpitWidgetKind);
}

function isValidWidgetKind(value: string): value is CockpitWidgetKind | "custom" {
  return value === "custom" || isCockpitWidgetKind(value);
}

function finiteNumber(value: unknown, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function clampPosition(pos: Partial<CockpitWidgetPosition>, kind: CockpitWidgetKind | "custom"): CockpitWidgetPosition {
  const constraints = kind === "custom"
    ? CUSTOM_WIDGET_SIZE_CONSTRAINTS
    : WIDGET_SIZE_CONSTRAINTS[kind];

  const clampedW = Math.max(
    constraints.minW,
    Math.min(12, Math.round(finiteNumber(pos.w, constraints.defaultW))),
  );
  const clampedH = Math.max(
    constraints.minH,
    Math.round(finiteNumber(pos.h, constraints.defaultH)),
  );
  let clampedX = Math.max(0, Math.min(11, Math.round(finiteNumber(pos.x, 0))));
  const clampedY = Math.max(0, Math.round(finiteNumber(pos.y, 0)));
  if (clampedX + clampedW > 12) {
    clampedX = Math.max(0, 12 - clampedW);
  }

  return {
    x: clampedX,
    y: clampedY,
    w: clampedW,
    h: clampedH,
  };
}

export function sanitizeCockpitPages(input: unknown): CockpitPageLayoutV2[] {
  if (!Array.isArray(input)) {
    return defaultCockpitPages();
  }
  const pages = input
    .map((item) => {
      const raw = item as Partial<CockpitPageLayoutV2>;
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
              const entry = widget as Partial<CockpitWidgetLayoutV2>;
              if (
                typeof entry.instance_id !== "string" ||
                !entry.instance_id.trim() ||
                typeof entry.widget !== "string" ||
                typeof entry.title !== "string"
              ) {
                return null;
              }
              if (!isValidWidgetKind(entry.widget)) {
                return null;
              }
              const position = clampPosition(
                (entry.position ?? {}) as Partial<CockpitWidgetPosition>,
                entry.widget,
              );
              const result: CockpitWidgetLayoutV2 = {
                instance_id: entry.instance_id.trim(),
                widget: entry.widget,
                title: entry.title.trim() || "Widget",
                position,
              };
              if (entry.widget === "custom" && entry.custom_config) {
                result.custom_config = entry.custom_config;
              }
              return result;
            })
            .filter((widget): widget is CockpitWidgetLayoutV2 => widget !== null)
        : [];
      return {
        page_id: raw.page_id.trim(),
        name: pageName,
        widgets,
      } satisfies CockpitPageLayoutV2;
    })
    .filter((page): page is CockpitPageLayoutV2 => page !== null);
  return pages.length > 0 ? pages : defaultCockpitPages();
}

/* ── Storage ──────────────────────────────────────────────────────────────── */

export function loadCockpitPagesFromStorage(): CockpitPageLayoutV2[] {
  if (typeof window === "undefined") {
    return defaultCockpitPages();
  }

  // Try v2 first
  const v2Raw = window.localStorage.getItem(COCKPIT_V2_KEY);
  if (v2Raw) {
    try {
      return sanitizeCockpitPages(JSON.parse(v2Raw) as unknown);
    } catch (error) {
      console.warn("[cockpitLayout] failed to parse v2 cockpit layout", error);
    }
  }

  // Auto-migrate from v1
  const v1Raw = window.localStorage.getItem(COCKPIT_V1_KEY);
  if (v1Raw) {
    try {
      const v1Parsed = JSON.parse(v1Raw) as CockpitPageLayoutV1[];
      if (Array.isArray(v1Parsed) && v1Parsed.length > 0) {
        const migrated = migrateV1ToV2(v1Parsed);
        persistCockpitPagesToStorage(migrated);
        window.localStorage.removeItem(COCKPIT_V1_KEY);
        return migrated;
      }
    } catch (error) {
      console.warn("[cockpitLayout] failed to parse v1 cockpit layout", error);
    }
  }

  return defaultCockpitPages();
}

export function persistCockpitPagesToStorage(pages: CockpitPageLayoutV2[]): void {
  if (typeof window === "undefined") {
    return;
  }
  window.localStorage.setItem(COCKPIT_V2_KEY, JSON.stringify(pages));
}
