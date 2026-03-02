# Cockpit Page Redesign — Free-Form Dashboard Builder

## Context

The cockpit page is Mission Control's configurable ops dashboard — the page operators should stare at all day. Currently it has critical problems:

1. **"Layout Studio" sidebar wastes ~35% of horizontal space** for a dropdown and a few buttons — most of which are only needed during editing
2. **Widgets are locked to a 4-column CSS grid** with only horizontal span (1-4). No height control. No free-form sizing.
3. **Movement is via Up/Down buttons** — no drag-and-drop despite `@dnd-kit` being installed
4. **New pages start with all 9 widgets pre-loaded** — the opposite of the Android home screen model the user wants (blank → add what you want)
5. **Violates 3 of 4 design Laws**: scroll bars everywhere, 10+ controls at rest, deeply nested actions

The user also wants a **widget builder** — custom widgets that can query any of the 49 existing API endpoints — because limiting operators to 9 hardcoded widget types when 23 read-only data sources exist is artificially restrictive.

## Design Decisions

**react-grid-layout over @dnd-kit** — RGL is purpose-built for dashboard builders (Grafana, Metabase use it). Provides free-form `{x, y, w, h}` positioning, drag-and-drop, resize handles, collision detection, and compaction. Building the same on `@dnd-kit` would be 1000+ lines of custom grid logic. `@dnd-kit` stays installed for future board DnD migration.

**12-column grid, 60px row height** — industry standard. A widget with `w=6, h=4` = half-width, 240px tall. Granular enough for any layout.

**Sidebar stays but slims down** — 48px icon rail for page navigation (always visible). The "Layout Studio" bulk goes away. Edit controls move to a floating canvas toolbar and widget picker modal.

**Blank default, template optional** — New cockpit = one empty page. Empty state offers "Add Widget" or "Load Ops Template" (applies the current 9-widget default as a preset, not the starting state).

**Storage migration v1→v2** — New localStorage key `mission_control.cockpit.pages.v2` with `{x, y, w, h}` per widget. Auto-migrates from v1 on first load. Falls back to defaults if corrupt.

---

## Phase 1: Layout Model + Storage Migration + Slim Sidebar

**Goal**: Replace data model, migrate storage, collapse sidebar. Widgets still render via CSS grid temporarily.

### Modify: `cockpitLayout.ts`

New v2 types alongside existing:
```ts
export interface CockpitWidgetPosition { x: number; y: number; w: number; h: number; }

export interface CockpitWidgetLayoutV2 {
  instance_id: string;
  widget: CockpitWidgetKind | "custom";
  title: string;
  position: CockpitWidgetPosition;
  custom_config?: CustomWidgetConfig; // Phase 5
}

export interface CockpitPageLayoutV2 {
  page_id: string;
  name: string;
  widgets: CockpitWidgetLayoutV2[];
}
```

Size constraints per widget kind:
```ts
export const WIDGET_SIZE_CONSTRAINTS: Record<CockpitWidgetKind, WidgetSizeConstraint> = {
  health:   { minW: 6, minH: 2, defaultW: 12, defaultH: 2 },
  focus:    { minW: 4, minH: 3, defaultW: 6,  defaultH: 4 },
  breakers: { minW: 4, minH: 3, defaultW: 6,  defaultH: 4 },
  jobs:     { minW: 4, minH: 3, defaultW: 6,  defaultH: 5 },
  channels: { minW: 4, minH: 3, defaultW: 6,  defaultH: 4 },
  profiles: { minW: 6, minH: 4, defaultW: 8,  defaultH: 5 },
  skills:   { minW: 4, minH: 3, defaultW: 6,  defaultH: 4 },
  plugins:  { minW: 4, minH: 3, defaultW: 6,  defaultH: 4 },
  events:   { minW: 4, minH: 3, defaultW: 6,  defaultH: 5 },
};
```

Migration function: `migrateV1ToV2()` converts `span` → `{x, y, w, h}` by filling 12-column grid left-to-right, row by row.

`defaultCockpitPages()` returns ONE empty page. New `opsDefaultTemplate()` returns the 9-widget preset.

### Modify: `CockpitPage.tsx`

