import {
  expect,
  test,
  type APIRequestContext,
  type Page,
} from "./testHarness";
import {
  completeQuickstartLocalOnboarding,
  GATEWAY_URL,
  TEST_TOKEN,
} from "./onboardingFlow";

const AUTH_HEADERS = { Authorization: `Bearer ${TEST_TOKEN}` };

/**
 * P6 final polish · whole-building proof: the registry-driven guided tour
 * (elevator stops, launcher focus restore, honest hidden-target recovery),
 * the 390px mobile shell law (no clipped title/mark/badge/posture/action,
 * stacked Office feed with Needs You first, Reef/Chatter summaries before
 * disclosure), editable-field keyboard guards, reduced-motion tour, and
 * 200%-zoom plus hostile-long-copy resilience — with exact console/page/
 * request failure accounting throughout.
 */

interface FailureAccounting {
  browserErrors: string[];
}

function trackFailures(page: Page): FailureAccounting {
  const browserErrors: string[] = [];
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      browserErrors.push(
        `${message.text()} [${message.location().url ?? "no-url"}]`,
      );
    }
  });
  page.on("requestfailed", (req) => {
    browserErrors.push(
      `request failed: ${req.method()} ${req.url()} ${req.failure()?.errorText ?? "unknown"}`,
    );
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      browserErrors.push(`${response.status()} ${response.url()}`);
    }
  });
  return { browserErrors };
}

async function setOpsState(
  request: APIRequestContext,
  payload: Record<string, unknown>,
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/ops-state`, {
    headers: AUTH_HEADERS,
    data: payload,
  });
  expect(response.ok()).toBeTruthy();
}

/** The shell refetches operational facts on gateway events, not by polling. */
async function nudgeOpsRefetch(request: APIRequestContext): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/ws-event`, {
    headers: AUTH_HEADERS,
    data: {
      event_type: "job.updated",
      entity: "job",
      payload: { job_id: "job-heartbeat" },
    },
  });
  expect(response.ok()).toBeTruthy();
}

/**
 * The fixed, centered "Force Crash" control exists only in e2e mode and
 * floats over the real topbar. Hiding it is sanctioned: it is the one
 * test-only sentinel, never real product chrome.
 */
async function hideCrashSentinel(page: Page): Promise<void> {
  await page
    .getByRole("button", { name: "Force crash active tab" })
    .evaluateAll((buttons) => {
      for (const button of buttons) {
        (button as HTMLElement).style.visibility = "hidden";
      }
    });
}

async function captureEvidence(page: Page, path: string): Promise<void> {
  const sentinels = page.getByRole("button", {
    name: "Force crash active tab",
  });
  await sentinels.evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLElement).dataset.qaPreviousVisibility =
        (button as HTMLElement).style.visibility;
      (button as HTMLElement).style.visibility = "hidden";
    }
  });
  await page.screenshot({ path, fullPage: true });
  await sentinels.evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLElement).style.visibility =
        (button as HTMLElement).dataset.qaPreviousVisibility ?? "";
      delete (button as HTMLElement).dataset.qaPreviousVisibility;
    }
  });
}

async function dismissVisibleToasts(page: Page): Promise<void> {
  await page.locator(".mc-toast-dismiss").evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLButtonElement).click();
    }
  });
}

async function expectNoHorizontalDocumentOverflow(page: Page): Promise<void> {
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    return doc.scrollWidth - doc.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(0);
}

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Attached, visible, and nonzero — for decorative (pointer-events: none) boxes. */
async function expectVisibleBox(page: Page, selector: string): Promise<Box> {
  const locator = page.locator(selector).first();
  await expect(locator).toBeVisible();
  const box = await locator.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.width).toBeGreaterThan(0);
  expect(box!.height).toBeGreaterThan(0);
  return box!;
}

/** Attached, visible, nonzero, and center-point unobscured — never a zero-rect pass. */
async function expectVisibleNonZeroRect(
  page: Page,
  selector: string,
): Promise<Box> {
  const locator = page.locator(selector).first();
  await expect(locator).toBeVisible();
  const box = await locator.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.width).toBeGreaterThan(0);
  expect(box!.height).toBeGreaterThan(0);
  const centerIsUnobscured = await locator.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    const hit = document.elementFromPoint(
      rect.left + rect.width / 2,
      rect.top + rect.height / 2,
    );
    return hit !== null && (hit === element || element.contains(hit));
  });
  expect(centerIsUnobscured).toBe(true);
  return box!;
}

