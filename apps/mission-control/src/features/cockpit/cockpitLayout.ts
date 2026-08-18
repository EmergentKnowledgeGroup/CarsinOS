import { STORAGE_KEYS } from "../../storageKeys";

/* ── Cockpit Layout — v2 free-form grid model ─────────────────────────────── */

const COCKPIT_V1_KEY = STORAGE_KEYS.cockpitPagesV1;
const COCKPIT_V2_KEY = STORAGE_KEYS.cockpitPagesV2;
const COCKPIT_V3_KEY = STORAGE_KEYS.cockpitPagesV3;

/* ── Widget kinds ─────────────────────────────────────────────────────────── */

export const STRATEGY_COCKPIT_WIDGET_KINDS = [
  "strategy_summary",
  "blocked_work",
  "stale_work",
  "goal_progress",
  "project_spend",
  "approval_backlog",
] as const;

export const RUNBOOK_COCKPIT_WIDGET_KINDS = [
  "runbook_summary",
  "runbook_attention",
] as const;

export type StrategyCockpitWidgetKind =
  (typeof STRATEGY_COCKPIT_WIDGET_KINDS)[number];
export type RunbookCockpitWidgetKind =
  (typeof RUNBOOK_COCKPIT_WIDGET_KINDS)[number];

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
  | StrategyCockpitWidgetKind
  | RunbookCockpitWidgetKind;

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
  ...RUNBOOK_COCKPIT_WIDGET_KINDS,
];

/* ── v2 position model ────────────────────────────────────────────────────── */

