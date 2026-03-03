# Mission Control — Theme/Shell Spot-Check Fix Spec

Date: 2026-03-03  
Owner: Codex (implementation), AppDex (approval)  
Scope:
- `apps/mission-control/src/styles.css`
- `apps/mission-control/src/app/AppShell.tsx`
- `apps/mission-control/src/ui/ThemeDropdown.tsx`
- `apps/mission-control/src/app/GuidedTourOverlay.tsx`
- (supporting) `apps/mission-control/src/app/useTheme.ts`

Source: Claude spot-check findings (baked-in).

## Goals
1. Remove theme CSS fragility that “works by accident of source order”.
2. Use a single, intentional font-loading strategy (avoid double-loading).
3. Remove sloppy/meaningless CSS classes in the DOM.
4. Make `ThemeDropdown` keyboard-accessible without lying via ARIA roles.
5. Keep Guided Tour bubble side-anchored to the highlighted target (right-first), not below it.

## Non-Goals
- No theme redesign (keep the current theme families + tokens).
- No “bundle fonts” project in this pass (optional P2 follow-up).

---

## Finding 1 — Auto light-mode selector fragility (pre-existing)

**Problem**
- `styles.css` uses `:root:not([data-theme$="-dark"])` inside `prefers-color-scheme: light`.
- That selector matches explicit light themes too (`phosphor-light`, `ember-light`, etc.).
- Today it “works” only because explicit light theme blocks appear later and win in cascade order.
- This is fragile: reordering blocks (or splitting CSS files) could break light theme families.

**Fix**
- Change the auto-detect selector to only apply when there is **no explicit theme**:
  - Replace `:root:not([data-theme$="-dark"])` with `:root:not([data-theme])`.

**Acceptance**
1. When `data-theme="phosphor-light"` (or any explicit `*-light`), the auto-detect light tokens do **not** apply.
2. When there is **no** `data-theme` attribute and the OS prefers light, the app still paints with a usable light baseline.

---

## Finding 2 — Dual Google Fonts loading strategy (pre-existing)

**Problem**
- `styles.css` has a static Google Fonts `@import` for Obsidian fonts.
- `useTheme.ts` also injects a `<link rel="stylesheet">` for theme family fonts.
- Net result:
  - Obsidian fonts always download even when not used.
  - This is unnecessary network work and increases CSP/offline risk.

**Fix (recommended) — Runtime injection only**
1. Remove the static `@import` line in `apps/mission-control/src/styles.css`.
2. Keep `useTheme.ts` as the single font loader, and apply theme as early as possible:
   - Change `useEffect` → `useLayoutEffect` for the `applyTheme(family, mode)` effect so the `data-theme` attribute and font link are applied before paint.

**Notes**
- Tauri CSP already allows this (`style-src` includes `https://fonts.googleapis.com`, `font-src` includes `https://fonts.gstatic.com`) via `apps/mission-control/src-tauri/tauri.conf.json`.

**Acceptance**
1. `styles.css` contains no Google Fonts `@import`.
2. On app load, the correct theme fonts still load (via the `mc-theme-fonts` link).
3. Switching theme families changes fonts without a full reload.

**Optional P2 (if offline becomes a requirement)**
- Bundle fonts in `apps/mission-control/src/assets/fonts/*` and remove Google Fonts from CSP entirely.

---

## Finding 3 — Trailing-hyphen connection class (pre-existing)

**Problem**
- When `wsState === "idle"`, `connectionTone` becomes `""`, producing a class like `mc-connection-dot-`.
- It’s harmless but noisy and unintentional.

**Fix**
- Only add the tone class when it’s non-empty:
  - e.g. `clsx("mc-connection-dot", connectionTone && \`mc-connection-dot-\${connectionTone}\`)`.

**Acceptance**
1. DOM never contains `mc-connection-dot-` (trailing hyphen).
2. Visual behavior remains unchanged for idle/neutral.

---

## Finding 4 — ARIA menu roles without keyboard contract (introduced)

**Problem**
- `ThemeDropdown` sets `role="menu"` and `role="menuitem"` on clickable `div`s.
- ARIA `menu/menuitem` implies arrow-key navigation + roving tabindex, which is not implemented.
- The row-level click target is not keyboard reachable; only the nested mode buttons are.

**Fix (recommended) — Remove menu roles + make selection keyboard-reachable**
1. Remove `role="menu"` and `role="menuitem"` usage.
2. Avoid invalid “button inside button” nesting:
   - Make each row a container `div`.
   - Add a dedicated **main** `<button>` for the row selection (family select / toggle mode).
   - Keep the Sun/Moon mode buttons as separate buttons in the row.
3. Add focus styling:
   - Add `:focus-visible` or `:focus-within` outline treatment on the row/main button so keyboard users can see focus.

**Acceptance**
1. Tabbing can reach the “select family” control for each theme row.
2. Enter/Space on the row main control triggers the same behavior as click.
3. No ARIA menu roles remain unless full arrow-key nav is implemented.

---

## Finding 5 — Guided Tour bubble position should be side-anchored (new UX requirement)

**Problem**
- Guided Tour bubble currently favors under/above placement relative to the highlighted target.
- Requested interaction is side-oriented so the bubble sits to the right of the highlighted control instead of under it.

**Fix**
1. Update `GuidedTourOverlay` bubble placement logic:
   - Prefer right-side placement (`target right edge + gap`).
   - If right side does not fit, fallback to left side.
   - Keep the bubble vertically centered against the highlighted target with viewport clamping.
2. Do not use under-target placement as primary behavior.

**Acceptance**
1. On standard desktop layouts, bubble appears to the right of the highlighted target.
2. If right side has no room, bubble appears to the left (still side-oriented).
3. Bubble remains fully visible via viewport clamping.

---

## Validations (must run after fixes)

Mission Control:
```bash
cd apps/mission-control && npm run typecheck && npm run lint && npm run build
```

Manual QA:
1. Toggle theme family and mode from:
   - Topbar theme dropdown
   - Settings modal theme picker
2. Verify keyboard access:
   - Tab into theme dropdown, select a family via keyboard, switch light/dark.
3. Verify system-light baseline:
   - With OS set to light and no stored theme keys, the initial paint is usable and does not override explicit light themes.
