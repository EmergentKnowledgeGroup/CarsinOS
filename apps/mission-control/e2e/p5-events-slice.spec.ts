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

async function emitWsEvent(
  request: APIRequestContext,
  payload: {
    event_type?: string;
    entity?: string;
    payload?: Record<string, unknown>;
  },
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/ws-event`, {
    headers: { Authorization: `Bearer ${TEST_TOKEN}` },
    data: payload,
  });
  expect(response.ok()).toBeTruthy();
}

async function emitWsBurst(
  request: APIRequestContext,
  payload: {
    count: number;
    event_type?: string;
    entity?: string;
    payload?: Record<string, unknown>;
  },
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/ws-burst`, {
    headers: { Authorization: `Bearer ${TEST_TOKEN}` },
    data: payload,
  });
  expect(response.ok()).toBeTruthy();
}

async function dismissVisibleToasts(page: Page): Promise<void> {
  await page.locator(".mc-toast-dismiss").evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLButtonElement).click();
    }
  });
}

/**
 * P5 Basement · Event stream room slice: the existing realtime feed surface
 * is rehomed behind its stable room id with zero pipeline changes. Real
 * websocket events drive domain filters, heartbeat visibility, recursive
 * JSON redaction, twelve-item pagination, and the 400-event retention cap;
 * the registry-backed pin performs no feed mutation; and the Office door
 * walks back to the exact room through refusal/restore, reload, and 390px
 * geometry with console-error and requestfailed capture.
 */