function expectContained(inner: Box, outer: Box): void {
  expect(inner.x).toBeGreaterThanOrEqual(outer.x - 0.5);
  expect(inner.y).toBeGreaterThanOrEqual(outer.y - 0.5);
  expect(inner.x + inner.width).toBeLessThanOrEqual(
    outer.x + outer.width + 0.5,
  );
  expect(inner.y + inner.height).toBeLessThanOrEqual(
    outer.y + outer.height + 0.5,
  );
}

function expectNoOverlap(a: Box, b: Box): void {
  const separated =
    a.x + a.width <= b.x ||
    b.x + b.width <= a.x ||
    a.y + a.height <= b.y ||
    b.y + b.height <= a.y;
  expect(separated).toBe(true);
}

async function expectWithinViewport(page: Page, box: Box): Promise<void> {
  const viewport = page.viewportSize();
  expect(viewport).not.toBeNull();
  expect(box.x).toBeGreaterThanOrEqual(-0.5);
  expect(box.y).toBeGreaterThanOrEqual(-0.5);
  expect(box.x + box.width).toBeLessThanOrEqual(viewport!.width + 0.5);
  expect(box.y + box.height).toBeLessThanOrEqual(viewport!.height + 0.5);
}

test("@core @p6-polish guided tour: elevator stops, launcher focus restore, hidden-target honesty, reduced motion", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const { browserErrors } = trackFailures(page);

  await completeQuickstartLocalOnboarding(page);
  await hideCrashSentinel(page);
  const tourBubble = page.locator(".mc-tour-bubble");
  const launcher = page.locator('[data-tour-id="topbar-tour"]');

  // Desktop: the first stop spotlights the Office floor in the rail and
  // lands its room by stable id.
  await launcher.click();
  await expect(page.locator(".mc-tour-progress-chip")).toHaveText("1/7");
  await expect(
    page.getByRole("heading", { name: "4F · The Office" }),
  ).toBeVisible();
  const highlight = await expectVisibleBox(page, ".mc-tour-highlight");
  // The scrim deliberately dims the whole page during the tour, so an
  // occlusion probe under it is meaningless; the box proof still matters.
  const officeFloor = await expectVisibleBox(
    page,
    '[data-tour-id="floor-office"]',
  );
  // The spotlight wraps the real floor section (6px halo).
  expectContained(officeFloor, {
    x: highlight.x - 1,
    y: highlight.y - 1,
    width: highlight.width + 2,
    height: highlight.height + 2,
  });
  await expect(page.locator('[data-tour-id="nav-assistant"]')).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.locator(".mc-tour-missing")).toHaveCount(0);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/tour-office-stop-desktop.png",
  );

  // Escape ends the tour and returns focus to the exact launcher.
  await page.keyboard.press("Escape");
  await expect(tourBubble).toBeHidden();
  await expect(launcher).toBeFocused();

  // Reduced motion: the reopened overlay carries no effective animation.
  await page.emulateMedia({ reducedMotion: "reduce" });
  await launcher.click();
  await expect(tourBubble).toBeVisible();
  const animatedDurations = await page.evaluate(() => {
    const parse = (value: string): number[] =>
      value.split(",").map((part) => {
        const trimmed = part.trim();
        const seconds = trimmed.endsWith("ms")
          ? Number.parseFloat(trimmed) / 1000
          : Number.parseFloat(trimmed);
        return Number.isFinite(seconds) ? seconds : 0;
      });
    const elements = [
      document.querySelector(".mc-tour-overlay"),
      document.querySelector(".mc-tour-scrim"),
      document.querySelector(".mc-tour-bubble"),
      document.querySelector(".mc-tour-highlight"),
    ].filter((element): element is Element => element !== null);
    return elements.flatMap((element) => {
      const style = window.getComputedStyle(element);
      return [
        ...parse(style.animationDuration),
        ...parse(style.transitionDuration),
      ];
    });
  });
  expect(animatedDurations.length).toBeGreaterThan(0);
  for (const duration of animatedDurations) {
    expect(duration).toBeLessThanOrEqual(0.00001);
  }
  await page.keyboard.press("Escape");
  await expect(tourBubble).toBeHidden();
  await page.emulateMedia({ reducedMotion: null });

  // Real-browser dialog proof: closed disclosures may not poison the
  // Settings focus ring, both directions wrap, Escape closes, and the exact
  // invoker receives focus again. The command palette follows the same law.
  const configInvoker = page.locator('button[data-tour-id="nav-config"]');
  await configInvoker.click();
  const settingsDialog = page.getByRole("dialog", { name: "Settings" });
  await expect(settingsDialog).toBeVisible();
  const settingsClose = settingsDialog.getByRole("button", {
    name: "Close settings",
  });
  const themeSummary = settingsDialog.locator("summary").last();
  await expect(settingsClose).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(themeSummary).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(settingsClose).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(settingsDialog).toBeHidden();
  await expect(configInvoker).toBeFocused();

  const commandInvoker = page.locator('[data-tour-id="topbar-command"]');
  await commandInvoker.click();
  const commandDialog = page.getByRole("dialog", { name: "Command palette" });
  await expect(commandDialog).toBeVisible();
  const commandInput = commandDialog.getByRole("textbox", {
    name: "Search commands",
  });
  const lastCommand = commandDialog.locator("[data-cmd-item]").last();
  await expect(commandInput).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(lastCommand).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(commandInput).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(commandDialog).toBeHidden();
  await expect(commandInvoker).toBeFocused();

  // 390px: the command-palette stop's anchor is hidden on the phone shell;
  // the tour says so honestly instead of spotlighting a zero rect.
  await page.setViewportSize({ width: 390, height: 844 });
  const bannerTour = page
    .locator(".mc-tab-help-banner")
    .getByRole("button", { name: "Tour" });
  await bannerTour.click();
  await expect(page.locator(".mc-tour-progress-chip")).toHaveText("1/7");
  for (let index = 0; index < 6; index += 1) {
    await tourBubble.getByRole("button", { name: "Next", exact: true }).click();
  }
  await expect(page.locator(".mc-tour-progress-chip")).toHaveText("7/7");
  await expect(
    page.getByRole("heading", { name: "Command palette" }),
  ).toBeVisible();
  await expect(page.locator(".mc-tour-missing")).toBeVisible();
  await expect(page.locator(".mc-tour-missing")).toContainText(
    "isn't visible right now",
  );
  await expect(page.locator(".mc-tour-highlight")).toHaveCount(0);
  const bubbleBox = await expectVisibleBox(page, ".mc-tour-bubble");
  await expectWithinViewport(page, bubbleBox);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/tour-mobile-hidden-target.png",
  );
  await tourBubble.getByRole("button", { name: "Finish", exact: true }).click();
  await expect(tourBubble).toBeHidden();
  // The tour walked away from the launching tab, so its banner unmounted and
  // exact restoration is impossible; focus must still land on a connected,
  // keyboard-reachable node rather than a detached one. (Exact same-invoker
  // restoration is proven on desktop above, where the launcher survives.)
  const restored = await page.evaluate(() => {
    const active = document.activeElement as HTMLElement | null;
    const style = active ? window.getComputedStyle(active) : null;
    return {
      tagName: active?.tagName ?? null,
      tourId: active?.dataset.tourId ?? null,
      connected: active?.isConnected ?? false,
      keyboardReachable:
        active !== null &&
        active !== document.body &&
        active !== document.documentElement &&
        active.tabIndex >= 0 &&
        active.getClientRects().length > 0 &&
        style?.display !== "none" &&
        style?.visibility !== "hidden",
    };
  });
  expect(restored.connected).toBe(true);
  expect(restored.keyboardReachable).toBe(true);
  expect(restored.tagName).toBe("BUTTON");
  expect(restored.tourId).toBe("nav-help-shortcut");

  await expectNoHorizontalDocumentOverflow(page);
  expect(browserErrors).toEqual([]);
});

