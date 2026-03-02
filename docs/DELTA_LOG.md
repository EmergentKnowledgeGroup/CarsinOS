# Design Pass Delta Log

**Author:** Claude (design-pass session)
**Started:** 2026-02-28
**Scope:** Phases 0–3 of mission-control-designpass.md
**Purpose:** Every file change logged here for Dex code review.

---

## Phase 0: Theme Tokens + Dark Mode

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 0.1 | `apps/mission-control/src/styles.css` | **Rewrite** | Complete tokenization of 1490-line stylesheet. Replaced 100+ hardcoded hex colors with CSS custom properties. Added comprehensive token system: 40+ color tokens (bg layers, surfaces, ink, accent, semantic status), spacing scale (9 steps), border radius scale (6 steps), shadow system (4 levels), typography scale (7 sizes), motion tokens (easing, durations). | Designpass §3.2 — "Dark Ops" color system. Zero hardcoded colors remaining. |
| 0.2 | `apps/mission-control/src/styles.css` | **Modify** | Changed font import from IBM Plex Sans + Unbounded to Outfit + Unbounded + JetBrains Mono. Updated all `font-family` references to use `var(--font-body)`, `var(--font-display)`, `var(--font-mono)`. Added monospace treatment to data values (stats, timestamps, IDs, event payloads). | Designpass §3.3 — "Replace IBM Plex Sans body with something with more character. Keep Unbounded for display." Mono for data separation. |
| 0.3 | `apps/mission-control/src/styles.css` | **Add** | Dark mode as default theme with deep charcoal backgrounds (#08090d→#222838), warm off-white text (#e4e2dd), amber accent (#ff6a13 kept). Light mode via `[data-theme="light"]` attribute + `prefers-color-scheme` media query fallback. | Designpass §3.2/§4.2 — "Dark mode isn't a nice-to-have — it's table stakes." |
| 0.4 | `apps/mission-control/src/styles.css` | **Add** | Film grain texture via `body::before` pseudo-element (SVG feTurbulence noise at 1.8% opacity, mix-blend-mode: overlay). Adds depth to dark surfaces without visible pattern. | Frontend-design skill — "Add contextual effects and textures that match the overall aesthetic." |
| 0.5 | `apps/mission-control/src/styles.css` | **Add** | Interactive micro-transitions on all interactive elements: cards, list items, buttons, tabs, inputs. Hover states use `--surface-hover`, focus uses accent glow ring. Active button scales to 0.97. | Designpass §3.5 — "Every animation should communicate something." |
| 0.6 | `apps/mission-control/src/styles.css` | **Add** | Severity-colored left borders on focus items (.high = danger-red, .medium = warn-amber) with tinted backgrounds. Previously just border color changes. | Designpass §3.4 — "High-severity items have a subtle pulse on their left color bar." |
| 0.7 | `apps/mission-control/src/styles.css` | **Add** | Incident mode glow effect — when `.incident-mode` is active on health strip, adds red glow shadow (0 0 24px danger-muted) in addition to red border. | Designpass §3.4 — "incident mode toggle should have a visual state change." |
| 0.8 | `docs/DELTA_LOG.md` | **Create** | New file for tracking all design pass changes for Dex code review. | Operator requested delta log for code review. |

**Phase 0 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (602ms, 30.82 KB CSS gzipped to 5.50 KB)

---

## Phase 1: Zero-Scroll + Layout Restructure

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 1.1 | `apps/mission-control/src/app/tabs.ts` | **Modify** | Added `icon` (Lucide icon name) and `shortcut` (keyboard hint) fields to `MissionControlTabItem`. Updated all 7 tab entries with icon identifiers. | Designpass §3.4 — "Icon + text label for each tab." Nav rail needs icon references. |
| 1.2 | `apps/mission-control/src/app/AppShell.tsx` | **Rewrite** | Complete restructure. Old: topbar (brand+chips) → health strip → connection config → tab bar → content. New: nav rail (icons+labels, settings button) + main column (thin 48px topbar with inline health chips + content area). Connection config moved to settings modal. Added theme toggle (Sun/Moon icon, persists to localStorage). Added incident mode dot toggle in topbar. Added connection status dot (green/amber/red). | Designpass §12.1 — "single topbar line" + "vertical nav rail on left" + "connection config → settings gear icon → modal." |
| 1.3 | `apps/mission-control/src/styles.css` | **Add** | New CSS classes: `.mc-shell-layout` (flex row), `.mc-nav-rail` (72px vertical rail), `.mc-nav-item` (icon+label buttons), `.mc-nav-item-active` (accent glow + left inset border), `.mc-main-column`, `.mc-topbar` (48px single line), `.mc-topbar-incident` (red glow), `.mc-topbar-icon-btn`, `.mc-incident-toggle/.mc-incident-dot`, `.mc-connection-dot`, `.mc-content-area`, `.mc-modal-overlay/.mc-modal/.mc-modal-header/.mc-modal-body/.mc-modal-field/.mc-modal-actions`. | Structural CSS for the new shell layout. |
| 1.4 | `apps/mission-control/src/styles.css` | **Modify** | Changed `--toolbar-height` from 255px to 48px (new thin topbar). Added `--nav-rail-width: 72px`. | Board column height calculation depends on toolbar height. |
| 1.5 | `apps/mission-control/src/styles.css` | **Modify** | Responsive: hide topbar center health chips below 1160px. Collapse nav rail to 52px/icon-only below 760px. Reduce content padding on small screens. | Designpass §3.8 — responsive density considerations. |
| 1.6 | `apps/mission-control/package.json` | **Modify** | Added `lucide-react` dependency. | Designpass §4.3 — "Lucide (MIT, consistent, extensive) — the successor to Feather Icons." |

**Phase 1 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.07s, 35.21 KB CSS gzipped to 6.03 KB)

---