Replace `mc-cockpit-grid` two-column layout (sidebar 0.35fr | canvas 0.65fr) with:
- 48px slim sidebar rail (vertical page tabs + add-page button)
- Canvas takes remaining space
- Remove "Layout Studio" `Surface` wrapper entirely

### Modify: `useCockpitController.ts`

All mutations updated for v2 model. Replace `moveCockpitWidget`/`resizeCockpitWidget` with `updateWidgetPosition(instanceId, Partial<CockpitWidgetPosition>)`. Add `loadTemplate()` for the Ops Default preset.

### Modify: `styles.css`

Rewrite `.mc-cockpit-grid` → `grid-template-columns: 48px minmax(0, 1fr)`. New `.mc-cockpit-sidebar-slim` styles.

### Validation
```bash
cd apps/mission-control && npm run typecheck && npm run lint && npm run build
```
Old v1 layouts auto-migrate. New pages start blank.

---

## Phase 2: react-grid-layout Integration

**Goal**: Replace CSS grid canvas with RGL for drag-and-drop + free-form resizing.

### Install
```bash
cd apps/mission-control && npm install react-grid-layout @types/react-grid-layout
```

### Create: `CockpitCanvas.tsx`

Wraps `WidthProvider(Responsive)` from RGL:
- `cols={{ lg: 12, md: 8, sm: 4, xs: 2 }}` — responsive breakpoints
- `rowHeight={60}`, `margin={[12, 12]}`
- `isDraggable={editMode}`, `isResizable={editMode}`
- `draggableHandle=".mc-widget-drag-handle"` — only header grip initiates drag
- `compactType="vertical"` — widgets compact upward
- Per-item `minW`/`minH` from `WIDGET_SIZE_CONSTRAINTS`
- `onLayoutChange` persists positions to controller

### Modify: `CockpitPage.tsx`

Replace `<div className="mc-cockpit-canvas">` with `<CockpitCanvas>`. Each widget rendered as `<div key={instance_id}>` child.

### Modify: `CockpitWidgetRenderer.tsx`

Add `.mc-widget-drag-handle` grip icon (lucide `GripVertical`) in header, visible only in edit mode. Set `overflow: hidden` on widget bodies.

### Modify: `useCockpitController.ts`

Add `handleLayoutChange(rglLayout)` that syncs RGL position changes back to widget state.

### Modify: `styles.css`

Import RGL styles, override to match dark theme:
```css
.react-grid-item.react-grid-placeholder {
  background: var(--accent); opacity: 0.15; border-radius: var(--radius-md);
}
```
Widget bodies: `overflow: hidden` (Law 1).

### Modify: `AppContent.tsx`

Update render prop wiring for the new canvas component.

### Validation
Widgets drag-and-drop, resize freely, positions persist. Zero scroll bars on canvas.

---

## Phase 3: Edit Mode Redesign + Widget Picker

**Goal**: Android home screen model — blank canvas, floating edit toolbar, widget picker overlay.

### Create: `CockpitEditToolbar.tsx`

Floating toolbar in edit mode only, top-right of canvas:
```
[ + Add Widget ] [ Rename Page ] [ ··· More ▾ ]
```
"More" dropdown: Export JSON, Import JSON, Delete Page, Load Template, Restore Defaults.

View mode: **0 controls** on page chrome. Edit mode: **3 controls**. Complies with Law 2.

Edit mode toggled via: pencil icon in sidebar page tab area, or Command Palette action.

### Create: `WidgetPickerModal.tsx`

Modal overlay with two tabs:
- **Built-in**: 3-column card grid of 9 widget types (icon, title, description). Click to add.
- **Custom**: Opens widget builder (Phase 5).

No scroll bars — 2 rows of 3 cards visible, paginated if needed (Law 1).

### Modify: `CockpitPage.tsx`

Empty state when `widgets.length === 0`:
```tsx
<EmptyState message="Your dashboard is empty.">
  <button onClick={openWidgetPicker}>Add Widget</button>
  <button onClick={loadOpsTemplate}>Load Ops Template</button>
</EmptyState>
```

Edit mode widget chrome: dashed border, drag handle in header, small "x" remove button top-right, resize handles at corners.

### Modify: Sidebar page management