test("@core @p6-polish 390px shell law: unclipped chrome, mark/badge separation, stacked Office feed, Window summaries", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const { browserErrors } = trackFailures(page);

  await page.setViewportSize({ width: 1280, height: 720 });
  await completeQuickstartLocalOnboarding(page);
  await hideCrashSentinel(page);
  await dismissVisibleToasts(page);

  // Desktop remains the fixed 6x4 canvas, and merely changing presentation
  // width must not rewrite the persisted Office arrangement.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("execass-office")).toBeVisible();
  const persistedDesktopLayout = await page.evaluate(() =>
    localStorage.getItem("mc-glass-config-v1"),
  );
  const desktopGrid = await page.locator(".mc-execass-buckets").evaluate(
    (element) => {
      const style = window.getComputedStyle(element);
      return {
        columns: style.gridTemplateColumns.split(/\s+/).filter(Boolean).length,
        rows: style.gridTemplateRows.split(/\s+/).filter(Boolean).length,
      };
    },
  );
  expect(desktopGrid).toEqual({ columns: 6, rows: 4 });
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/desktop-office-6x4.png",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(() => localStorage.getItem("mc-glass-config-v1")),
  ).toBe(persistedDesktopLayout);

  // Shell chrome: title, posture word, and the rail survive 390px without
  // clipping or occlusion.
  const topbar = await expectVisibleNonZeroRect(page, ".mc-topbar");
  const title = await expectVisibleNonZeroRect(page, ".mc-topbar-title");
  expectContained(title, topbar);
  await expect(page.locator(".mc-topbar-title")).toHaveText("Mission Control");
  const posture = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-posture-status"]',
  );
  expectContained(posture, topbar);
  expectNoOverlap(title, posture);
  const liveFeedAction = await expectVisibleNonZeroRect(
    page,
    '[data-testid="live-feed-toggle"]',
  );
  expectContained(liveFeedAction, topbar);
  expectNoOverlap(title, liveFeedAction);
  await expectWithinViewport(page, title);
  await expectWithinViewport(page, posture);
  await expectWithinViewport(page, liveFeedAction);
  const visibleTopbarBadges = page.locator(
    ".mc-topbar-right .mc-live-feed-toggle-badge, .mc-topbar-right .mc-notification-badge",
  );
  for (const badge of await visibleTopbarBadges.all()) {
    if (!(await badge.isVisible())) continue;
    const box = await badge.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.width).toBeGreaterThan(0);
    expect(box!.height).toBeGreaterThan(0);
    expectContained(box!, topbar);
    expectNoOverlap(title, box!);
    await expectWithinViewport(page, box!);
  }

  // Rail room marks replace labels; the seeded approvals badge may not
  // cover the Breakers room mark.
  const railBox = await expectVisibleNonZeroRect(page, ".mc-nav-rail");
  const focusItem = page.locator('[data-tour-id="nav-focus"]');
  await expect(focusItem).toBeVisible();
  // The elevator scrolls on a phone; a Basement room must be scrolled into
  // its visible range before its geometry means anything.
  await focusItem.scrollIntoViewIfNeeded();
  const focusMark = await expectVisibleNonZeroRect(
    page,
    '[data-tour-id="nav-focus"] .mc-nav-room-mark',
  );
  // The badge is pointer-events: none by design, so it can never win an
  // elementFromPoint probe; its box truth is what matters.
  const focusBadge = await expectVisibleBox(
    page,
    '[data-tour-id="nav-focus"] .mc-nav-badge',
  );
  expectNoOverlap(focusMark, focusBadge);
  expectContained(focusMark, railBox);
  await expectWithinViewport(page, focusMark);
  await expectWithinViewport(page, focusBadge);

  // The Office is a stacked feed with Needs You first.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("execass-office")).toBeVisible();
  const blocks = page.locator('[data-testid^="office-block-"]');
  const blockCount = await blocks.count();
  expect(blockCount).toBeGreaterThan(1);
  // Read every rectangle in one layout snapshot. Scrolling between
  // viewport-relative measurements would make cross-block ordering vacuous.
  const blockBoxes = await blocks.evaluateAll((elements) =>
    elements.map((element) => {
      const rect = element.getBoundingClientRect();
      return {
        id: element.getAttribute("data-testid") ?? "",
        box: {
          x: rect.x,
          y: rect.y,
          width: rect.width,
          height: rect.height,
        },
      };
    }),
  );
  for (const entry of blockBoxes) {
    expect(entry.box.width).toBeGreaterThan(0);
    expect(entry.box.height).toBeGreaterThan(0);
  }
  const needsYou = blockBoxes.find(
    (entry) => entry.id === "office-block-needs-you",
  );
  expect(needsYou).toBeTruthy();
  for (const entry of blockBoxes) {
    if (entry.id === "office-block-needs-you") continue;
    expect(needsYou!.box.y).toBeLessThan(entry.box.y);
    expect(Math.abs(entry.box.x - needsYou!.box.x)).toBeLessThan(1);
  }
  await page
    .getByTestId("office-block-needs-you")
    .evaluate((element) => element.scrollIntoView({ block: "start" }));
  await expectNoHorizontalDocumentOverflow(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/mobile-office-stacked-feed.png",
  );

  // The Window summarizes Reef and Chatter before disclosure, and a
  // hostile long owner note stays contained after disclosure.
  await page.locator('[data-tour-id="nav-window"]').click();
  const reefSummary = page.locator(".mc-reef-collapse > summary");
  await expect(reefSummary).toBeVisible();
  await expect(reefSummary).toContainText("on the floor");
  const chatterCollapse = page.locator(".mc-chatter-collapse");
  await expect(chatterCollapse).toBeVisible();
  const chatterSummary = page.locator(".mc-chatter-collapse > summary");
  await expect(chatterSummary).toContainText("room");
  await expect(page.locator(".mc-chatter-compose input")).toBeHidden();

  await chatterSummary.click();
  const composeInput = page.locator(".mc-chatter-compose input");
  await expect(composeInput).toBeVisible();
  const hostileNote =
    "Hostile long copy — " +
    "unbroken-hyphenated-segment-".repeat(18) +
    " end.";
  await composeInput.fill(hostileNote.slice(0, 900));
  await page
    .locator(".mc-chatter-compose")
    .getByRole("button", { name: "Send" })
    .click();
  await expect(
    page.locator(".mc-chatter-messages").getByText("Hostile long copy", {
      exact: false,
    }),
  ).toBeVisible();
  const chatterPanel = page.locator(".mc-chatter-panel");
  await chatterPanel.scrollIntoViewIfNeeded();
  const panelBox = await expectVisibleNonZeroRect(page, ".mc-chatter-panel");
  const hostileArticle = page
    .locator(".mc-chatter-messages article", { hasText: "Hostile long copy" })
    .last();
  const noteBox = await hostileArticle.boundingBox();
  expect(noteBox).not.toBeNull();
  expect(noteBox!.width).toBeGreaterThan(0);
  expect(noteBox!.x).toBeGreaterThanOrEqual(panelBox.x - 0.5);
  expect(noteBox!.x + noteBox!.width).toBeLessThanOrEqual(
    panelBox.x + panelBox.width + 0.5,
  );
  expect(noteBox!.y).toBeLessThan(panelBox.y + panelBox.height);
  expect(noteBox!.y + noteBox!.height).toBeGreaterThan(panelBox.y);
  await expectWithinViewport(page, panelBox);
  const visibleArticlePointIsUnobscured = await hostileArticle.evaluate(
    (element) => {
      const rect = element.getBoundingClientRect();
      const panel = element.closest(".mc-chatter-panel");
      if (!panel) return false;
      const panelRect = panel.getBoundingClientRect();
      const x = Math.max(
        rect.left + 1,
        Math.min(rect.right - 1, panelRect.left + panelRect.width / 2),
      );
      const y = Math.max(
        rect.top + 1,
        Math.min(rect.bottom - 1, panelRect.top + panelRect.height / 2),
      );
      const hit = document.elementFromPoint(x, y);
      return hit !== null && (hit === element || element.contains(hit));
    },
  );
  expect(visibleArticlePointIsUnobscured).toBe(true);
  await expectNoHorizontalDocumentOverflow(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/mobile-window-summaries.png",
  );

  expect(browserErrors).toEqual([]);
});