test("@core @p5-events event stream room identity, live feed parity, pin-to-office, and the office shortcut hold at desktop and 390px", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  const browserErrors: string[] = [];
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      browserErrors.push(message.text());
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

  await completeQuickstartLocalOnboarding(page);
  await dismissVisibleToasts(page);

  // The Event stream room lights exactly one lamp on its dedicated route.
  const activeRooms = page.locator(".mc-nav-item-active");
  await expect(page.locator('button[title="BF · Event stream"]')).toHaveCount(
    1,
  );
  await page.locator('button[title="BF · Event stream"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Event stream");
  await expect(
    page.getByRole("heading", { name: "Realtime Event Stream" }),
  ).toBeVisible();

  // Real websocket events, one per domain, plus a heartbeat and a payload
  // full of secrets for the redaction proof.
  const wsDot = page.locator(".mc-connection-dot").first();
  await expect
    .poll(async () => wsDot.getAttribute("title"), { timeout: 20_000 })
    .toBe("ws: connected");
  await emitWsEvent(request, {
    event_type: "board.card.created",
    entity: "board",
    payload: { action: "created", title: "Ship the basement" },
  });
  await emitWsEvent(request, {
    event_type: "job.updated",
    entity: "job",
    payload: { job_id: "job-parity-1" },
  });
  await emitWsEvent(request, {
    event_type: "approval.resolved",
    entity: "approval",
    payload: { decision: "approved" },
  });
  await emitWsEvent(request, {
    event_type: "channel.message",
    // The entity renders on the row (channel payload summaries do not), so
    // it doubles as the marker for the redaction proof.
    entity: "channel-secret-e2e",
    payload: {
      summary: "secret-bearing channel event",
      api_key: "sk-live-e2e-secret",
      nested: { client_secret: "nested-e2e-secret" },
      safe: "public-e2e-fact",
    },
  });
  await emitWsEvent(request, {
    event_type: "agent_mail.delivered",
    entity: "mail",
    payload: { summary: "mail parity event" },
  });
  await emitWsEvent(request, {
    event_type: "heartbeat.gateway",
    entity: "system",
    payload: { summary: "heartbeat parity event" },
  });

  const eventsSurface = page
    .locator("article.mc-surface")
    .filter({
      has: page.getByRole("heading", { name: "Realtime Event Stream" }),
    });
  const eventRows = eventsSurface.locator(".mc-event-item");
  await expect(
    eventRows.filter({ hasText: "board.card.created" }),
  ).toHaveCount(1);
  await expect(
    eventRows.filter({ hasText: "board.card.created" }),
  ).toContainText("Card created: Ship the basement");

  // Heartbeats stay hidden until the operator asks, through the real seam.
  await expect(
    eventRows.filter({ hasText: "heartbeat.gateway" }),
  ).toHaveCount(0);
  const heartbeats = page.getByLabel("Show heartbeats");
  await heartbeats.check();
  await expect(
    eventRows.filter({ hasText: "heartbeat.gateway" }),
  ).toHaveCount(1);
  await heartbeats.uncheck();
  await expect(
    eventRows.filter({ hasText: "heartbeat.gateway" }),
  ).toHaveCount(0);

  // Domain chips are live filters with pressed state.
  const chips = page.locator(".mc-event-filters .mc-filter-chip");
  await expect(chips).toHaveText([
    "All",
    "Board",
    "Job",
    "Approval",
    "Channel",
    "Mail",
  ]);
  // The feed is live and other surfaces emit their own events, so domain
  // filtering is proven by marker presence plus whole-page domain purity —
  // never by absolute counts.
  await chips.filter({ hasText: "Board" }).click();
  await expect(
    chips.filter({ hasText: "Board" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    eventRows.filter({ hasText: "Ship the basement" }),
  ).toHaveCount(1);
  await expect(
    eventRows.filter({ hasText: "agent_mail.delivered" }),
  ).toHaveCount(0);
  expect(
    (await eventRows.evaluateAll((rows) => rows.map((row) => row.className))).filter(
      (className) => !className.includes("mc-event-domain-accent"),
    ),
  ).toEqual([]);
  await chips.filter({ hasText: "Mail" }).click();
  await expect(
    eventRows.filter({ hasText: "agent_mail.delivered" }),
  ).toHaveCount(1);
  await expect(
    eventRows.filter({ hasText: "board.card.created" }),
  ).toHaveCount(0);
  expect(
    (await eventRows.evaluateAll((rows) => rows.map((row) => row.className))).filter(
      (className) => !className.includes("mc-event-domain-ok"),
    ),
  ).toEqual([]);

  // Redaction end-to-end: the wire carried real secrets; the expanded JSON
  // may not.
  await chips.filter({ hasText: "Channel" }).click();
  const secretRow = eventRows.filter({
    hasText: "channel-secret-e2e",
  });
  await expect(secretRow).toHaveCount(1);
  await secretRow.locator(".mc-event-expand").click();
  const payloadPre = secretRow.locator(".mc-event-payload");
  await expect(payloadPre).toBeVisible();
  await expect(payloadPre).toContainText("[REDACTED]");
  await expect(eventsSurface).toContainText("public-e2e-fact");
  await expect(payloadPre).not.toContainText("sk-live-e2e-secret");
  await expect(payloadPre).not.toContainText("nested-e2e-secret");
  const renderedEventSurface = await eventsSurface.evaluate(
    (surface) => surface.outerHTML,
  );
  expect(renderedEventSurface).not.toContain("sk-live-e2e-secret");
  expect(renderedEventSurface).not.toContain("nested-e2e-secret");
  await secretRow.locator(".mc-event-expand").click();
  await expect(secretRow.locator(".mc-event-payload")).toHaveCount(0);

  // A full default canvas refuses the pin and leaves the exact persisted
  // config untouched.
  const pin = page.getByRole("button", { name: "Pin Event stream to Office" });
  await expect(pin).toBeVisible();
  const fullCanvasConfig = await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = [
      { id: "needs-you", size: "l", visible: true },
      { id: "in-motion", size: "m", visible: true },
      { id: "done", size: "m", visible: true },
      { id: "next", size: "s", visible: false },
      { id: "boards", size: "s", visible: true },
      { id: "calendar", size: "s", visible: true },
      { id: "strategy", size: "s", visible: true },
      { id: "staff", size: "s", visible: true },
    ];
    const serialized = JSON.stringify(config);
    localStorage.setItem(key, serialized);
    window.dispatchEvent(new Event("mc-glass-config-changed"));
    return serialized;
  });
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note.is-error")).toHaveText(
    "Pinning this would exceed the six-column, four-row Office canvas.",
  );
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-events-slice/events-full-canvas-refusal.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(() => localStorage.getItem("mc-glass-config-v1")),
  ).toBe(fullCanvasConfig);
  expect(
    await page.evaluate(() => {
      const config = JSON.parse(
        localStorage.getItem("mc-glass-config-v1") ?? "{}",
      );
      return config.layout?.some(
        (placement: { id?: string }) => placement.id === "events",
      );
    }),
  ).toBe(false);

  // Free one medium block; the pin succeeds without touching feed state —
  // the operator's filter and open JSON row stay exactly as left.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "in-motion"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator(".mc-pin-to-office-note")).toHaveCount(0);
  await chips.filter({ hasText: "Job" }).click();
  const parityJobRow = eventRows.filter({ hasText: "job-parity-1" });
  await expect(parityJobRow).toHaveCount(1);
  await parityJobRow.locator(".mc-event-expand").click();
  await expect(parityJobRow.locator(".mc-event-payload")).toBeVisible();
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "On the Office canvas.",
  );
  await expect(
    chips.filter({ hasText: "Job" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(parityJobRow.locator(".mc-event-payload")).toBeVisible();

  // A repeat pin is byte-identical config plus the honest note.
  const pinnedConfig = await page.evaluate(() =>
    localStorage.getItem("mc-glass-config-v1"),
  );
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "Already on the Office canvas.",
  );
  expect(
    await page.evaluate(() => localStorage.getItem("mc-glass-config-v1")),
  ).toBe(pinnedConfig);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-events-slice/events-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut names its floor and consumes config events live.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-events");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await shortcut.scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "../../runtime/qa/p5-events-slice/office-shortcut-block.png",
    fullPage: true,
  });
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "events"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-events")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "events"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-events")).toBeVisible();

  // The door opens the exact room by stable id.
  await shortcut.getByRole("button", { name: "Open Event stream" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Event stream");
  await expect(
    page.getByRole("heading", { name: "Realtime Event Stream" }),
  ).toBeVisible();

  // The pin is config: it survives a full reload and still opens the exact
  // room afterwards.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await dismissVisibleToasts(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-events");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open Event stream" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Event stream");
  await expect(
    page.getByRole("heading", { name: "Realtime Event Stream" }),
  ).toBeVisible();

  // Hiding the Basement floor removes the lamp; the persisted door refuses
  // honestly and clears the moment the floor is restored.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.floorOverrides = {
      ...config.floorOverrides,
      basement: { hidden: true },
    };
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="BF · Event stream"]')).toHaveCount(
    0,
  );
  await page
    .getByTestId("office-block-events")
    .getByRole("button", { name: "Open Event stream" })
    .click();
  await expect(
    page.getByTestId("office-block-events").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await page.getByTestId("office-block-events").evaluate((block) => {
    block.scrollIntoView({ block: "center", inline: "nearest" });
  });
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-events-slice/events-disabled-door.png",
    fullPage: true,
  });
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.basement;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="BF · Event stream"]')).toHaveCount(
    1,
  );
  await expect(
    page.getByTestId("office-block-events").getByRole("status"),
  ).toHaveCount(0);
  await page.getByTestId("office-block-events").evaluate((block) => {
    block.scrollIntoView({ block: "center", inline: "nearest" });
  });
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-events-slice/events-restored-door.png",
    fullPage: true,
  });
  await page
    .getByTestId("office-block-events")
    .getByRole("button", { name: "Open Event stream" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Event stream");
  await expect(
    page.getByRole("heading", { name: "Realtime Event Stream" }),
  ).toBeVisible();

  // Pagination boundaries on a real thirty-event burst, then the 400-event
  // retention cap after five hundred more.
  await expect
    .poll(async () => wsDot.getAttribute("title"), { timeout: 20_000 })
    .toBe("ws: connected");
  // Start this proof lane from an empty documented recovery fixture. The
  // quickstart profile intentionally has the live-feed drawer disabled, so
  // test isolation clears only its persisted recovery key and then reconnects
  // through the real websocket pipeline.
  await page.evaluate(() => {
    localStorage.removeItem("mc-live-feed-recovery-v1");
  });
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await dismissVisibleToasts(page);
  await page.locator('button[title="BF · Event stream"]').click();
  await expect
    .poll(async () => wsDot.getAttribute("title"), { timeout: 20_000 })
    .toBe("ws: connected");
  await emitWsBurst(request, {
    count: 30,
    event_type: "agent_mail.delivered",
    entity: "mail-pagination-burst",
    payload: { summary: "pagination-burst" },
  });
  const freshChips = eventsSurface.locator(".mc-event-filters .mc-filter-chip");
  await freshChips.filter({ hasText: "Mail" }).click();
  const paginationInfo = eventsSurface.locator(".mc-pagination-info");
  await expect(paginationInfo).toHaveText("1 / 3");
  await expect(eventRows).toHaveCount(12);
  await expect(
    eventRows.filter({ hasText: "mail-pagination-burst" }),
  ).toHaveCount(12);
  await eventsSurface.getByRole("button", { name: "Next" }).click();
  await expect(paginationInfo).toHaveText("2 / 3");
  await expect(eventRows).toHaveCount(12);
  await expect(
    eventRows.filter({ hasText: "mail-pagination-burst" }),
  ).toHaveCount(12);
  await eventsSurface.getByRole("button", { name: "Next" }).click();
  await expect(paginationInfo).toHaveText("3 / 3");
  await expect(eventRows).toHaveCount(6);
  await expect(
    eventRows.filter({ hasText: "mail-pagination-burst" }),
  ).toHaveCount(6);
  await expect(
    eventsSurface.getByRole("button", { name: "Next" }),
  ).toBeDisabled();
  // Clearing the filter resets to page one; the live feed's total page
  // count is not asserted because other domains keep emitting.
  await freshChips.filter({ hasText: "All" }).click();
  await expect(paginationInfo).toHaveText(/^1 \/ \d+$/);

  // The cap is over the raw retained buffer. Expose heartbeats so an
  // interleaved heartbeat cannot make a correct 400-item buffer read as 399.
  await heartbeats.check();
  await expect(heartbeats).toBeChecked();
  const eventsSubtitle = page
    .locator("article.mc-surface")
    .filter({ has: page.getByRole("heading", { name: "Realtime Event Stream" }) })
    .locator("header p")
    .first();
  await emitWsBurst(request, {
    count: 500,
    event_type: "job.updated",
    entity: "job",
    payload: { job_id: "retention-job" },
  });
  await expect
    .poll(async () => (await eventsSubtitle.textContent())?.trim() ?? "", {
      timeout: 20_000,
    })
    .toBe("400 events");

  // Narrow width: nonzero visible ES room mark, inner containment for
  // chips, checkbox, rows, JSON, and pagination — a document-level
  // no-overflow check alone is insufficient.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('button[title="BF · Event stream"]').click();
  const mobileEventsRoom = page.locator('button[title="BF · Event stream"]');
  await expect(mobileEventsRoom).toHaveClass(/mc-nav-item-active/);
  const mark = mobileEventsRoom.locator(".mc-nav-room-mark");
  await expect(mark).toHaveText("ES");
  await expect(mark).toBeVisible();
  expect(
    await mark.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth &&
        rect.top >= 0 &&
        rect.bottom <= window.innerHeight
      );
    }),
  ).toBe(true);

  await eventRows.first().locator(".mc-event-expand").click();
  const geometry = await page.evaluate(() => {
    const contains = (
      outer: DOMRect,
      inner: DOMRect,
      slack = 1,
    ): boolean =>
      inner.width > 0 &&
      inner.height > 0 &&
      inner.left >= outer.left - slack &&
      inner.right <= outer.right + slack &&
      inner.top >= outer.top - slack &&
      inner.bottom <= outer.bottom + slack;
    // Vertical scrolling and the surface header's designed bleed are not
    // defects; the chop class of bug is horizontal. Wide checks require a
    // nonzero rect fully inside the container's horizontal span.
    const containsX = (
      outer: DOMRect,
      inner: DOMRect,
      slack = 1,
    ): boolean =>
      inner.width > 0 &&
      inner.height > 0 &&
      inner.left >= outer.left - slack &&
      inner.right <= outer.right + slack;
    const surface = Array.from(
      document.querySelectorAll("article.mc-surface"),
    ).find((candidate) =>
      Array.from(candidate.querySelectorAll("h2")).some(
        (heading) => heading.textContent === "Realtime Event Stream",
      ),
    );
    if (!surface) return { ok: false, reason: "no surface" };
    const surfaceRect = surface.getBoundingClientRect();
    const filters = surface.querySelector(".mc-event-filters");
    if (!filters) return { ok: false, reason: "no filters" };
    const filtersRect = filters.getBoundingClientRect();
    const chipResults = Array.from(
      filters.querySelectorAll(".mc-filter-chip"),
    ).map((chip) => contains(filtersRect, chip.getBoundingClientRect()));
    const checkbox = surface.querySelector(".mc-checkbox");
    const checkboxRect = checkbox?.getBoundingClientRect() ?? null;
    const checkboxOk = checkboxRect
      ? containsX(surfaceRect, checkboxRect)
      : false;
    const rowResults = Array.from(
      surface.querySelectorAll(".mc-event-item"),
    ).map((row) => containsX(surfaceRect, row.getBoundingClientRect()));
    const firstRow = surface.querySelector(".mc-event-item");
    const payload = firstRow?.querySelector(".mc-event-payload");
    const payloadOk =
      firstRow && payload
        ? contains(
            firstRow.getBoundingClientRect(),
            payload.getBoundingClientRect(),
          )
        : false;
    const expandButtons = Array.from(
      surface.querySelectorAll(".mc-event-expand"),
    ).map((button) =>
      containsX(surfaceRect, button.getBoundingClientRect()),
    );
    const paginationNav = surface.querySelector(".mc-pagination");
    const paginationOk = paginationNav
      ? Array.from(paginationNav.querySelectorAll("button, span")).every(
          (control) =>
            contains(
              paginationNav.getBoundingClientRect(),
              control.getBoundingClientRect(),
            ),
        )
      : false;
    const toDebug = (rect: DOMRect | null) =>
      rect
        ? [rect.left, rect.top, rect.right, rect.bottom].map(Math.round)
        : null;
    return {
      ok: true,
      chipCount: chipResults.length,
      chipsOk: chipResults.every(Boolean),
      checkboxOk,
      surfaceRectDebug: toDebug(surfaceRect),
      checkboxRectDebug: toDebug(checkboxRect),
      rowCount: rowResults.length,
      rowsOk: rowResults.every(Boolean),
      payloadOk,
      expandOk: expandButtons.every(Boolean),
      paginationOk,
    };
  });
  expect(geometry).toMatchObject({
    ok: true,
    chipCount: 6,
    chipsOk: true,
    checkboxOk: true,
    rowsOk: true,
    payloadOk: true,
    expandOk: true,
    paginationOk: true,
  });
  expect(geometry.rowCount).toBeGreaterThan(0);
  await expect(
    page.getByRole("button", { name: "Pin Event stream to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.getByRole("button", { name: "Hide quick guides" }).click();
  await eventsSurface
    .getByRole("heading", { name: "Realtime Event Stream" })
    .scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-events-slice/events-390.png",
    fullPage: true,
  });
  await eventsSurface.locator(".mc-pagination").scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "../../runtime/qa/p5-events-slice/events-390-pagination.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