- Vertical page tab buttons (first-letter avatar + name tooltip)
- Active = accent left-border
- "+" at bottom for new page
- Right-click context menu: rename, duplicate, delete
- Max 8 visible pages, pagination dots if more

### Validation
Empty canvas shows proper empty state. Widget picker adds widgets. 0 controls in view mode, 3 in edit mode. All actions ≤ 2 clicks (Law 4).

---

## Phase 4: Widget Internal Pagination + Law Compliance

**Goal**: Every widget operates within its fixed RGL height. Zero scroll bars. Zero Law violations.

### Create utility: `useWidgetPagination` hook

```ts
function useWidgetPagination(totalItems: number, containerRef: RefObject<HTMLElement>, itemHeight: number)
  → { page, setPage, pageSize, totalPages, visibleSlice }
```
Uses `ResizeObserver` to measure available height, computes `pageSize` dynamically. When widget is resized via RGL, pagination auto-adjusts.

### Widget-specific changes in `CockpitWidgetRenderer.tsx`

| Widget | Fix |
|--------|-----|
| focus | Paginated list, pageSize from available height |
| breakers | Tabbed sub-nav `[Core] [Plugin]`, paginated within each tab |
| jobs | Paginated list |
| channels | Paginated list |
| profiles | 2 dropdowns + paginated profile list |
| skills | Paginated list |
| plugins | Paginated list |
| events | Fixed N items, "Show more" links to Events tab |
| health | Already compact, no changes |

### CSS enforcement

Every `overflow-y: auto` on cockpit elements → `overflow: hidden` + pagination.

### Law compliance checklist

- **Law 1 (Zero scroll bars)**: All overflow hidden, all lists paginated
- **Law 2 (Caveman simplicity)**: View=0 controls, Edit=3 controls
- **Law 3 (Dropdowns over text)**: Widget picker=cards, page rename=only freeform input
- **Law 4 (Nothing buried)**: Add widget=2 clicks, every action ≤2 clicks

### Validation
Every widget paginates within fixed height. Resize triggers re-pagination. Zero scroll bars anywhere.

---

## Phase 5: Custom Widget Builder

**Goal**: Operators create custom dashboard widgets that query any of 23 read-only API endpoints, including parametric ones.

### Create: `cockpitDataSources.ts`

Registry mapping API function names to metadata, including parameter definitions:

```ts
export interface CockpitDataSourceParam {
  key: string;            // e.g. "boardId", "agentId"
  label: string;          // e.g. "Board", "Agent"
  resolver: string;       // API to fetch options: "listBoards", "listAgents", etc.
  resolverLabelField: string;  // e.g. "name"
  resolverValueField: string;  // e.g. "board_id"
}

export interface CockpitDataSource {
  id: string;
  label: string;
  category: string;
  description: string;
  responseShape: "object" | "array";
  sampleFields: string[];
  params?: CockpitDataSourceParam[];  // empty/undefined = zero-param endpoint
}
```

23 entries covering all read-only endpoints. Parametric endpoints include: `getBoard` (needs `boardId` dropdown from `listBoards`), `getAgentProviderProfileOrder` (needs `agentId` from `listAgents` + `provider` from `listAuthProfiles`), `getAgentMailThread` (needs `threadId` from `listAgentMailThreads`), `listAgentMailMessages` (needs `threadId`), `listAuthProfiles` (optional `provider` filter), `listApprovals` (optional `status` filter).

### Create: `cockpitApiRunner.ts`

Dynamic dispatch that maps `data_source` string → actual API function call, with parameter injection:
```ts
export async function runCockpitDataSource(
  id: string,
  settings: RuntimeConnectionSettings,
  params?: Record<string, string>  // e.g. { boardId: "abc-123" }
): Promise<unknown>
```
The runner resolves the function and injects params positionally based on the data source definition.

### Create: `CustomWidgetBuilderModal.tsx`