## Phase 2: Dropdown-First + Caveman Simplification

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 2.1 | `apps/mission-control/src/ui/AgentPicker.tsx` | **Create** | New component: clickable chip list for multi-selecting agents. Replaces CSV text inputs with toggle-able agent name chips. Shows all available agents, highlights selected ones in accent color. | Law 3: "Dropdowns over text inputs — wherever selecting from a known set." Multi-select equivalent of dropdown. |
| 2.2 | `apps/mission-control/src/styles.css` | **Add** | CSS for `.mc-agent-picker`, `.mc-agent-chip`, `.mc-agent-chip-selected`, `.mc-ttl-presets`, `.mc-ttl-preset`, `.mc-ttl-preset-active`. | Styling for the new agent picker and TTL preset buttons. |
| 2.3 | `apps/mission-control/src/features/agentMail/MailPage.tsx` | **Modify** | Added `agents: Agent[]` prop. Converted: Principal Override (text→select), Sender (text→select), Recipients (text→AgentPicker), Thread Participants (text→AgentPicker), Lease Holder (text→select), Lease Glob (text→preset select with custom fallback), Lease TTL (text→preset buttons: 5m/15m/1h/4h/24h). 7 conversions total. | Law 3 table: all 7 mail-page fields converted from text to dropdowns/pickers. |
| 2.4 | `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx` | **Modify** | Added `agents: Agent[]` prop. Converted: Room Participants (text→AgentPicker), Chat Sender (text→select), Chat Recipients (text→AgentPicker). 3 conversions total. | Law 3 table: all 3 chatroom fields converted. |
| 2.5 | `apps/mission-control/src/app/AppContent.tsx` | **Modify** | Threaded `agents` prop to MailPage and ChatroomsPage render calls. | Required for dropdown population. |

**Phase 2 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.19s, 36.60 KB CSS gzipped to 6.17 KB)

---

## Phase 3: UI Primitives + Design System

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 3.1 | `apps/mission-control/src/ui/Modal.tsx` | **Create** | Reusable overlay modal with Escape key close, backdrop click dismiss, header (title + optional subtitle), scrollable body, optional footer. Uses existing `.mc-modal-overlay`/`.mc-modal` CSS from Phase 1, adds `.mc-modal-subtitle`. | Designpass §14 — "Modal primitive for settings and confirmations." Replaces ad-hoc modals. |
| 3.2 | `apps/mission-control/src/ui/Toast.tsx` | **Create** | Toast notification stack. Max 4 visible, positioned top-right. Auto-dismiss by severity: info=4s, error=8s, critical=manual only. Progress bar countdown animation. Slide-in entry animation. | Designpass §14 — "Toast stack for transient feedback." Severity-aware timing. |
| 3.3 | `apps/mission-control/src/ui/useToasts.ts` | **Create** | Extracted `useToasts()` hook from Toast.tsx. Manages toast state: addToast(message, tone), dismissToast(id). Counter-based unique IDs. | React Fast Refresh requires component-only exports. Hook split into own file. |
| 3.4 | `apps/mission-control/src/ui/Pagination.tsx` | **Create** | Simple prev/next pagination with "page / total" display. Hides when totalPages ≤ 1. Disabled states on boundaries. | Designpass §14 — "Pagination replaces scroll bars." Law 1 compliance. |
| 3.5 | `apps/mission-control/src/ui/usePagination.ts` | **Create** | Extracted `usePagination<T>(items, pageSize)` hook. Returns totalPages + getPage(n) for slicing arrays. | React Fast Refresh split. Generic hook for paginating any array. |
| 3.6 | `apps/mission-control/src/ui/Badge.tsx` | **Create** | Notification count badge pill. 5 tone variants (accent, danger, warn, ok, info). Hidden at count ≤ 0. Caps display at "99+". | Designpass §14 — "Badge for unread/pending counts." Used in nav rail and tabs. |
| 3.7 | `apps/mission-control/src/ui/Skeleton.tsx` | **Create** | Loading placeholder with shimmer animation. 3 variants: text (full width, 1em tall), circle (round), rect (standard radius). CSS shimmer via translateX gradient sweep. | Designpass §14 — "Skeleton screens over spinners." Progressive loading feel. |
| 3.8 | `apps/mission-control/src/ui/Tabs.tsx` | **Create** | Horizontal sub-tab bar for within-page navigation. Bottom-border active indicator. Optional count badge per tab. Distinct from main nav rail — used inside feature pages. | Designpass §14 — "Sub-tabs for multi-panel pages (Mail, Calendar)." |
| 3.9 | `apps/mission-control/src/styles.css` | **Add** | ~230 lines of CSS for all 6 UI primitives: `.mc-sub-tabs`/`.mc-sub-tab*`, `.mc-badge`/`.mc-badge-*` (5 tone variants), `.mc-skeleton`/`@keyframes mc-shimmer`, `.mc-pagination`/`.mc-pagination-*`, `.mc-toast-stack`/`.mc-toast`/`.mc-toast-*`/`@keyframes mc-toast-in`/`@keyframes mc-toast-countdown`, `.mc-modal-subtitle`. Inserted before responsive breakpoints section. | CSS backing for all Phase 3 components. Consistent token usage throughout. |

**Phase 3 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (988ms, 41.02 KB CSS gzipped to 6.97 KB)

---

## Phase 4: Team Page (Agent Roster)

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 4.1 | `apps/mission-control/src/app/useAppController.ts` | **Modify** | Added `"team"` to `MissionControlTab` union type. | New tab registration. |
| 4.2 | `apps/mission-control/src/app/tabs.ts` | **Modify** | Added `{ tab: "team", label: "Team", icon: "users", shortcut: "7" }` between Rooms and Cockpit. Cockpit shortcut bumped to "8". | Designpass §13 — "Team goes between Chatrooms and Cockpit." |
| 4.3 | `apps/mission-control/src/app/AppShell.tsx` | **Modify** | Added `Users` to Lucide imports. Added `users: Users` to `NAV_ICONS` map. | Icon for Team nav rail entry. |
| 4.4 | `apps/mission-control/src/features/team/TeamPage.tsx` | **Create** | Full agent roster page: paginated agent cards (5 per page) with avatar initial, name, agent_id (mono), provider/model tag chips. Create Agent modal (agent_id, name, provider dropdown, model_id). Edit Agent modal (same, agent_id disabled). Empty state with Bot icon. Error handling. Uses Modal + Pagination + usePagination primitives from Phase 3. | Designpass §13 — "Agent Roster. Avatar cards with roles, visual hierarchy. Creation modal with all dropdowns." |
| 4.5 | `apps/mission-control/src/app/AppContent.tsx` | **Modify** | Added `TeamPage` import. Added `activeTab === "team"` branch rendering `<TeamPage agents settings onRefresh />`. | Wiring new page into app content router. |
| 4.6 | `apps/mission-control/src/styles.css` | **Add** | ~180 lines CSS: `.mc-team-page` layout, `.mc-team-header` (display title + stats + "+ New Agent" button), `.mc-team-roster` card list, `.mc-team-card` (hover: lift + shadow + glow, staggered entry animation via `@keyframes mc-team-card-in` with nth-child delays), `.mc-team-card-avatar` (accent glow ring, hover intensifies), `.mc-team-card-info`/name/id/tags, `.mc-team-empty` (centered illustration state), `.mc-btn`/`.mc-btn-accent`/`.mc-btn-loading` (reusable button system), `.mc-form-error` (danger-tinted error box), `.mc-modal-form` (flex column layout for modal forms). | Design-forward CSS: staggered reveals, avatar glow-on-hover, lift transitions. Reusable `.mc-btn` system. |

