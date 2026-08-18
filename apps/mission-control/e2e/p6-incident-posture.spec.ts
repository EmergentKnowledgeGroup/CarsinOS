import {
  expect,
  test,
  type APIRequestContext,
  type Page,
} from "./testHarness";
import {
  completeQuickstartLocalOnboarding,
  moveWizardToConnectionStep,
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

async function emitExecassEvent(
  request: APIRequestContext,
  payload: { event_name: string; summary: string },
): Promise<void> {
  const response = await request.post(
    `${GATEWAY_URL}/api/v1/e2e/execass-event`,
    {
      headers: AUTH_HEADERS,
      data: payload,
    },
  );
  expect(response.ok()).toBeTruthy();
}

async function recoverExecassIntegrity(
  request: APIRequestContext,
): Promise<void> {
  const response = await request.post(
    `${GATEWAY_URL}/api/v1/e2e/execass-integrity-recovered`,
    { headers: AUTH_HEADERS },
  );
  expect(response.ok()).toBeTruthy();
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

async function expectVisibleNonZeroRect(
  page: Page,
  selector: string,
): Promise<{ x: number; y: number; width: number; height: number }> {
  const locator = page.locator(selector);
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

async function enableIncidentAutoTrigger(page: Page): Promise<void> {
  await page.locator('button[data-tour-id="nav-config"]').click();
  await page.getByText("Choose what pages show").click();
  const toggle = page.getByLabel("Auto-switch to incident mode");
  await expect(toggle).toBeVisible();
  await toggle.check();
  await expect(toggle).toBeChecked();
  await page
    .locator(".mc-modal-header")
    .getByRole("button")
    .click();
  await expect(page.locator(".mc-settings-modal")).toHaveCount(0);
}

const BREAKER_FIXTURE = {
  circuit_breakers: [
    {
      scope: "provider",
      target_id: "openai-p6-incident",
      state: "open",
      consecutive_failures: 4,
      cooldown_until: Date.now() + 5 * 60_000,
      last_error_code: "timeout_p6",
      updated_at: Date.now() - 60_000,
    },
  ],
};

/**
 * P6 cross-floor slice 1 · authoritative incident posture: the composed
 * calm/unknown/incident model replaces the old critical/high event-burst
 * trigger. Proven here: honest Checking-before-loaded truth, calm truth,
 * a real open-breaker incident with auto incident mode, one plain-language
 * band with an exact walk-there destination, the operator toggle as a
 * same-cause override, authoritative auto-recovery clearing the band
 * without a click, a higher-priority receipt-integrity cause arriving live
 * through the one ExecAss stream from another floor, desktop and 390px
 * geometry, reduced motion, keyboard activation, and full console/page/
 * request-failure accounting.
 */

test("@core @p6-incident authoritative incident posture: composed triggers, walk-there, override, auto-recovery, and 390px truth", async ({
  page,
  request,
}) => {
  test.setTimeout(180_000);
  const browserErrors: string[] = [];
  let expectedIntegrityQuarantineResponses = 0;
  let expectedIntegrityQuarantineConsoleErrors = 0;
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      const rendered = `${message.text()} [${message.location().url ?? "no-url"}]`;
      if (
        rendered.includes("status of 500") &&
        rendered.includes("/api/v1/execass/summary")
      ) {
        expectedIntegrityQuarantineConsoleErrors += 1;
      } else {
        browserErrors.push(rendered);
      }
    }
  });
  page.on("requestfailed", (req) => {
    browserErrors.push(
      `request failed: ${req.method()} ${req.url()} ${req.failure()?.errorText ?? "unknown"}`,
    );
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      if (
        response.status() === 500 &&
        new URL(response.url()).pathname === "/api/v1/execass/summary"
      ) {
        expectedIntegrityQuarantineResponses += 1;
      } else {
        browserErrors.push(`${response.status()} ${response.url()}`);
      }
    }
  });

  const postureStatus = page.locator(
    '[data-testid="incident-posture-status"]',
  );
  const band = page.locator('[data-testid="incident-band"]');
  const topbar = page.locator(".mc-topbar");
  const incidentToggle = page.getByLabel("Toggle incident mode");

  // Unknown truth: with no gateway or token configured yet, the shell says
  // Checking — it never claims calm (or alarm) from unloaded sources.
  const wizardVisible = await moveWizardToConnectionStep(page);
  expect(wizardVisible).toBe(true);
  await expect(postureStatus).toHaveText("Checking");
  await expect(band).toHaveCount(0);
  await page.getByRole("button", { name: "Dismiss (24h)" }).click();
  await expect(page.locator(".mc-onboarding-overlay")).toHaveCount(0);
  await expect(postureStatus).toHaveText("Checking");
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/checking-desktop.png",
  );
  await page.evaluate(() => {
    localStorage.removeItem("mc-onboarding-dismissed-at-ms");
  });
  await page.reload();
  expect(await moveWizardToConnectionStep(page)).toBe(true);

  await completeQuickstartLocalOnboarding(page, {
    startFromConnectionStep: true,
  });
  await dismissVisibleToasts(page);
  await enableIncidentAutoTrigger(page);

  // The authoritative Office summary loads as a shell fact even though the
  // sitting starts on Boards. The owner never has to visit the Office first.
  await expect(postureStatus).toHaveText("Calm", { timeout: 20_000 });
  await expect(band).toHaveCount(0);
  await expect(topbar).not.toHaveClass(/mc-topbar-incident/);
  await expect(incidentToggle).not.toBeChecked();
  await dismissVisibleToasts(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/calm-desktop.png",
  );

  // One real open core breaker becomes exactly one plain-language incident
  // with auto incident mode and a walk-there action.
  await setOpsState(request, BREAKER_FIXTURE);
  await emitWsEvent(request, {
    event_type: "job.updated",
    entity: "job",
    payload: { job_id: "job-heartbeat" },
  });
  await expect(band).toBeVisible({ timeout: 20_000 });
  await expect(band).toHaveAttribute("role", "status");
  await expect(band).toHaveAttribute("aria-live", "polite");
  await expect(band).toContainText("Incident");
  await expect(band).toContainText(
    "A circuit breaker is open, so part of the system is paused.",
  );
  await expect(band).toContainText("provider:openai-p6-incident");
  await expect(postureStatus).toHaveText("Incident");
  await expect(topbar).toHaveClass(/mc-topbar-incident/);
  await expect(incidentToggle).toBeChecked();
  const walk = page.locator('[data-testid="incident-band-walk"]');
  await expect(walk).toHaveCount(1);
  await expect(walk).toHaveText("Go to Breakers & Scheduler");
  await dismissVisibleToasts(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/incident-breaker-desktop.png",
  );

  // The walk action lands on the exact stable room and the exact breaker,
  // activated through the keyboard so focus and Enter are proven usable.
  await walk.focus();
  await expect(walk).toBeFocused();
  await page.keyboard.press("Enter");
  const activeRooms = page.locator(".mc-nav-item-active");
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Breakers & Scheduler",
  );
  await expect(
    page.getByRole("tab", { name: /^Breakers/ }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    page.locator('[aria-label="Core breakers"]'),
  ).toContainText("openai-p6-incident");

  // The operator toggle is an override for exactly this cause: mode drops,
  // the band keeps telling the truth, and the same cause does not re-raise.
  await page.locator(".mc-incident-toggle").click();
  await expect(topbar).not.toHaveClass(/mc-topbar-incident/);
  await expect(incidentToggle).not.toBeChecked();
  await expect(band).toBeVisible();
  await emitWsEvent(request, {
    event_type: "job.updated",
    entity: "job",
    payload: { job_id: "job-heartbeat" },
  });
  await page.waitForTimeout(1_500);
  await expect(incidentToggle).not.toBeChecked();
  await expect(band).toBeVisible();

  // Authoritative recovery clears the band and the status without a click.
  await setOpsState(request, { circuit_breakers: [] });
  await emitWsEvent(request, {
    event_type: "job.updated",
    entity: "job",
    payload: { job_id: "job-heartbeat" },
  });
  await expect(band).toHaveCount(0, { timeout: 20_000 });
  await expect(postureStatus).toHaveText("Calm");
  await expect(topbar).not.toHaveClass(/mc-topbar-incident/);
  await dismissVisibleToasts(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/recovered-desktop.png",
  );

  // A receipt integrity failure arrives through the one durable ExecAss
  // stream while the shell sits on the Basement floor. It is a new,
  // higher-priority cause, so the earlier off-override does not silence
  // it: the band re-raises and targets the Office desk.
  await emitExecassEvent(request, {
    event_name: "execass.v1.receipt.integrity_failed",
    summary: "Receipt chain verification failed for delegation dlg-retreat.",
  });
  const quarantineProof = await request.get(
    `${GATEWAY_URL}/api/v1/execass/summary`,
    { headers: AUTH_HEADERS },
  );
  expect(quarantineProof.status()).toBe(500);
  expect(await quarantineProof.json()).toMatchObject({
    code: "execass.v1.receipt_integrity_quarantined",
    safe_human_message:
      "The authoritative summary could not be rendered safely.",
    safe_for_display: true,
  });
  await expect(band).toBeVisible({ timeout: 20_000 });
  await expect(band).toContainText(
    "A work receipt failed its integrity check and needs your review.",
  );
  await expect(band).toContainText(
    "Receipt chain verification failed for delegation dlg-retreat.",
  );
  await expect(postureStatus).toHaveText("Incident");
  await expect(incidentToggle).toBeChecked();
  const walkToDesk = page.locator('[data-testid="incident-band-walk"]');
  await expect(walkToDesk).toHaveText("Go to Office desk");
  await expect
    .poll(() => expectedIntegrityQuarantineResponses, { timeout: 20_000 })
    .toBe(1);
  await dismissVisibleToasts(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/incident-integrity-desktop.png",
  );

  // If the target floor is disabled, the shell refuses honestly instead of
  // exposing a dead walk button. Restoring the floor restores the action
  // without dismissing or recreating the authoritative incident.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.floorOverrides = {
      ...config.floorOverrides,
      office: { hidden: true },
    };
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(walkToDesk).toHaveCount(0);
  await expect(page.locator(".mc-incident-band-unroutable")).toHaveText(
    "Office desk is not available right now.",
  );
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.office;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(walkToDesk).toBeVisible();
  await walkToDesk.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "4F · Your desk");

  // 390px: the band wraps, every critical control keeps nonzero visible
  // geometry inside the viewport, and the document never scrolls sideways.
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(band).toBeVisible();
  const bandBox = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-band"]',
  );
  const messageBox = await expectVisibleNonZeroRect(
    page,
    ".mc-incident-band-message",
  );
  const walkBox = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-band-walk"]',
  );
  for (const child of [messageBox, walkBox]) {
    expect(child.x).toBeGreaterThanOrEqual(bandBox.x - 1);
    expect(child.x + child.width).toBeLessThanOrEqual(
      bandBox.x + bandBox.width + 1,
    );
    expect(child.y).toBeGreaterThanOrEqual(bandBox.y - 1);
    expect(child.y + child.height).toBeLessThanOrEqual(
      bandBox.y + bandBox.height + 1,
    );
  }
  expect(bandBox.x + bandBox.width).toBeLessThanOrEqual(391);
  expect(bandBox.y).toBeGreaterThanOrEqual(0);
  expect(bandBox.y + bandBox.height).toBeLessThanOrEqual(845);
  const statusBox = await expectVisibleNonZeroRect(
    page,
    '[data-testid="incident-posture-status"]',
  );
  const toggleBox = await expectVisibleNonZeroRect(
    page,
    ".mc-incident-toggle",
  );
  // The posture word must stay readable beside the toggle, not under it.
  const overlaps =
    statusBox.x < toggleBox.x + toggleBox.width &&
    toggleBox.x < statusBox.x + statusBox.width &&
    statusBox.y < toggleBox.y + toggleBox.height &&
    toggleBox.y < statusBox.y + statusBox.height;
  expect(overlaps).toBe(false);
  await expectNoHorizontalDocumentOverflow(page);
  await dismissVisibleToasts(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/incident-integrity-390.png",
  );

  // Recovery is also authoritative for integrity: a successful summary read
  // after the backend quarantine clears removes the latch without a click.
  await recoverExecassIntegrity(request);
  await expect(band).toHaveCount(0, { timeout: 20_000 });
  await expect(postureStatus).toHaveText("Calm");
  await expect(incidentToggle).not.toBeChecked();
  await dismissVisibleToasts(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/integrity-recovered-390.png",
  );

  // Reduced motion: a fresh sitting with prefers-reduced-motion still
  // renders a fully usable band for a new authoritative incident (the
  // global reduced-motion rule strips its animations) and the walk action
  // works. Onboarding runs again after the fresh load, like any sitting.
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await completeQuickstartLocalOnboarding(page);
  await dismissVisibleToasts(page);
  await setOpsState(request, BREAKER_FIXTURE);
  await emitWsEvent(request, {
    event_type: "job.updated",
    entity: "job",
    payload: { job_id: "job-heartbeat" },
  });
  await expect(band).toBeVisible({ timeout: 20_000 });
  await expect(band).toContainText("provider:openai-p6-incident");
  const reducedMotionStyle = await band.evaluate((element) => {
    const style = getComputedStyle(element);
    const beaconStyle = getComputedStyle(
      element.querySelector(".mc-incident-band-beacon")!,
    );
    return {
      animationDuration: style.animationDuration,
      transitionDuration: style.transitionDuration,
      beaconAnimationDuration: beaconStyle.animationDuration,
    };
  });
  for (const duration of Object.values(reducedMotionStyle)) {
    expect(Number.parseFloat(duration)).toBeLessThanOrEqual(0.00001);
  }
  await dismissVisibleToasts(page);
  await captureEvidence(
    page,
    "../../runtime/qa/p6-incident-posture/incident-reduced-motion-desktop.png",
  );
  await page.locator('[data-testid="incident-band-walk"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Breakers & Scheduler",
  );

  // Full accounting: no console errors, page errors, failed requests, or
  // HTTP error responses anywhere in this scenario.
  expect(expectedIntegrityQuarantineResponses).toBe(1);
  expect(expectedIntegrityQuarantineConsoleErrors).toBe(1);
  expect(browserErrors).toEqual([]);
});