/** Grid column count — 10 cols gives a clean 5-per-row default (w=2 each). */
export const COCKPIT_GRID_COLS = 10;

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
  health:            { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  focus:             { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  breakers:          { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  jobs:              { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  channels:          { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  profiles:          { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  skills:            { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  plugins:           { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  events:            { minW: 2, minH: 3, defaultW: 2, defaultH: 4 },
  strategy_summary:  { minW: 4, minH: 3, defaultW: 10, defaultH: 3 },
  blocked_work:      { minW: 3, minH: 3, defaultW: 5, defaultH: 5 },
  stale_work:        { minW: 3, minH: 3, defaultW: 5, defaultH: 5 },
  goal_progress:     { minW: 3, minH: 3, defaultW: 5, defaultH: 4 },
  project_spend:     { minW: 3, minH: 3, defaultW: 5, defaultH: 4 },
  approval_backlog:  { minW: 3, minH: 3, defaultW: 5, defaultH: 4 },
  runbook_summary:   { minW: 4, minH: 3, defaultW: 10, defaultH: 3 },
  runbook_attention: { minW: 3, minH: 3, defaultW: 5, defaultH: 4 },
};

const CUSTOM_WIDGET_SIZE_CONSTRAINTS: WidgetSizeConstraint = {
  minW: 2,
  minH: 2,
  defaultW: 2,
  defaultH: 4,
};

/* ── Reflow helper — re-lays out widgets using current grid defaults ─────── */

function resolveWidgetSize(
  widget: CockpitWidgetLayoutV2,
  mode: "preserve-size" | "default-size",
): { w: number; h: number } {
  const constraints =
    widget.widget === "custom"
      ? CUSTOM_WIDGET_SIZE_CONSTRAINTS
      : WIDGET_SIZE_CONSTRAINTS[widget.widget];
  if (mode === "default-size") {
    return {
      w: constraints.defaultW,
      h: constraints.defaultH,
    };
  }
  return {
    w: Math.max(
      constraints.minW,
      Math.min(COCKPIT_GRID_COLS, Math.round(widget.position.w)),
    ),
    h: Math.max(constraints.minH, Math.round(widget.position.h)),
  };
}

export function packWidgetsRowFirst(
  widgets: CockpitWidgetLayoutV2[],
  mode: "preserve-size" | "default-size" = "preserve-size",
): CockpitWidgetLayoutV2[] {
  let cursorX = 0;
  let cursorY = 0;
  let rowMaxH = 0;

  return widgets.map((widget) => {
    const { w, h } = resolveWidgetSize(widget, mode);

    if (cursorX + w > COCKPIT_GRID_COLS) {
      cursorX = 0;
      cursorY += rowMaxH || h;
      rowMaxH = 0;
    }

    const result: CockpitWidgetLayoutV2 = {
      ...widget,
      position: { x: cursorX, y: cursorY, w, h },
    };

    cursorX += w;
    rowMaxH = Math.max(rowMaxH, h);
    if (cursorX >= COCKPIT_GRID_COLS) {
      cursorX = 0;
      cursorY += rowMaxH;
      rowMaxH = 0;
    }

    return result;
  });
}

export function reflowWidgetsToGrid(widgets: CockpitWidgetLayoutV2[]): CockpitWidgetLayoutV2[] {
  return packWidgetsRowFirst(widgets, "default-size");
}

const OPS_TEMPLATE_INSTANCE_IDS = new Set([
  "health-template",
  "focus-template",
  "breakers-template",
  "jobs-template",
  "channels-template",
  "profiles-template",
  "skills-template",
  "plugins-template",
  "events-template",
]);

function isLikelyCollapsedTemplateLayout(page: CockpitPageLayoutV2): boolean {
  if (page.widgets.length < 6) {
    return false;
  }

  const templateWidgetCount = page.widgets.filter((widget) =>
    OPS_TEMPLATE_INSTANCE_IDS.has(widget.instance_id),
  ).length;

  if (templateWidgetCount < 6) {
    return false;
  }

  const anchoredLeftCount = page.widgets.filter((widget) => widget.position.x === 0).length;
  const nearFullWidthCount = page.widgets.filter(
    (widget) => widget.position.w >= COCKPIT_GRID_COLS - 1,
  ).length;
  const bottomEdge = page.widgets.reduce(
    (maxY, widget) => Math.max(maxY, widget.position.y + widget.position.h),
    0,
  );
  const distinctColumns = new Set(page.widgets.map((widget) => widget.position.x)).size;

  const mostlyLeftStacked =
    anchoredLeftCount >= Math.ceil(page.widgets.length * 0.75) &&
    nearFullWidthCount >= 2;
  const veryTallForTemplate = bottomEdge >= 18 && distinctColumns <= 3;

  return mostlyLeftStacked || veryTallForTemplate;
}

function isLikelyCollapsedFullWidthStackLayout(page: CockpitPageLayoutV2): boolean {
  if (page.widgets.length < 5) {
    return false;
  }
  const anchoredLeftCount = page.widgets.filter((widget) => widget.position.x === 0).length;
  const nearFullWidthCount = page.widgets.filter(
    (widget) => widget.position.w >= COCKPIT_GRID_COLS - 1,
  ).length;
  const bottomEdge = page.widgets.reduce(
    (maxY, widget) => Math.max(maxY, widget.position.y + widget.position.h),
    0,
  );
  const distinctColumns = new Set(page.widgets.map((widget) => widget.position.x)).size;

  return (
    anchoredLeftCount >= Math.ceil(page.widgets.length * 0.75) &&
    nearFullWidthCount >= Math.ceil(page.widgets.length * 0.6) &&
    distinctColumns <= 2 &&
    bottomEdge >= page.widgets.length * 2
  );
}

/**
 * Detect layouts that need reflowing — either single-column stacks or
 * positions left over from a different grid column count (e.g. old 12-col).
 */
export function needsGridReflow(page: CockpitPageLayoutV2): boolean {
  if (page.widgets.length <= 1) return false;
  // Any widget that overflows the current grid boundary → old column count
  if (page.widgets.some((w) => w.position.x + w.position.w > COCKPIT_GRID_COLS)) return true;
  // Any overlapping widgets means the saved layout is invalid and should be healed.
  for (let i = 0; i < page.widgets.length; i += 1) {
    const a = page.widgets[i].position;
    for (let j = i + 1; j < page.widgets.length; j += 1) {
      const b = page.widgets[j].position;
      const overlaps =
        a.x < b.x + b.w &&
        a.x + a.w > b.x &&
        a.y < b.y + b.h &&
        a.y + a.h > b.y;
      if (overlaps) {
        return true;
      }
    }
  }
  // Some legacy template layouts were saved as very tall left-stacked cards.
  // Reflow those back into row-first geometry.
  if (isLikelyCollapsedTemplateLayout(page)) {
    return true;
  }
  // Guard against pages saved as near-full-width left stacks.
  if (isLikelyCollapsedFullWidthStackLayout(page)) {
    return true;
  }
  // All non-full-width widgets stacked at x=0 → single-column
  const narrow = page.widgets.filter((w) => w.position.w < COCKPIT_GRID_COLS);
  return narrow.length >= 3 && narrow.every((w) => w.position.x === 0);
}

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
  {
    widget: "runbook_summary",
    title: "Runbook Summary",
    description: "Watch live runbook state counts sourced from the shared runbook read model.",
    icon: "workflow",
  },
  {
    widget: "runbook_attention",
    title: "Runbook Attention",
    description: "Surface waiting, blocked, failed, and limited runbooks with direct open actions.",
    icon: "list-filter",
  },
];

/* ── Defaults ─────────────────────────────────────────────────────────────── */

export function defaultCockpitPages(): CockpitPageLayoutV2[] {
  const dashboardTemplate = opsDefaultTemplate();
  return [
    {
      page_id: "dashboard",
      name: "Dashboard",
      widgets: dashboardTemplate.widgets,
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

    if (cursorX + w > COCKPIT_GRID_COLS) {
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
    if (cursorX >= COCKPIT_GRID_COLS) {
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

const LEGACY_OPS_TEMPLATE_WIDGETS: CockpitWidgetLayoutV2[] = [
  { instance_id: "health-template", widget: "health", title: "Pinned Health Strip", position: { x: 0, y: 0, w: 12, h: 2 } },
  { instance_id: "focus-template", widget: "focus", title: "Incident Queue", position: { x: 0, y: 2, w: 6, h: 4 } },
  { instance_id: "breakers-template", widget: "breakers", title: "Breaker Radar", position: { x: 6, y: 2, w: 6, h: 4 } },
  { instance_id: "jobs-template", widget: "jobs", title: "Scheduler Matrix", position: { x: 0, y: 6, w: 6, h: 5 } },
  { instance_id: "channels-template", widget: "channels", title: "Channel Control", position: { x: 6, y: 6, w: 6, h: 4 } },
  { instance_id: "profiles-template", widget: "profiles", title: "Agent Provider Routing", position: { x: 0, y: 11, w: 8, h: 5 } },
  { instance_id: "skills-template", widget: "skills", title: "Skills Control", position: { x: 0, y: 16, w: 6, h: 4 } },
  { instance_id: "plugins-template", widget: "plugins", title: "Plugins Control", position: { x: 6, y: 16, w: 6, h: 4 } },
  { instance_id: "events-template", widget: "events", title: "Event Tail", position: { x: 0, y: 20, w: 6, h: 5 } },
];

/* NOTE: LEGACY_OPS_TEMPLATE_WIDGETS retains the old positions intentionally —
   it is only used by upgradeLegacyOpsTemplatePage() to *detect* the old layout
   and migrate it to the new 2-column default produced by opsDefaultTemplate(). */

function matchesWidgetPosition(
  left: CockpitWidgetLayoutV2,
  right: CockpitWidgetLayoutV2,
): boolean {
  return (
    left.instance_id === right.instance_id &&
    left.widget === right.widget &&
    left.title === right.title &&
    left.position.x === right.position.x &&
    left.position.y === right.position.y &&
    left.position.w === right.position.w &&
    left.position.h === right.position.h
  );
}

function upgradeLegacyOpsTemplatePage(
  page: CockpitPageLayoutV2,
): CockpitPageLayoutV2 {
  if (page.widgets.length !== LEGACY_OPS_TEMPLATE_WIDGETS.length) {
    return page;
  }
  const isLegacyTemplate = LEGACY_OPS_TEMPLATE_WIDGETS.every((legacyWidget) => {
    const existing = page.widgets.find(
      (widget) => widget.instance_id === legacyWidget.instance_id,
    );
    return existing ? matchesWidgetPosition(existing, legacyWidget) : false;
  });
  if (!isLegacyTemplate) {
    return page;
  }
  return {
    ...page,
    widgets: opsDefaultTemplate().widgets,
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

      const colSpan = Math.min(COCKPIT_GRID_COLS, Math.max(1, Math.round(w.span * 3)));
      const widgetW = Math.max(constraints.minW, colSpan);
      const widgetH = constraints.defaultH;

      if (cursorX + widgetW > COCKPIT_GRID_COLS) {
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
    Math.min(COCKPIT_GRID_COLS, Math.round(finiteNumber(pos.w, constraints.defaultW))),
  );
  const clampedH = Math.max(
    constraints.minH,
    Math.round(finiteNumber(pos.h, constraints.defaultH)),
  );
  let clampedX = Math.max(0, Math.min(COCKPIT_GRID_COLS - 1, Math.round(finiteNumber(pos.x, 0))));
  const clampedY = Math.max(0, Math.round(finiteNumber(pos.y, 0)));
  if (clampedX + clampedW > COCKPIT_GRID_COLS) {
    clampedX = Math.max(0, COCKPIT_GRID_COLS - clampedW);
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
  return pages.length > 0
    ? pages.map(upgradeLegacyOpsTemplatePage)
    : defaultCockpitPages();
}

/* ── Storage ──────────────────────────────────────────────────────────────── */

export function loadCockpitPagesFromStorage(): CockpitPageLayoutV2[] {
  if (typeof window === "undefined") {
    return defaultCockpitPages();
  }

  // Try v3 first (current format — 2-column grid layout)
  const v3Raw = window.localStorage.getItem(COCKPIT_V3_KEY);
  if (v3Raw) {
    try {
      return sanitizeCockpitPages(JSON.parse(v3Raw) as unknown);
    } catch (error) {
      console.warn("[cockpitLayout] failed to parse v3 cockpit layout", error);
    }
  }

  // Auto-migrate from v2 — reflow widgets into the 2-column grid
  const v2Raw = window.localStorage.getItem(COCKPIT_V2_KEY);
  if (v2Raw) {
    try {
      const v2Pages = sanitizeCockpitPages(JSON.parse(v2Raw) as unknown);
      const reflowed = v2Pages.map((page) => ({
        ...page,
        widgets: reflowWidgetsToGrid(page.widgets),
      }));
      persistCockpitPagesToStorage(reflowed);
      window.localStorage.removeItem(COCKPIT_V2_KEY);
      return reflowed;
    } catch (error) {
      console.warn("[cockpitLayout] failed to migrate v2 cockpit layout", error);
    }
  }

  // Auto-migrate from v1
  const v1Raw = window.localStorage.getItem(COCKPIT_V1_KEY);
  if (v1Raw) {
    try {
      const v1Parsed = JSON.parse(v1Raw) as CockpitPageLayoutV1[];
      if (Array.isArray(v1Parsed) && v1Parsed.length > 0) {
        const migrated = migrateV1ToV2(v1Parsed);
        const reflowed = migrated.map((page) => ({
          ...page,
          widgets: reflowWidgetsToGrid(page.widgets),
        }));
        persistCockpitPagesToStorage(reflowed);
        window.localStorage.removeItem(COCKPIT_V1_KEY);
        return reflowed;
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
  window.localStorage.setItem(COCKPIT_V3_KEY, JSON.stringify(pages));
}