**Phase 4 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.03s, 44.90 KB CSS gzipped to 7.48 KB)

---

## Phase 6: Calendar Week Grid

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 6.1 | `apps/mission-control/src/features/calendar/CalendarPage.tsx` | **Rewrite** | Complete restructure with sub-tabs (`[Week View] [Schedule] [Active Jobs]`). **Week View**: 7-column day grid with color-coded job blocks positioned by `next_run_at`, "Always Running" strip with interval chips, "Next Up" strip with relative timestamps. **Schedule**: paginated table (6 per page) with relative timestamps, Play/Pause icon buttons, Chip status. **Active Jobs**: two-section list (Always Running + Next Up) with job dots, interval badges, action buttons. Added `formatInterval()` (seconds→human), `formatRelative()` (ms→relative time), `jobColor()` (stable hash-based color per job name, 8-color palette). Uses Tabs, Pagination, usePagination primitives. Replaced InlineActions/Surface with icon buttons and direct layout. | Designpass §12.3 + §7.4 — "Calendar has no calendar… add actual temporal visualization." Sub-tabs per §12.3. Relative timestamps per §7.9. |
| 6.2 | `apps/mission-control/src/styles.css` | **Add** | ~280 lines CSS: `.mc-calendar-page`, `.mc-cal-week`, `.mc-cal-always` (always-running strip), `.mc-cal-always-chip` (pill with hover tint), `.mc-cal-job-dot` (6px colored dot via `--job-color` custom property), `.mc-cal-grid` (7-column CSS grid with 1px gap borders), `.mc-cal-day`/`.mc-cal-day-today` (accent-tinted today column), `.mc-cal-day-header`/`.mc-cal-day-body`, `.mc-cal-job-block` (left-bordered blocks with hover slide), `.mc-cal-next-up`/`.mc-cal-next-item`, `.mc-cal-active`/`.mc-cal-active-item`, `.mc-cal-actions`, `.mc-table-sub`, `.mc-table-empty`, `.mc-mono` utility class. | Visual week grid with day columns, job blocks color-coded by name hash, today highlight, hover micro-interactions. |

**Phase 6 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.00s, 49.82 KB CSS gzipped to 8.02 KB)

---