test("@core @p6-polish zoom and keyboard truth: 200% layout, editable-field guard, long-copy incident band", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  const { browserErrors } = trackFailures(page);

  // 720x450 is the layout viewport a 1440x900 screen presents at 200% zoom.
  await page.setViewportSize({ width: 720, height: 450 });
  await completeQuickstartLocalOnboarding(page);
  await hideCrashSentinel(page);
  await dismissVisibleToasts(page);
  await expectNoHorizontalDocumentOverflow(page);
  const zoomTopbar = await expectVisibleNonZeroRect(page, ".mc-topbar");
  const zoomTitle = await expectVisibleNonZeroRect(page, ".mc-topbar-title");
  const zoomPosture = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-posture-status"]',
  );
  expectContained(zoomTitle, zoomTopbar);
  expectContained(zoomPosture, zoomTopbar);
  expectNoOverlap(zoomTitle, zoomPosture);

  // Elevator shortcuts must ignore editable fields.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("execass-office")).toBeVisible();
  const askInput = page.locator(".mc-execass-ask input, .mc-execass-ask textarea").first();
  await askInput.click();
  await askInput.pressSequentially("24b");
  await expect(page.locator('[data-tour-id="nav-assistant"]')).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(askInput).toHaveValue("24b");
  // Outside an editable field the same key rides the elevator.
  await page.locator(".mc-topbar-title").click();
  await page.keyboard.press("2");
  await expect(page.locator('[data-tour-id="nav-boards"]')).toHaveAttribute(
    "aria-current",
    "page",
  );

  // A hostile long breaker id may not break the incident band at 200% zoom
  // or at 390px; the walk action stays usable and walks to the exact room.
  await page.locator('button[data-tour-id="nav-config"]').click();
  await page.getByText("Choose what pages show").click();
  const autoToggle = page.getByLabel("Auto-switch to incident mode");
  await autoToggle.check();
  await page.getByRole("button", { name: "Close settings" }).click();
  await expect(page.locator(".mc-settings-modal")).toHaveCount(0);

  const longTarget =
    "provider-with-a-hostile-extremely-long-identifier-" +
    "x".repeat(80);
  await setOpsState(request, {
    circuit_breakers: [
      {
        scope: "provider",
        target_id: longTarget,
        state: "open",
        consecutive_failures: 5,
        cooldown_until: Date.now() + 5 * 60_000,
        last_error_code: "timeout_p6_polish",
        updated_at: Date.now() - 30_000,
      },
    ],
  });
  await nudgeOpsRefetch(request);
  const band = page.locator('[data-testid="incident-band"]');
  await expect(band).toBeVisible({ timeout: 30_000 });
  const zoomTitleWithIncident = await expectVisibleNonZeroRect(
    page,
    ".mc-topbar-title",
  );
  const zoomBadges = page.locator(
    ".mc-topbar-right .mc-live-feed-toggle-badge, .mc-topbar-right .mc-notification-badge",
  );
  for (const badge of await zoomBadges.all()) {
    if (!(await badge.isVisible())) continue;
    const box = await badge.boundingBox();
    expect(box).not.toBeNull();
    expectContained(box!, await expectVisibleNonZeroRect(page, ".mc-topbar"));
    expectNoOverlap(zoomTitleWithIncident, box!);
    await expectWithinViewport(page, box!);
  }
  const bandBox = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-band"]',
  );
  await expectWithinViewport(page, bandBox);
  await expectNoHorizontalDocumentOverflow(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/zoom-200-long-copy-band.png",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await expect(band).toBeVisible();
  const narrowIncidentTopbar = await expectVisibleNonZeroRect(
    page,
    ".mc-topbar",
  );
  const narrowIncidentTitle = await expectVisibleNonZeroRect(
    page,
    ".mc-topbar-title",
  );
  const narrowIncidentPosture = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-posture-status"]',
  );
  expectContained(narrowIncidentTitle, narrowIncidentTopbar);
  expectContained(narrowIncidentPosture, narrowIncidentTopbar);
  expectNoOverlap(narrowIncidentTitle, narrowIncidentPosture);
  await expectWithinViewport(page, narrowIncidentTitle);
  await expectWithinViewport(page, narrowIncidentPosture);
  const bandBoxNarrow = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-band"]',
  );
  await expectWithinViewport(page, bandBoxNarrow);
  const walk = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-band-walk"]',
  );
  await expectWithinViewport(page, walk);
  await expectNoHorizontalDocumentOverflow(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/mobile-long-copy-band.png",
  );
  await page.locator('[data-testid="incident-band-walk"]').click();
  await expect(page.locator('[data-tour-id="nav-focus"]')).toHaveAttribute(
    "aria-current",
    "page",
  );

  // Authoritative recovery clears the band without a click.
  await setOpsState(request, { circuit_breakers: [] });
  await nudgeOpsRefetch(request);
  await expect(band).toBeHidden({ timeout: 30_000 });

  expect(browserErrors).toEqual([]);
});

