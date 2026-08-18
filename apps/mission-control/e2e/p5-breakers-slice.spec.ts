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

async function emitWsEvent(
  request: APIRequestContext,
  payload: {
    event_type?: string;
    entity?: string;
    payload?: Record<string, unknown>;
  },
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/ws-event`, {
    headers: AUTH_HEADERS,
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
 * P5 Basement · Breakers & Scheduler room slice: the existing Focus surface
 * gains operations-facing Breakers and Scheduler sections fed by the one
 * authoritative Mission Control controller. Real gateway payloads drive core
 * and plugin breaker facts, scheduler/lock/stop-reason posture, scheduled-job
 * Run and Pause/Resume controls with an honest forced failure, six-item job
 * pagination, the registry-backed pin with full-canvas byte immutability, and
 * the Office door through refusal/restore, reload, and 390px geometry with
 * console-error and requestfailed capture.
 */

test("@core @p5-breakers breakers & scheduler room identity, honest operations facts, job controls, pin-to-office, and the office door hold at desktop and 390px", async ({
  page,
  request,
}) => {
  test.setTimeout(180_000);
  const browserErrors: string[] = [];
  const responseCounts = new Map<string, number>();
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      // Record the source URL so the one deliberate forced-failure endpoint
      // can be excepted narrowly instead of blanket-ignoring load failures.
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
    responseCounts.set(
      response.url(),
      (responseCounts.get(response.url()) ?? 0) + 1,
    );
    if (response.status() >= 400) {
      browserErrors.push(`${response.status()} ${response.url()}`);
    }
  });

  await completeQuickstartLocalOnboarding(page);
  await dismissVisibleToasts(page);

  // The Breakers & Scheduler room lights exactly one lamp on its dedicated
  // focus route and lands on the operations-facing Breakers section.
  const activeRooms = page.locator(".mc-nav-item-active");
  const roomButton = page.locator(
    'button[title="BF · Breakers & Scheduler"]',
  );
  await expect(roomButton).toHaveCount(1);
  await roomButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Breakers & Scheduler",
  );
  await expect(
    page.getByRole("tab", { name: /^Breakers/ }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("heading", { name: "Circuit breakers" }),
  ).toBeVisible();
  for (const tabName of [/^Queue/, /^System Status/, /^Scheduler/]) {
    await expect(page.getByRole("tab", { name: tabName })).toBeVisible();
  }

  // Loaded-empty truth: the gateway has answered, so empty means empty — and
  // recovery is automatic, so no reset control may exist anywhere.
  const breakersSurface = page.locator("article.mc-surface").filter({
    has: page.getByRole("heading", { name: "Circuit breakers" }),
  });
  await expect(breakersSurface).toContainText("No open core breakers.");
  await expect(breakersSurface).toContainText("No faulted plugin runtimes.");
  await expect(breakersSurface).toContainText("no manual reset");
  expect(
    await breakersSurface
      .locator("button")
      .evaluateAll((buttons) =>
        buttons.filter((button) => /reset/i.test(button.textContent ?? ""))
          .length,
      ),
  ).toBe(0);

  // Real operational posture through the gateway seam: one open core
  // breaker, one faulted plugin runtime, stop reasons, and a job fleet with
  // a deliberately failing job for the honest-error proof.
  const wsDot = page.locator(".mc-connection-dot").first();
  await expect
    .poll(async () => wsDot.getAttribute("title"), { timeout: 20_000 })
    .toBe("ws: connected");
  await setOpsState(request, {
    circuit_breakers: [
      {
        scope: "provider",
        target_id: "openai-e2e-breaker",
        state: "open",
        consecutive_failures: 4,
        cooldown_until: Date.now() + 5 * 60_000,
        last_error_code: "timeout_e2e",
        updated_at: Date.now() - 2 * 60_000,
      },
    ],
    plugin_breakers: [
      {
        plugin_id: "plugin-e2e-bridge",
        enabled: true,
        faulted: true,
        disabled_until_ms: Date.now() + 10 * 60_000,
        consecutive_failures: 3,
        last_error_code: "spawn_failed",
        last_error: "binary exited with code 7",
        last_success_ms: Date.now() - 60 * 60_000,
        last_invoked_ms: Date.now() - 4 * 60_000,
      },
    ],
    top_stop_reasons: [
      { code: "budget_exhausted_e2e", count: 3 },
      { code: "circuit_open_e2e", count: 1 },
    ],
    fail_job_run_ids: ["e2e-job-flaky"],
    append_jobs: [
      ...Array.from({ length: 6 }, (_, index) => ({
        job_id: `e2e-job-sched-${String(index + 1).padStart(2, "0")}`,
        name: `E2E scheduled job ${index + 1}`,
        interval_seconds: 600 * (index + 1),
      })),
      {
        job_id: "e2e-job-flaky",
        name: "Flaky replication sweep",
        cron_expr: "*/5 * * * *",
        schedule_kind: "cron",
      },
      { job_id: "e2e-job-paused", name: "Paused digest", enabled: false },
    ],
  });
  await emitWsEvent(request, {
    event_type: "job.updated",
    entity: "job",
    payload: { job_id: "e2e-job-sched-01" },
  });

  const coreGroup = breakersSurface.locator('[aria-label="Core breakers"]');
  await expect(coreGroup).toContainText("openai-e2e-breaker", {
    timeout: 20_000,
  });
  await expect(coreGroup).toContainText("provider");
  await expect(coreGroup).toContainText("open");
  await expect(coreGroup).toContainText("4 consecutive failures");
  await expect(coreGroup).toContainText("timeout_e2e");
  await expect(coreGroup).toContainText("Cooldown until");
  const pluginGroup = breakersSurface.locator(
    '[aria-label="Plugin runtimes"]',
  );
  await expect(pluginGroup).toContainText("plugin-e2e-bridge");
  await expect(pluginGroup).toContainText("faulted");
  await expect(pluginGroup).toContainText("3 consecutive failures");
  await expect(pluginGroup).toContainText("binary exited with code 7");
  await expect(pluginGroup).toContainText("Disabled until");
  await expect(
    page.getByRole("tab", { name: /^Breakers/ }).locator(".mc-sub-tab-count"),
  ).toHaveText("2");
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/breakers-live-desktop.png",
    fullPage: true,
  });

  // Scheduler facts come from jobsStatus only; raw lock paths stay reserved
  // for the later File-lock machinery room.
  await page.getByRole("tab", { name: /^Scheduler/ }).click();
  const schedulerSurface = page.locator("article.mc-surface").filter({
    has: page.getByRole("heading", { name: "Scheduler", exact: true }),
  });
  await expect(schedulerSurface.locator(".mc-sched-posture")).toContainText(
    "running",
  );
  await expect(schedulerSurface.locator(".mc-sched-posture")).toContainText(
    "Lock held by stub-gateway",
  );
  await expect(schedulerSurface).not.toContainText(
    "/tmp/carsinos-scheduler.lock",
  );
  const stats = schedulerSurface.locator(".mc-sched-stats");
  await expect(stats).toContainText("Jobs total");
  await expect(stats).toContainText("9");
  await expect(stats).toContainText("Enabled");
  await expect(stats).toContainText("8");
  await expect(schedulerSurface).toContainText("budget_exhausted_e2e");
  await expect(schedulerSurface).toContainText("×3");
  await expect(schedulerSurface).toContainText("circuit_open_e2e");
  await expect(schedulerSurface).toContainText("×1");
  await schedulerSurface
    .locator(".mc-sched-posture")
    .scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/scheduler-desktop-facts.png",
    fullPage: true,
  });

  // Six-item job pagination with honest boundaries over nine real jobs.
  const jobRows = schedulerSurface.locator(".mc-sched-job");
  const schedulerPagination = schedulerSurface.locator(".mc-pagination-info");
  await expect(jobRows).toHaveCount(6);
  await expect(jobRows.first()).toContainText("Gateway heartbeat check");
  await expect(schedulerPagination).toHaveText("1 / 2");
  await expect(
    schedulerSurface.getByRole("button", { name: "Prev" }),
  ).toBeDisabled();

  // Run now reuses the existing mutation, targets the exact job once, and
  // changes only that job's authoritative run timestamp.
  const heartbeatRunUrl = `${GATEWAY_URL}/api/v1/jobs/job-heartbeat/run`;
  const jobsBeforeRunResponse = await request.get(`${GATEWAY_URL}/api/v1/jobs`, {
    headers: AUTH_HEADERS,
  });
  const jobsBeforeRun = (await jobsBeforeRunResponse.json()).items as Array<{
    job_id: string;
    last_run_at: number | null;
  }>;
  const heartbeatBefore = jobsBeforeRun.find(
    (job) => job.job_id === "job-heartbeat",
  )?.last_run_at;
  await jobRows
    .filter({ hasText: "Gateway heartbeat check" })
    .getByRole("button", { name: "Run now" })
    .click();
  await expect(page.getByText(/Job run started \(queued\)/)).toBeVisible();
  await expect
    .poll(async () => {
      const response = await request.get(`${GATEWAY_URL}/api/v1/jobs`, {
        headers: AUTH_HEADERS,
      });
      const items = (await response.json()).items as Array<{
        job_id: string;
        last_run_at: number | null;
      }>;
      return items.find((job) => job.job_id === "job-heartbeat")?.last_run_at;
    })
    .not.toBe(heartbeatBefore);
  expect(responseCounts.get(heartbeatRunUrl)).toBe(1);
  await dismissVisibleToasts(page);

  // Pause and Resume reuse the existing toggle mutation and re-render from
  // the authoritative refetch, not invented state.
  const pauseRow = jobRows.filter({ hasText: "E2E scheduled job 1" });
  await pauseRow.getByRole("button", { name: "Pause" }).click();
  await expect(pauseRow.getByRole("button", { name: "Resume" })).toBeVisible({
    timeout: 15_000,
  });
  await expect(pauseRow).toContainText("paused");
  await pauseRow.getByRole("button", { name: "Resume" }).click();
  await expect(pauseRow.getByRole("button", { name: "Pause" })).toBeVisible({
    timeout: 15_000,
  });
  await expect(pauseRow).toContainText("enabled");
  await dismissVisibleToasts(page);

  // Honest failure copy on the exact row when the gateway refuses a run;
  // the operator keeps every other row and control.
  await schedulerSurface.getByRole("button", { name: "Next" }).click();
  await expect(schedulerPagination).toHaveText("2 / 2");
  await expect(jobRows).toHaveCount(3);
  const flakyRow = jobRows.filter({ hasText: "Flaky replication sweep" });
  await flakyRow.getByRole("button", { name: "Run now" }).click();
  await expect(flakyRow.getByRole("alert")).toContainText("Action failed:", {
    timeout: 15_000,
  });
  await expect(
    jobRows.filter({ hasText: "Paused digest" }).getByRole("button", {
      name: "Resume",
    }),
  ).toBeVisible();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/scheduler-desktop-error.png",
    fullPage: true,
  });

  // Queue and System Status stay one tap away; the attention queue is
  // untouched by the room rename.
  await page.getByRole("tab", { name: /^Queue/ }).click();
  await expect(page.getByText("Operator Focus Queue")).toBeVisible();
  await expect(
    page.locator(".mc-focus-item").filter({ hasText: "Approval requested:" }),
  ).toHaveCount(2);

  // A full default canvas refuses the pin and leaves the exact persisted
  // config untouched, byte for byte.
  await page.getByRole("tab", { name: /^Breakers/ }).click();
  const pin = page.getByRole("button", {
    name: "Pin Breakers & Scheduler to Office",
  });
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
    path: "../../runtime/qa/p5-breakers-slice/breakers-full-canvas-refusal.png",
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
        (placement: { id?: string }) => placement.id === "breakers",
      );
    }),
  ).toBe(false);

  // Free one medium block; the pin succeeds without touching the operator's
  // section or any operational state.
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
  const operationalSnapshot = async () => {
    const [approvals, jobs, jobsStatus, pluginStatus] = await Promise.all([
      request.get(`${GATEWAY_URL}/api/v1/approvals?status=requested&limit=200`, {
        headers: AUTH_HEADERS,
      }),
      request.get(`${GATEWAY_URL}/api/v1/jobs`, { headers: AUTH_HEADERS }),
      request.get(`${GATEWAY_URL}/api/v1/jobs/status`, {
        headers: AUTH_HEADERS,
      }),
      request.get(
        `${GATEWAY_URL}/api/v1/extensions/plugins/status?include_disabled=true`,
        { headers: AUTH_HEADERS },
      ),
    ]);
    const statusBody = await jobsStatus.json();
    delete statusBody.now_utc;
    return {
      approvals: await approvals.json(),
      jobs: await jobs.json(),
      jobsStatus: statusBody,
      pluginStatus: await pluginStatus.json(),
    };
  };
  const operationsBeforePin = await operationalSnapshot();
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "On the Office canvas.",
  );
  await expect(
    page.getByRole("tab", { name: /^Breakers/ }),
  ).toHaveAttribute("aria-selected", "true");

  // Pinning is config-only: the complete approval, job, scheduler/breaker,
  // stop-reason, and plugin-runtime responses remain unchanged.
  expect(await operationalSnapshot()).toEqual(operationsBeforePin);

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
    path: "../../runtime/qa/p5-breakers-slice/breakers-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut names its floor and consumes config events live.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-breakers");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await shortcut.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/office-shortcut-block.png",
    fullPage: true,
  });
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "breakers"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-breakers")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "breakers"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-breakers")).toBeVisible();

  // The door opens the exact room by stable id and lands on operations.
  await shortcut
    .getByRole("button", { name: "Open Breakers & Scheduler" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Breakers & Scheduler",
  );
  await expect(
    page.getByRole("heading", { name: "Circuit breakers" }),
  ).toBeVisible();

  // The pin is config: it survives a full reload and still opens the exact
  // room afterwards.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await dismissVisibleToasts(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-breakers");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open Breakers & Scheduler" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Breakers & Scheduler",
  );
  await expect(
    page.getByRole("heading", { name: "Circuit breakers" }),
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
  await expect(roomButton).toHaveCount(0);
  await page
    .getByTestId("office-block-breakers")
    .getByRole("button", { name: "Open Breakers & Scheduler" })
    .click();
  await expect(
    page.getByTestId("office-block-breakers").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await page.getByTestId("office-block-breakers").evaluate((block) => {
    block.scrollIntoView({ block: "center", inline: "nearest" });
  });
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/breakers-disabled-door.png",
    fullPage: true,
  });
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.basement;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(roomButton).toHaveCount(1);
  await expect(
    page.getByTestId("office-block-breakers").getByRole("status"),
  ).toHaveCount(0);
  await page.getByTestId("office-block-breakers").evaluate((block) => {
    block.scrollIntoView({ block: "center", inline: "nearest" });
  });
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/breakers-restored-door.png",
    fullPage: true,
  });
  // The restored door must actually execute and land in the exact room.
  await page
    .getByTestId("office-block-breakers")
    .getByRole("button", { name: "Open Breakers & Scheduler" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Breakers & Scheduler",
  );
  await expect(
    page.getByRole("heading", { name: "Circuit breakers" }),
  ).toBeVisible();

  // Narrow width: nonzero visible BS room mark plus inner containment for
  // tabs, breaker rows, scheduler facts, job controls, the action error, and
  // pagination — a document-level no-overflow check alone is insufficient.
  await page.setViewportSize({ width: 390, height: 844 });
  await roomButton.click();
  await expect(roomButton).toHaveClass(/mc-nav-item-active/);
  const mark = roomButton.locator(".mc-nav-room-mark");
  await expect(mark).toHaveText("BS");
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
  expect(
    await roomButton.evaluate((button) => {
      const markRect = button
        .querySelector(".mc-nav-room-mark")!
        .getBoundingClientRect();
      const badgeRect = button
        .querySelector(".mc-nav-badge")!
        .getBoundingClientRect();
      return (
        markRect.right <= badgeRect.left ||
        badgeRect.right <= markRect.left ||
        markRect.bottom <= badgeRect.top ||
        badgeRect.bottom <= markRect.top
      );
    }),
  ).toBe(true);

  const breakersGeometry = await page.evaluate(() => {
    const containsX = (outer: DOMRect, inner: DOMRect, slack = 1): boolean =>
      inner.width > 0 &&
      inner.height > 0 &&
      inner.left >= outer.left - slack &&
      inner.right <= outer.right + slack;
    const focusPage = document.querySelector(".mc-focus-page");
    if (!focusPage) return { ok: false, reason: "no focus page" };
    const pageRect = focusPage.getBoundingClientRect();
    const tabs = Array.from(focusPage.querySelectorAll(".mc-sub-tab")).map(
      (tab) => containsX(pageRect, tab.getBoundingClientRect()),
    );
    const surface = Array.from(
      document.querySelectorAll("article.mc-surface"),
    ).find((candidate) =>
      Array.from(candidate.querySelectorAll("h2")).some(
        (heading) => heading.textContent === "Circuit breakers",
      ),
    );
    if (!surface) return { ok: false, reason: "no breakers surface" };
    const surfaceRect = surface.getBoundingClientRect();
    const note = surface.querySelector(".mc-breakers-note");
    const noteOk = note
      ? containsX(surfaceRect, note.getBoundingClientRect())
      : false;
    const rows = Array.from(surface.querySelectorAll(".mc-breaker-row")).map(
      (row) => containsX(surfaceRect, row.getBoundingClientRect()),
    );
    const facts = Array.from(
      surface.querySelectorAll(".mc-breaker-facts"),
    ).map((fact) => containsX(surfaceRect, fact.getBoundingClientRect()));
    const pinButton = surface.querySelector(".mc-pin-to-office-button");
    const pinOk = pinButton
      ? containsX(surfaceRect, pinButton.getBoundingClientRect())
      : false;
    return {
      ok: true,
      tabCount: tabs.length,
      tabsOk: tabs.every(Boolean),
      noteOk,
      rowCount: rows.length,
      rowsOk: rows.every(Boolean),
      factsOk: facts.every(Boolean),
      pinOk,
    };
  });
  expect(breakersGeometry).toMatchObject({
    ok: true,
    tabCount: 4,
    tabsOk: true,
    noteOk: true,
    rowsOk: true,
    factsOk: true,
    pinOk: true,
  });
  expect(
    (breakersGeometry as { rowCount: number }).rowCount,
  ).toBeGreaterThanOrEqual(2);
  await page.getByRole("button", { name: "Hide quick guides" }).click();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/breakers-390.png",
    fullPage: true,
  });

  // Scheduler at 390: facts, second-page controls, the honest action error,
  // and pagination all stay inside their containers.
  await page.getByRole("tab", { name: /^Scheduler/ }).click();
  await schedulerSurface.getByRole("button", { name: "Next" }).click();
  await expect(schedulerPagination).toHaveText("2 / 2");
  await flakyRow.getByRole("button", { name: "Run now" }).click();
  await expect(flakyRow.getByRole("alert")).toContainText("Action failed:", {
    timeout: 15_000,
  });
  const schedulerPaginationNav = schedulerSurface.locator(".mc-pagination");
  await schedulerPaginationNav.scrollIntoViewIfNeeded();
  await expect(schedulerPaginationNav).toBeInViewport();
  const schedulerGeometry = await page.evaluate(() => {
    const containsX = (outer: DOMRect, inner: DOMRect, slack = 1): boolean =>
      inner.width > 0 &&
      inner.height > 0 &&
      inner.left >= outer.left - slack &&
      inner.right <= outer.right + slack;
    const contains = (outer: DOMRect, inner: DOMRect, slack = 1): boolean =>
      containsX(outer, inner, slack) &&
      inner.top >= outer.top - slack &&
      inner.bottom <= outer.bottom + slack;
    const surface = Array.from(
      document.querySelectorAll("article.mc-surface"),
    ).find((candidate) =>
      Array.from(candidate.querySelectorAll("h2")).some(
        (heading) => heading.textContent === "Scheduler",
      ),
    );
    if (!surface) return { ok: false, reason: "no scheduler surface" };
    const surfaceRect = surface.getBoundingClientRect();
    const posture = surface.querySelector(".mc-sched-posture");
    const postureOk = posture
      ? containsX(surfaceRect, posture.getBoundingClientRect())
      : false;
    const statEntries = Array.from(
      surface.querySelectorAll(".mc-sched-stats li"),
    ).map((entry) => containsX(surfaceRect, entry.getBoundingClientRect()));
    const rows = Array.from(surface.querySelectorAll(".mc-sched-job")).map(
      (row) => containsX(surfaceRect, row.getBoundingClientRect()),
    );
    const controls = Array.from(
      surface.querySelectorAll(".mc-sched-job button"),
    ).map((button) => containsX(surfaceRect, button.getBoundingClientRect()));
    const errorAlert = surface.querySelector(
      '.mc-sched-job [role="alert"]',
    );
    const errorOk = errorAlert
      ? containsX(surfaceRect, errorAlert.getBoundingClientRect())
      : false;
    const paginationNav = surface.querySelector(".mc-pagination");
    const paginationOk = paginationNav
      ? containsX(surfaceRect, paginationNav.getBoundingClientRect()) &&
        paginationNav.getBoundingClientRect().top >= 0 &&
        paginationNav.getBoundingClientRect().bottom <= window.innerHeight &&
        Array.from(paginationNav.querySelectorAll("button, span")).every(
          (control) =>
            contains(
              paginationNav.getBoundingClientRect(),
              control.getBoundingClientRect(),
            ),
        )
      : false;
    return {
      ok: true,
      postureOk,
      statCount: statEntries.length,
      statsOk: statEntries.every(Boolean),
      rowCount: rows.length,
      rowsOk: rows.every(Boolean),
      controlsOk: controls.every(Boolean),
      errorOk,
      paginationOk,
    };
  });
  expect(schedulerGeometry).toMatchObject({
    ok: true,
    postureOk: true,
    statCount: 3,
    statsOk: true,
    rowsOk: true,
    controlsOk: true,
    errorOk: true,
    paginationOk: true,
  });
  expect(
    (schedulerGeometry as { rowCount: number }).rowCount,
  ).toBeGreaterThan(0);
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-breakers-slice/scheduler-390-pagination-error.png",
    fullPage: true,
  });

  // The only tolerated entries are the two deliberate forced-failure runs —
  // a narrow, source-justified exception (the mock returns 500 for exactly
  // the flaky job this proof exercises, once at desktop and once at 390px).
  // Both the response record and Chrome's console load-failure line must
  // point at that exact endpoint; everything else must be clean.
  const flakyRunUrl = `${GATEWAY_URL}/api/v1/jobs/e2e-job-flaky/run`;
  const isAllowed = (entry: string): boolean =>
    entry === `500 ${flakyRunUrl}` ||
    (entry.startsWith("Failed to load resource") &&
      entry.includes(`[${flakyRunUrl}]`));
  const unexpectedErrors = browserErrors.filter((entry) => !isAllowed(entry));
  expect(unexpectedErrors).toEqual([]);
  expect(responseCounts.get(flakyRunUrl)).toBe(2);
  const allowedCount = browserErrors.filter(isAllowed).length;
  expect(allowedCount).toBeGreaterThanOrEqual(2);
  expect(allowedCount).toBeLessThanOrEqual(4);
});