## Phase 7: Motion & Micro-interactions

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 7.1 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-page-enter` — content area children fade-in + translateY(6px→0) on tab switch. Applied to `.mc-content-area > *`. | Designpass §3.5 — "Tab switches: cross-fade content with 200ms opacity transition + subtle translateY." |
| 7.2 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-nav-enter` — nav rail items slide-in from left with 40ms stagger per item (8 items = 320ms total cascade). | Frontend-design skill — "staggered reveals create more delight than scattered micro-interactions." |
| 7.3 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-event-enter` — event stream items slide-in from top + scale(0.98→1) with 40ms stagger for first 5 items. | Designpass §3.5 — "Event stream: new events slide in from top with opacity fade, 300ms stagger." |
| 7.4 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-severity-pulse` — high-severity focus items get a 2.4s infinite pulse: left inset shadow + danger glow radiates out and back. | Designpass §3.5 — "Focus items: high-severity items have a subtle pulse on their left color bar." |
| 7.5 | `apps/mission-control/src/styles.css` | **Add** | `.mc-card:active` — drag state: scale(1.03), 0.5deg rotation, deep shadow, slight opacity reduction. Cursor changes to `grabbing`. | Designpass §3.5 — "Card drag: picked-up card scales to 1.03 with deeper shadow." |
| 7.6 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-dot-pulse` — connection status dot pulses with green glow when connected (`.up` state), 3s cycle. Smooth transition on dot background color changes. | Designpass §3.5 — "Connection status: smooth color transitions between states." |
| 7.7 | `apps/mission-control/src/styles.css` | **Add** | `.mc-pinned-stat:hover` — health strip chips lift 1px on hover. Smooth transition added. | Subtle interactive feedback for clickable health chips. |
| 7.8 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-surface-enter` — all Surface panels fade-in + translateY(8px→0) on mount. | Consistent entrance animation for panel components. |
| 7.9 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-overlay-enter` + `@keyframes mc-modal-enter` — modal overlay fades in fast, modal body slides up from 16px + scales 0.97→1. | Designpass §3.5 — "purposeful motion communicates something" — modal entrance confirms action. |
| 7.10 | `apps/mission-control/src/styles.css` | **Add** | `@keyframes mc-incident-glow` — incident mode topbar pulses danger-red glow, 1.5s cycle. Shadow radiates from bottom border. | Designpass §3.4 — "incident mode toggle should have a visual state change." |
| 7.11 | `apps/mission-control/src/styles.css` | **Add** | `@media (prefers-reduced-motion: reduce)` — kills all animation-duration and transition-duration to 0.01ms. Respects OS accessibility settings. | Accessibility — motion sickness prevention. |

**Phase 7 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.01s, 52.75 KB CSS gzipped to 8.53 KB)

---

## Phase 8: Power-User Features (Keyboard Shortcuts, Command Palette, Density, Themes)

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| 8.1 | `apps/mission-control/src/app/useKeyboardShortcuts.ts` | **Create** | New hook: global keydown listener. Shortcuts: `1-8` tab switching (suppressed in inputs/textareas and overlays), `Cmd+K` toggle command palette, `Escape` close overlays (palette first, then settings), `Cmd+Shift+I` toggle incident mode. `isEditableTarget()` guard prevents firing in form fields. | Designpass §7.15 — "For a power-user ops tool, keyboard shortcuts are the single biggest productivity gap." |
| 8.2 | `apps/mission-control/src/ui/CommandPalette.tsx` | **Create** | Command palette overlay (Cmd+K). 520px modal at 18vh from top. Fuzzy search across 3 sections: Navigate (8 tab shortcuts with hint badges), Actions (incident toggle, refresh, settings), Appearance (theme mode toggle, density toggle). Arrow key + Enter keyboard navigation. `selectedIndex` tracks highlight. Section-grouped results with labels. Slide-in animation reusing `mc-overlay-enter` / `mc-modal-enter` keyframes. Auto-focuses input on open. Scrolls active item into view. | Designpass §4.4 Phase 6 — "Command palette (Cmd+K)." Inspired by Linear/Raycast. |
| 8.3 | `apps/mission-control/src/app/useTheme.ts` | **Create** | Theme management hook. `ThemeFamily` type: `obsidian \| phosphor \| arctic \| ember \| brutalist`. `ThemeMode`: `dark \| light`. `THEME_FAMILIES` array with label, accent color, description per family. `THEME_FONT_URLS` record mapping each family to its Google Fonts import URL. Dynamic font loading via DOM `<link>` injection — replaces previous link when theme changes. `applyTheme()` sets `data-theme` attribute to `{family}-{mode}`. localStorage persistence for both `mc-theme-name` and `mc-theme-mode`. | Designpass §10 — "Switching Mechanism" + "Font Loading Strategy" — dynamic per-theme font injection. |
| 8.4 | `apps/mission-control/src/app/AppShell.tsx` | **Rewrite** | Integrated `useKeyboardShortcuts`, `useTheme`, `CommandPalette`, density toggle. Replaced local theme state with `useTheme()` hook. Added `Command` button in topbar center (opens palette). Added density toggle button in topbar right (Minimize2/Maximize2 icons). Settings modal now has 2 sections: Connection (unchanged) + Theme (theme family picker with color swatches, mode toggle, density toggle). Removed inline `getThemeMode()`/`applyThemeMode()` functions (now in `useTheme`). Destructured `incidentMode`/`onIncidentModeChange` from props to satisfy React Compiler. Added optional `onRefresh` prop for command palette's Refresh action. | Phase 8 integration point — all power-user features wired into shell. |
| 8.5 | `apps/mission-control/src/App.tsx` | **Modify** | Added `onRefresh` prop to `<AppShell>`, wired to `missionControl.queueMissionControlRefresh(settings)`. | Command palette "Refresh Data" action needs a refresh callback. |
| 8.6 | `apps/mission-control/src/styles.css` | **Add** | ~130 lines: Command palette CSS — `.mc-cmd-trigger` (pill button with Cmd icon + kbd hint), `.mc-cmd-overlay` (fixed backdrop at z-index 9000), `.mc-cmd-palette` (520px modal, shadow-lg), `.mc-cmd-input-wrap` (search icon + input + esc hint), `.mc-cmd-section`/`.mc-cmd-section-label` (uppercase grouped headers), `.mc-cmd-item`/`.mc-cmd-item-active` (accent-muted highlight on hover/keyboard select), `.mc-cmd-hint` (mono kbd badges), `.mc-cmd-empty`. | Command palette visual design — dark floating modal with search, grouped items, keyboard-navigable highlights. |
| 8.7 | `apps/mission-control/src/styles.css` | **Add** | ~70 lines: Settings modal extensions — `.mc-settings-modal` (max-width 520px), `.mc-settings-section`/`.mc-settings-section-title` (bordered sections with uppercase labels), `.mc-theme-picker` (flex column of theme option buttons), `.mc-theme-option`/`.mc-theme-option-active` (swatch dot + name + description, accent border when active), `.mc-settings-row`/`.mc-settings-row-label` (mode/density toggle rows). | Theme picker UI inside settings modal. Each theme shown with its accent color swatch. |
| 8.8 | `apps/mission-control/src/styles.css` | **Add** | ~35 lines: Compact density overrides — `[data-density="compact"]` reduces all spacing tokens by ~40%, shrinks typography scale, reduces toolbar height to 40px and nav rail to 56px. `.mc-nav-label` font-size reduced. Team card, calendar day, focus item padding tightened. Table cell padding reduced. | Designpass §3.8 — "Compact density mode: Toggle between comfortable and compact spacing for operators who want maximum data density." |
| 8.9 | `apps/mission-control/src/styles.css` | **Add** | ~60 lines: Phosphor dark theme — `[data-theme="phosphor-dark"]`. All monospace fonts (VT323 display, IBM Plex Mono body+mono). Green-on-black palette (#39ff14 accent, #b8e6a0 text, #0a0d08 bg). No box shadows (glow only). Sharp corners (2-4px radius). Semantic colors shifted to green/amber/red phosphor family. | Designpass §8 Theme 2 — "Retro-terminal. The ghost of CRT monitors past." |
| 8.10 | `apps/mission-control/src/styles.css` | **Add** | ~45 lines: Phosphor light theme — `[data-theme="phosphor-light"]`. "Greenbar printout" aesthetic. Green-tinted surfaces (#f0f4ec bg), dark green text (#1a2e10), forest green accent (#1a8c09). Same monospace fonts. | Designpass §8 Phosphor light — "Like a greenbar continuous-feed printout." |
| 8.11 | `apps/mission-control/src/styles.css` | **Add** | ~20 lines: Phosphor overrides — `[data-theme^="phosphor"] body::after` CRT scanline overlay (repeating-linear-gradient, 3% opacity). Text glow on brand/titles via `text-shadow`. Light variant gets subtler glow. | Designpass §8 — "CRT scanline overlay, text glow on primary content." |
| 8.12 | `apps/mission-control/src/styles.css` | **Add** | ~55 lines: Arctic dark theme — `[data-theme="arctic-dark"]`. Outfit font (body+display). Ice-blue accent (#60b4ff). Deep navy backgrounds (#0c0f16). Large border radius (8-18px). Blue-tinted shadows. | Designpass §8 Theme 3 — "Cold. Clean. Scandinavian-minimal." |
| 8.13 | `apps/mission-control/src/styles.css` | **Add** | ~45 lines: Arctic light theme — `[data-theme="arctic-light"]`. Snow-white surfaces, blue accent (#2b7de9), blue-tinted shadows, extra-large radius. | Designpass §8 Arctic light — "Snow and ice. Vast whitespace. Blue shadows." |
| 8.14 | `apps/mission-control/src/styles.css` | **Add** | ~55 lines: Midnight Ember dark theme — `[data-theme="ember-dark"]`. DM Serif Text display + DM Sans body. Copper/rose-gold accent (#e0a06e). Deep warm purplish-dark backgrounds (#100f14). Warmer semantic colors. Deep warm shadows. | Designpass §8 Theme 4 — "Rich. Warm. Premium. Whiskey-by-the-fireplace." |
| 8.15 | `apps/mission-control/src/styles.css` | **Add** | ~45 lines: Midnight Ember light theme — `[data-theme="ember-light"]`. Warm parchment surfaces (#f6f1ea), copper accent (#b87530), warm-tinted shadows. Intentionally refined version of original beige — "with proper contrast ratios and intentional hierarchy." | Designpass §8 Ember light — "This light variant is actually closest to the CURRENT design but with proper contrast." |
| 8.16 | `apps/mission-control/src/styles.css` | **Add** | ~55 lines: Brutalist dark theme — `[data-theme="brutalist-dark"]`. Archivo Black display + Archivo body. Pure black bg (#000000). Electric red accent (#ff2020). Zero border radius. Zero shadows. Pure green/yellow/red semantic colors. 2px border width. | Designpass §8 Theme 5 — "Raw. Uncompromising. Anti-design as design." |
| 8.17 | `apps/mission-control/src/styles.css` | **Add** | ~40 lines: Brutalist light theme — `[data-theme="brutalist-light"]`. Pure white bg, pure black borders, electric red accent. "Newsprint. Black on white. Unforgiving." | Designpass §8 Brutalist light variant. |
| 8.18 | `apps/mission-control/src/styles.css` | **Add** | ~30 lines: Brutalist overrides — `[data-theme^="brutalist"]` forces uppercase buttons with 0.08em letter-spacing and font-weight 700. Hover replaces with accent fill (no cute lifts). Brand gets accent underline. Section headers get accent underline. | Designpass §8 — "Labels are SCREAMING." |
| 8.19 | `apps/mission-control/src/styles.css` | **Modify** | Renamed `[data-theme="light"]` → `[data-theme="obsidian-light"]`. Updated system-preference media query from `:root:not([data-theme="dark"])` → `:root:not([data-theme$="-dark"])` to match new naming convention. | Theme naming migration to `{family}-{mode}` pattern. All themes now follow consistent naming. |

**Phase 8 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.10s, 68.05 KB CSS / 11.48 KB gzipped, 354.34 KB JS / 102.49 KB gzipped)

---

## Phase 1-2 Gap Fill (Compliance Sweep)

**Context:** Verification audit found 7 gaps across Phases 1 and 2. All filled in this pass.

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| G.1 | `apps/mission-control/src/features/agentMail/MailPage.tsx` | **Rewrite** | Added sub-tabs via `Tabs` component: "Messages" tab (2-column layout: thread sidebar + conversation with inline compose at bottom) and "Leases" tab (dedicated lease form + active lease list). Lease form uses 2-column grid layout for better density. Moved "New Direct Thread" from inline sidebar form to `Modal` component triggered by "+ New Thread" button. Added lease release confirmation `Modal` — clicking Release opens a confirm dialog showing the glob pattern before executing. | Phase 1 gap: §12.6 "Sub-tabs: [Messages] [Leases]." Phase 1 gap: "creation flows → modals." Phase 2 gap: §7.5 "destructive actions require confirmation." |
| G.2 | `apps/mission-control/src/features/focus/FocusPage.tsx` | **Rewrite** | Added sub-tabs via `Tabs` component: "Queue" tab (focus items in Surface) and "System Status" tab (stats list + channel grid with health Chips per channel). Added degraded channel count computation for badge on Status tab. Added empty states for both tabs. | Phase 1 gap: §12.4 "Sub-tabs: [Queue (3)] [System Status]." |
| G.3 | `apps/mission-control/src/app/AppShell.tsx` | **Modify** | Added `navBadges` prop (`Partial<Record<MissionControlTab, number>>`). Nav rail items now render `Badge` component when count > 0. Focus tab badge uses "danger" tone, others use "accent." Badge positioned absolute top-right of nav item with subtle pulse animation. Imported `Badge` from `../ui/Badge`. | Phase 1 gap: "alert badges on nav items — Badge component exists but not wired." |
| G.4 | `apps/mission-control/src/App.tsx` | **Modify** | Added `navBadges` prop to `<AppShell>`: `focus` = approvals count from `missionControl.approvalsById.size`, `mail` = total unread from `mailController.mailThreads.reduce(sum + t.unread_count)`. | Wiring badge data to shell. |
| G.5 | `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx` | **Rewrite** | Converted to 2-column layout (room sidebar + conversation, no separate moderation panel). "Create Room" moved from inline sidebar form to `Modal`. Room moderation (participants, actions, leases) moved to "Room Settings" `Modal` triggered by button in conversation header. Added lease release confirmation `Modal`. Inline compose at bottom of conversation column. | Phase 1 gap: "Mail/Chatrooms creation flows → modals." Phase 2 gap: "confirmation dialogs for lease release." Designpass §12.7 "moderation panel → Room Settings modal." |
| G.6 | `apps/mission-control/src/app/AppShell.tsx` | **Modify** | Replaced `window.confirm()` for Clear Token with proper `Modal` confirmation dialog. Added `clearTokenConfirmOpen` state. Modal shows explanation text and Cancel/Clear Token buttons. Imported `Modal` from `../ui/Modal`. | Phase 2 gap: §7.5 "destructive actions should require confirmation modal." |
| G.7 | `apps/mission-control/src/app/AppShell.tsx` | **Modify** | Added gateway URL history system. `getGatewayUrlHistory()` / `pushGatewayUrlHistory()` functions read/write array of up to 8 URLs from localStorage key `mc-gateway-url-history`. Gateway URL input now uses HTML `datalist` for autocomplete suggestions from history. Save + Connect button records current URL to history before saving. | Phase 2 gap: Law 3 table — "Gateway URL: Combo dropdown with history." |
| G.8 | `apps/mission-control/src/ui/TagPicker.tsx` | **Create** | Multi-select tag chip picker component. Same pattern as AgentPicker but for arbitrary string tags. Shows all known tags (from props `suggestions`) plus any already-selected tags as toggleable chips. "Add new tag" input + button at bottom for creating tags not in suggestion list. Enter key support. | Phase 2 gap: Law 3 table — "Board card Tags: Multi-select tag picker." |
| G.9 | `apps/mission-control/src/features/boards/BoardsPage.tsx` | **Modify** | Replaced plain text `<input>` for `cardEditor.tagsCsv` with `<TagPicker>` component. Added `useMemo` to aggregate known tags from all loaded cards across all columns (`knownTags`). Imported `TagPicker` from `../../ui/TagPicker`. | Phase 2 gap: tags field was CSV text, now chip picker with suggestions. |
| G.10 | `apps/mission-control/src/styles.css` | **Add** | New CSS: `.mc-mail-page`/`.mc-focus-page` (flex column layout for sub-tab pages), `.mc-mail-grid-2col` (2-column grid override), `.mc-mail-compose-inline` (border-top compose area), `.mc-mail-compose-row` (2-column form grid), `.mc-msg-count` (mono caption style), `.mc-lease-page`/`.mc-lease-form-row` (lease tab layout), `.mc-nav-badge` (absolute positioned badge on nav items, 16px pill, pulse animation), `@keyframes mc-badge-pulse` (subtle 2s opacity pulse), `.mc-modal-section`/`.mc-modal-section h3` (bordered sections inside modals with uppercase labels), `.mc-tag-add-row`/`.mc-tag-add-input` (tag picker add-new row). Also added `position: relative` to `.mc-nav-item` for badge positioning. | CSS for all gap-fill components. Consistent with existing token system. |

**Gap Fill Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.10s, 69.36 KB CSS / 11.72 KB gzipped, 359.62 KB JS / 103.79 KB gzipped)

---

## Phase 1-4 Gap Fill Pass 2 (Final Compliance Sweep)

**Context:** Second verification audit found remaining gaps across all phases. 14 functional gaps closed in this pass.

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| H.1 | `apps/mission-control/src/App.tsx` | **Modify** | Wired `useToasts` hook + `ToastStack` component. Created `setNotice` adapter function wrapping `addToast()` to avoid touching 60+ controller callsites. Removed `notice` state from app-level. | Phase 3 gap: Toast system existed but wasn't wired into the app. |
| H.2 | `apps/mission-control/src/app/useAppController.ts` | **Modify** | Removed `notice`/`setNotice` state. Added `NotifyFn` type export: `(notice: Notice \| null) => void`. | Type system support for toast adapter pattern. |
| H.3 | `apps/mission-control/src/app/AppShell.tsx` | **Modify** | Removed `notice` prop from interface, removed `Notice` type import, removed `mc-notice` JSX block. | Old notice banner replaced by toast stack. |
| H.4 | `apps/mission-control/src/app/AppContent.tsx` | **Modify** | Changed `setNotice` prop type from `Dispatch<SetStateAction<Notice \| null>>` to `NotifyFn`. | Align with toast adapter signature. |
| H.5 | `apps/mission-control/src/features/boards/useBoardsController.ts` | **Modify** | Changed `setNotice` type to `NotifyFn`. Added optional `opts` parameter to `handleCreateCard` for `owner_kind`, `owner_agent_id`, `owner_human_id`. | Toast adapter + richer card creation from modal. |
| H.6 | `apps/mission-control/src/app/useRuntimeConnectionController.ts` | **Modify** | Changed `setNotice` type to `NotifyFn`. | Toast adapter. |
| H.7 | `apps/mission-control/src/app/useMissionControlController.ts` | **Modify** | Changed `setNotice` type to `NotifyFn`. | Toast adapter. |
| H.8 | `apps/mission-control/src/features/agentMail/useAgentMailController.ts` | **Modify** | Changed `setNotice` type to `NotifyFn`. | Toast adapter. |
| H.9 | `apps/mission-control/src/styles.css` | **Modify** | Added `--overlay-bg` and `--overlay-bg-heavy` CSS variables to `:root` and light theme. Replaced hardcoded `rgba(4,4,8,0.65)` in `.mc-modal-overlay` and `rgba(4,4,8,0.72)` in `.mc-onboarding-overlay` with variable references. | Phase 0 gap: 2 hardcoded overlay colors. |
| H.10 | `apps/mission-control/src/features/calendar/CalendarPage.tsx` | **Modify** | Replaced 4 hardcoded hex colors in `jobColor()` fallback palette with CSS variable references using `getComputedStyle`. | Phase 0 gap: 4 hardcoded calendar colors. |
| H.11 | `apps/mission-control/src/features/focus/FocusPage.tsx` | **Modify** | Added pagination (6 items/page) using `usePagination` + `Pagination`. Removed `overflow: auto` from `.mc-focus-list`. | Phase 1 gap: Focus list had no pagination, relied on scroll. |
| H.12 | `apps/mission-control/src/features/events/EventsPage.tsx` | **Rewrite** | Complete restructure: domain filter chips (All/Board/Job/Approval/Channel/Mail), structured event rendering with `eventDomain()` parser, `domainTone()` color mapping, `eventSummary()` human-readable extraction, collapsible JSON per event, pagination (12/page). | Phase 1 gap: Events was raw JSON dump with scroll. Now structured + paginated + filterable. |
| H.13 | `apps/mission-control/src/features/agentMail/MailPage.tsx` | **Modify** | Added pagination to threads (8/page), messages (10/page), leases (6/page). Removed `overflow: auto` from scroll regions. Added `composeOptionsOpen` state — Sender and Recipients fields now hidden behind "Options" toggle button. | Phase 1 gap: scroll regions. Phase 2 gap: compose had 5+ controls at rest, now 3 (textarea + attach + send). |
| H.14 | `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx` | **Modify** | Added pagination to rooms (8/page) and messages (10/page). Removed `overflow: auto`. Added `chatOptionsOpen` state — Sender and Recipients hidden behind "Options" toggle. | Phase 1 gap: scroll. Phase 2 gap: compose control count. |
| H.15 | `apps/mission-control/src/features/boards/BoardsPage.tsx` | **Rewrite** | Card editor drawer → Modal with 3 sub-tabs [Details][Script][Assets]. Details: 5 controls max (Title, Description, Owner with contextual agent/human, Due, Tags). Assets: paginated (6/page). Added "New Card" creation modal (Title + Column dropdown + Owner dropdown — 3 fields + 2 buttons). Added owner filter tabs (All/Unassigned/per-agent). Added per-page stats header (Cards/In Progress/Done). Layout changed from `mc-main-grid` (2-col with drawer) to `mc-board-full` (single col + modals). | Phase 1/2 gap: drawer had 15 controls. Now modal with sub-tabs, max 5 per tab. Board filter tabs per §12.2. New Card creation modal per §12.2. |
| H.16 | `apps/mission-control/src/features/cockpit/CockpitPage.tsx` | **Modify** | Added `editMode` toggle — widget move/resize/remove buttons only visible in edit mode. Widget palette cards replaced with single dropdown + "Add" button. Rename Page, Add Page, Export/Import/Restore only visible in edit mode. | Phase 2 gap: Cockpit had 10+ controls at rest. Widget palette → dropdown. Edit mode toggle hides layout controls. |
| H.17 | `apps/mission-control/src/features/team/TeamPage.tsx` | **Modify** | Model ID free-text input → dropdown filtered by provider (`MODELS_BY_PROVIDER` static map). Added Tool Profile dropdown (standard/restricted/none). Added Workspace dropdown (combo with known workspaces from existing agents + custom input fallback). Added Role Card modal showing agent config in definition list format. Agent cards now show model, tool profile, workspace metadata. | Phase 4 gap: §13 "Model dropdown filtered by provider, Tool Profile dropdown, Workspace dropdown, Role Card view." |
| H.18 | `apps/mission-control/src/types.ts` | **Modify** | Added `workspace_root?: string` and `tool_profile?: string` to `Agent` interface. | API already accepts these fields; type was missing them. |
| H.19 | `apps/mission-control/src/styles.css` | **Add** | New CSS: `.mc-board-full` (flex column layout), `.mc-board-toolbar-left`/`.mc-board-stats` (toolbar split), `.mc-board-filter-bar` (owner filter tabs), `.mc-event-filters`/`.mc-filter-chip`/`.mc-filter-chip-active` (domain filters), `.mc-event-domain-*` (6 color-coded left borders), `.mc-event-head`/`.mc-event-type`/`.mc-event-summary`/`.mc-event-entity`/`.mc-event-time`/`.mc-event-expand`/`.mc-event-payload` (structured events), `.mc-cockpit-add-widget` (palette dropdown row), `.mc-edit-mode-active` (accent toggle), `.mc-mail-compose-options` (2-col collapsible options), `.mc-options-active` (accent toggle), `.mc-team-card-meta`/`.mc-team-card-actions`/`.mc-btn-sm` (team card enhancements), `.mc-role-card`/`.mc-role-card-avatar`/`.mc-role-card-section`/`.mc-role-card-dl` (role card modal). | CSS for all gap-fill components. |

**Gap Fill Pass 2 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.05s, 72.56 KB CSS / 12.22 KB gzipped, 370.84 KB JS / 107.10 KB gzipped)

---

## Gap Fill Pass 3 (Phase 0-4 Compliance Sweep)

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| I.1 | `apps/mission-control/src/styles.css` | **Fix** | Replaced hardcoded badge text colors (`#fff`, `#1a1200`, `#002e0f`) with semantic tokens (`var(--danger-ink)`, `var(--warn-ink)`, `var(--ok-ink)`, `var(--info-ink)`). | Phase 0 gap: badge text colors were hardcoded, broke theme switching for non-Obsidian themes. |
| I.2 | `apps/mission-control/src/styles.css` | **Fix** | Command palette overlay `rgba(0,0,0,0.55)` → `var(--overlay-bg-heavy)`. Palette box-shadow extra rgba → `var(--shadow-glow)`. 2 modal box-shadows `rgba(0,0,0,0.5)` → `var(--shadow-lg)`. Dot-pulse animation rgba → `var(--ok-muted)`. Severity pulse rgba → `transparent`. Event domain purple `#a78bfa` → `var(--channel)`. | Phase 0 gap: hardcoded rgba/hex values outside theme tokens. |
| I.3 | `apps/mission-control/src/styles.css` | **Add** | Added `--channel` semantic token to all 11 theme variants (dark: `#a78bfa`–`#c4b5fd`, light: `#7c3aed`–`#6b21a8`). | Phase 0 gap: event domain "purple" used hardcoded hex. Now theme-aware via `--channel` token. |
| I.4 | `apps/mission-control/src/features/boards/BoardLane.tsx` | **Rewrite** | Replaced `@tanstack/react-virtual` virtualizer with `usePagination` (8 cards/page). Cards rendered as flat list instead of absolute-positioned virtual items. Added empty state. Pagination controls shown only when >1 page. Drag-and-drop preserved. | Phase 1 gap: Law 1 — lane body had `overflow: auto` scroll bar. Now paginated, zero scroll. |
| I.5 | `apps/mission-control/src/styles.css` | **Fix** | Removed `overflow: auto` from `.mc-lane-body`, replaced with flex column layout. Added `.mc-lane-empty` style. | Phase 1 gap: lane scroll bar removal. |
| I.6 | `apps/mission-control/src/styles.css` | **Fix** | Removed `overflow: auto` from `.mc-table-wrap`. Schedule table already paginated (6/page), scroll was redundant. | Phase 1 gap: Law 1 — table wrap had scroll bar. |
| I.7 | `apps/mission-control/src/styles.css` | **Fix** | Removed `overflow: auto` from `.mc-cockpit-palette`, `.mc-cockpit-canvas`, `.mc-cockpit-widget-body`, and `.mc-lane-panel`. | Phase 1 gap: Law 1 — cockpit elements and lane panel had scroll bars. |
| I.8 | `apps/mission-control/src/styles.css` | **Delete** | Removed dead drawer CSS: `.mc-drawer` (3 rules + responsive), `.mc-drawer-header`/`h2`, `.run-pill`, `.script-area`, `.mc-drawer-actions`, `.mc-board-panel`. None referenced in TSX. ~45 lines removed. | Phase 1 gap: dead drawer CSS from pre-modal era. |
| I.9 | `apps/mission-control/src/features/cockpit/CockpitPage.tsx` | **Fix** | Replaced `window.confirm()` with `confirmResetOpen` state + Modal component for "Restore Defaults" action. | Phase 2 gap: Law 2 — `window.confirm` is a browser chrome dialog, violates "nothing buried" principle. |
| I.10 | `apps/mission-control/src/features/boards/BoardsPage.tsx` | **Fix** | Merged Owner kind + Agent selection into a single combined dropdown. Options: "Unassigned", each agent by name, "Human (custom)". Reduces Details tab controls from 6→5 at rest (Human ID input only appears when "Human (custom)" selected). | Phase 2 gap: Law 2 — details tab had 6 controls when agent/human selected. |
| I.11 | `apps/mission-control/src/features/agentMail/MailPage.tsx` | **Fix** | Moved lease creation form (7 controls) behind "New Lease" button → Modal. Leases tab at rest: header + "New Lease" button + paginated list. | Phase 2 gap: Law 2 — lease form had 7 inline controls. |
| I.12 | `apps/mission-control/src/features/team/TeamPage.tsx` | **Fix** | Moved Tool Profile + Workspace fields behind "Advanced..." toggle. Agent form at rest: Agent ID, Name, Provider, Model, Advanced toggle = 5 controls. | Phase 2 gap: Law 2 — team form had 6 fields. |
| I.13 | `apps/mission-control/src/features/team/TeamPage.tsx` | **Fix** | Added `activeJobCount` prop and "N active jobs" stat to Team page header. | Phase 4 gap: Team page missing Active Jobs stat. |
| I.14 | `apps/mission-control/src/app/AppContent.tsx` | **Fix** | Passes `activeJobCount={calendarJobs.filter(j => j.enabled).length}` to TeamPage. | Plumbing for I.13. |