5-step wizard modal:
1. **Data Source** — dropdown grouped by category + description
2. **Parameters** (conditional) — if the selected data source has `params`, show live-fetched dropdown pickers for each (e.g. "Board" dropdown populated by `listBoards`, "Agent" dropdown populated by `listAgents`). Skipped for zero-param endpoints.
3. **Display Mode** — radio cards: `stat-card` | `table` | `list` | `kv-pairs`
4. **Configuration** — title, refresh interval dropdown (Manual/10s/30s/1m/5m), response path, display-mode-specific fields
5. **Preview** — live data fetch with selected params + render in mock widget card → "Add to Dashboard" button

Max 3-4 controls per step (Law 2 compliant). Parameter dropdowns are fetched on mount of Step 2 using the `resolver` API functions from the data source definition.

### Create: `CustomWidgetRenderer.tsx`

Self-contained component:
- Accepts `CustomWidgetConfig` (which now includes `params?: Record<string, string>`) + `RuntimeConnectionSettings`
- `useEffect` + `setInterval` for auto-refresh, passes stored params to `runCockpitDataSource`
- Extracts sub-field via `response_path` (dot-notation traversal)
- Renders by display mode:
  - **stat-card**: Large centered number + label
  - **table**: Paginated rows with column headers
  - **list**: Paginated vertical items
  - **kv-pairs**: Two-column key-value grid
- Error boundary per widget — failed fetch shows error chip, not crash

### Modify: `CockpitWidgetRenderer.tsx`

Add branch for `widget.widget === "custom"`:
```ts
if (widget.widget === "custom" && widget.custom_config) {
  return <CustomWidgetRenderer config={widget.custom_config} settings={settings} />;
}
```
Thread `settings: RuntimeConnectionSettings` into renderer (currently receives pre-fetched data).

### Modify: `cockpitLayout.ts`

Add `"custom"` to widget kind union. Add custom widget size constraints: `{ minW: 3, minH: 2, defaultW: 4, defaultH: 3 }`. Sanitizer validates `custom_config`.

### Modify: `AppContent.tsx`

Pass `settings` through the render prop to support custom widget data fetching.

### Validation
Builder completes all 5 steps. Custom widgets render real API data with correct params. Auto-refresh works. Persists to localStorage. Paginates within fixed height.

---

## File Inventory

### Files to modify
| File | Phases |
|------|--------|
| `features/cockpit/cockpitLayout.ts` | 1, 5 |
| `features/cockpit/CockpitPage.tsx` | 1, 2, 3 |
| `features/cockpit/CockpitWidgetRenderer.tsx` | 2, 3, 4, 5 |
| `features/cockpit/useCockpitController.ts` | 1, 2 |
| `app/AppContent.tsx` | 2, 5 |
| `styles.css` | 1, 2, 3, 4 |
| `package.json` | 2 |

### Files to create
| File | Phase |
|------|-------|
| `features/cockpit/CockpitCanvas.tsx` | 2 |
| `features/cockpit/CockpitEditToolbar.tsx` | 3 |
| `features/cockpit/WidgetPickerModal.tsx` | 3 |
| `features/cockpit/useWidgetPagination.ts` | 4 |
| `features/cockpit/cockpitDataSources.ts` | 5 |
| `features/cockpit/cockpitApiRunner.ts` | 5 |
| `features/cockpit/CustomWidgetBuilderModal.tsx` | 5 |
| `features/cockpit/CustomWidgetRenderer.tsx` | 5 |

### Existing code to reuse
| What | Where |
|------|-------|
| `Surface`, `Modal`, `EmptyState`, `Badge`, `Chip`, `InlineActions` | `src/ui/*.tsx` |
| `formatRelative` | `src/utils/datetime.ts` |
| All 23 read-only API functions | `src/lib/api.ts` |
| `RuntimeConnectionSettings` type | `src/lib/runtime.ts` |
| lucide-react icons (`GripVertical`, `Plus`, `Pencil`, `Trash2`, etc.) | Already installed |
| `@dnd-kit/*` | Installed, unused — stays for future board DnD |

## Phase Dependencies

```
Phase 1 (model + sidebar) → Phase 2 (RGL) → Phase 3 (edit mode + picker) → Phase 4 (pagination) → Phase 5 (custom widgets)
```

Each phase ships independently. Each is under the 500 LOC PR limit.

## Verification (per phase)

```bash
cd apps/mission-control
npm run typecheck
npm run lint
npm run build
```