test("@core @p6-polish resilience truth: bounded loading, partial error, retry, and exact failure accounting", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const unexpectedErrors: string[] = [];
  let injectingCapabilitiesFailure = false;
  let injected = false;
  let injectedRequests = 0;
  let injectedResponses = 0;
  let injectedConsoleErrors = 0;
  let releaseFailure!: () => void;
  let markFailureRequested!: () => void;
  const failureRequested = new Promise<void>((resolve) => {
    markFailureRequested = resolve;
  });
  const failureReleased = new Promise<void>((resolve) => {
    releaseFailure = resolve;
  });

  page.on("pageerror", (error) => unexpectedErrors.push(error.message));
  page.on("requestfailed", (request) => {
    unexpectedErrors.push(
      `request failed: ${request.method()} ${request.url()} ${request.failure()?.errorText ?? "unknown"}`,
    );
  });
  page.on("request", (request) => {
    if (
      injectingCapabilitiesFailure &&
      request.method() === "GET" &&
      /\/api\/v1\/providers\/capabilities(\?|$)/.test(request.url())
    ) {
      injectedRequests += 1;
    }
  });
  page.on("response", (response) => {
    if (response.status() < 400) return;
    if (
      injectingCapabilitiesFailure &&
      response.status() === 503 &&
      response.request().method() === "GET" &&
      /\/api\/v1\/providers\/capabilities(\?|$)/.test(response.url())
    ) {
      injectedResponses += 1;
      return;
    }
    unexpectedErrors.push(`${response.status()} ${response.url()}`);
  });
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    if (
      injectingCapabilitiesFailure &&
      message.text().includes("503") &&
      /providers\/capabilities/.test(
        `${message.location().url} ${message.text()}`,
      )
    ) {
      injectedConsoleErrors += 1;
      return;
    }
    unexpectedErrors.push(message.text());
  });
  await page.route("**/api/v1/providers/capabilities*", async (route) => {
    if (!injectingCapabilitiesFailure || injected) {
      await route.continue();
      return;
    }
    injected = true;
    markFailureRequested();
    await failureReleased;
    await route.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({ error: "capability catalog temporarily down" }),
    });
  });

  await completeQuickstartLocalOnboarding(page);
  await hideCrashSentinel(page);
  await dismissVisibleToasts(page);
  await page.locator('button[title="BF · Models & Providers"]').click();
  const modelsSurface = page.locator(".mc-models-surface");
  await expect(modelsSurface).toBeVisible();
  const quickGuideDismiss = page
    .locator(".mc-tab-help-banner")
    .getByRole("button", { name: "Hide quick guides" });
  if (await quickGuideDismiss.isVisible()) {
    await quickGuideDismiss.click();
  }
  await expect(modelsSurface.getByTestId("models-provider-ollama")).toContainText(
    "streaming",
  );
  // The successful baseline includes an explicit loaded-empty profile fact,
  // not a green/healthy invention.
  await expect(modelsSurface).toContainText(
    "No configured profiles for this provider.",
  );

  injectingCapabilitiesFailure = true;
  await modelsSurface
    .getByRole("button", { name: "Refresh", exact: true })
    .click();
  await failureRequested;
  await expect(modelsSurface).toContainText(
    "Loading provider capability facts…",
  );
  releaseFailure();
  await expect(modelsSurface).toContainText(
    "Provider capability facts could not be loaded.",
  );
  // Capability failure is partial: configured/assigned provider truth stays
  // present and is labeled unavailable instead of disappearing.
  const partialProvider = modelsSurface.getByTestId("models-provider-ollama");
  await expect(partialProvider).toBeVisible();
  await expect(partialProvider).toContainText("capability facts unavailable");
  await expect
    .poll(() => ({
      console: injectedConsoleErrors,
      requests: injectedRequests,
      responses: injectedResponses,
    }))
    .toEqual({ console: 1, requests: 1, responses: 1 });

  await page.setViewportSize({ width: 390, height: 844 });
  await partialProvider.scrollIntoViewIfNeeded();
  const surfaceBox = await expectVisibleBox(page, ".mc-models-surface");
  const providerBox = await expectVisibleBox(
    page,
    '[data-testid="models-provider-ollama"]',
  );
  expect(providerBox.x).toBeGreaterThanOrEqual(surfaceBox.x - 0.5);
  expect(providerBox.x + providerBox.width).toBeLessThanOrEqual(
    surfaceBox.x + surfaceBox.width + 0.5,
  );
  await expectNoHorizontalDocumentOverflow(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-polish/resilience-models-partial-error-390.png",
  );

  injectingCapabilitiesFailure = false;
  await modelsSurface
    .getByRole("button", { name: "Refresh", exact: true })
    .click();
  await expect(modelsSurface).not.toContainText(
    "Provider capability facts could not be loaded.",
  );
  await expect(partialProvider).toContainText("streaming");
  expect(unexpectedErrors).toEqual([]);
});