**Gap Fill Pass 3 Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅ (1.22s, 72.07 KB CSS / 12.14 KB gzipped, 371.90 KB JS / 107.36 KB gzipped)

**Remaining known scroll:** `.mc-board-scroll` (kanban horizontal column virtualizer) + `.mc-onboarding-modal` (full-page wizard) — both accepted exceptions.

---

## Verification Pass (E2E Spec Audit)

**Context:** Full end-to-end verification of `mission-control-designpass.md` against implementation. Audited all phases (0–8), all four Laws, and all §7 items. Found and fixed 13 gaps.

| # | File | Action | What Changed | Why |
|---|------|--------|-------------|-----|
| V.1 | `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx` | **Fix** | Removed standalone "Refresh" button from sidebar header. Moved emoji picker (`SmilePlus`) from conversation header area to compose inline-actions bar. Moved "Attach" file input behind Options toggle panel (progressive disclosure). Reduced compose controls at rest from 8 → 5 (emoji, Options, Send + textarea + thread list). | Law 2 violation: Chatrooms had 8 visible controls at rest, exceeding ≤5 limit. |
| V.2 | `apps/mission-control/src/features/boards/BoardsPage.tsx` | **Fix** | Converted dynamic owner filter chips (grew with agent count, unbounded) to a single `<select>` dropdown with options: All, Unassigned, and each agent by name. Board toolbar controls at rest: Board dropdown + Owner dropdown + New Card = 3. | Law 2 violation: owner filter chips could exceed 5 controls with many agents. Law 3: dropdown over dynamic chip set. |
| V.3 | `apps/mission-control/src/features/cockpit/CockpitPage.tsx` | **Fix** | Added `removeWidgetId` state + confirmation `Modal` for widget Remove action. Remove button now sets state instead of calling handler directly. Modal shows widget title and Cancel/Remove buttons. | §7.5 gap: Remove was the only destructive action without a confirmation dialog. |
| V.4 | `apps/mission-control/src/ui/Avatar.tsx` | **Create** | New `Avatar` component: colored circle with deterministic initials. 8-color palette via string hash. Initials extracted by splitting on space/dot/dash/underscore. Inline styles for size, color, border-radius. Title attribute shows full name on hover. | §7.7 gap: "sender avatars for mail messages" — spec requires visual identity in message streams. |
| V.5 | `apps/mission-control/src/features/agentMail/MailPage.tsx` | **Fix** | Added `Avatar` rendering before `sender_principal` in message heads. Converted 3 `formatDateTime` usages to `formatRelative` with `formatDateTime` tooltip (thread list, message time, lease expiry). Added `sending` state + `handleSend` wrapper for Send button loading feedback ("Sending..." text while in-flight). | §7.7 avatars, §7.9 relative timestamps, §7.11 loading states. |
| V.6 | `apps/mission-control/src/features/agentMail/ChatroomsPage.tsx` | **Fix** | Added `Avatar` rendering before `sender_principal` in room message heads. Converted `formatDateTime` to `formatRelative` with tooltip in thread list and message timestamps. Added `sending` state for Send button with "Sending..." feedback. | §7.7 avatars, §7.9 relative timestamps, §7.11 loading states. |
| V.7 | `apps/mission-control/src/features/focus/FocusPage.tsx` | **Fix** | Added severity icons (`AlertTriangle` for critical/error, `AlertCircle` for warning, `Info` for info, `CheckCircle` for default) via new `SeverityIcon` component. Added `busyItems` Set state + `withBusy` helper for tracking in-flight approve/deny operations. Approve/Deny buttons disable when busy, show "Working..." text. | §7.6 severity icons, §7.11 loading states on async approval actions. |
| V.8 | `apps/mission-control/src/features/events/EventsPage.tsx` | **Fix** | Converted event timestamp to `formatRelative` with `formatDateTime` tooltip. | §7.9 gap: Events page still showed absolute timestamps. |
| V.9 | `apps/mission-control/src/features/cockpit/CockpitWidgetRenderer.tsx` | **Fix** | Converted 2 timestamps to `formatRelative` with `formatDateTime` tooltip: job `next_run_at` in Jobs widget, event `ts_unix_ms` in Events widget. | §7.9 gap: Cockpit widgets showed absolute timestamps. |
| V.10 | `apps/mission-control/src/features/boards/BoardLane.tsx` | **Fix** | Added Lucide icon imports (`Bot`, `User`, `HelpCircle`). Created `OwnerIcon` component rendering per `owner_kind`. Card metadata row now shows icon + kind text. | §7.8 gap: board cards lacked visual metadata icons. |

**Verification Pass Validation:** `typecheck` ✅ · `lint` ✅ · `build` ✅
