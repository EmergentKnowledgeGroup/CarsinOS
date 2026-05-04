import {
  expect,
  test,
  type APIRequestContext,
  type Page,
} from "./testHarness";

const E2E_APP_URL = "/?e2e=1";
const GATEWAY_URL = "http://127.0.0.1:19789";
const TEST_TOKEN = "stub-token-001";
const ASSISTANT_MODEL_ID = "qwen3.5-9b-instruct";

const OPS_CONFIG = {
  schema_version: "mc-opsux-runtime-v1",
  controls: {
    global_kill_switch: false,
    live_feed_drawer: true,
    incident_auto_trigger: true,
    usage_charts: true,
  },
  safety: {
    fail_safe_on_config_error: true,
    incident_high_burst_threshold: 5,
    incident_high_burst_window_ms: 60_000,
    incident_auto_cooldown_ms: 10 * 60_000,
    incident_health_degraded_trigger_ms: 30_000,
    incident_healthy_exit_ms: 5 * 60_000,
    recovery_retention_window_ms: 30 * 60_000,
    recovery_log_max_bytes: 50 * 1024 * 1024,
    mark_read_undo_window_ms: 5 * 60_000,
  },
};

async function openWizard(page: Page): Promise<void> {
  await page.addInitScript((payload: { config: typeof OPS_CONFIG }) => {
    window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
    window.localStorage.setItem("mc-opsux-runtime-v1", JSON.stringify(payload.config));
  }, { config: OPS_CONFIG });
  await page.goto(E2E_APP_URL);
  await expect(page.getByRole("dialog", { name: "Setup Wizard" })).toBeVisible();
}

async function waitForWsConnected(page: Page): Promise<void> {
  const wsDot = page.locator(".mc-connection-dot").first();
  await expect(wsDot).toBeVisible({ timeout: 20_000 });
  await expect
    .poll(async () => wsDot.getAttribute("title"), {
      timeout: 20_000,
      message: "Expected websocket status indicator to reach connected state.",
    })
    .toBe("ws: connected");
}

async function completeLocalOnboarding(page: Page): Promise<void> {
  await openWizard(page);
  const setupWizard = page.getByRole("dialog", { name: "Setup Wizard" });

  await setupWizard.getByRole("button", { name: "Continue" }).click();
  await setupWizard.getByRole("button", { name: "Continue" }).click();

  await setupWizard.getByLabel("Gateway URL").fill(GATEWAY_URL);
  await setupWizard.getByLabel("Gateway token").fill(TEST_TOKEN);
  await setupWizard.getByRole("button", { name: /Save \+ Connect/ }).click();
  await expect(setupWizard.getByText(/Connection status:\s*Connected/)).toBeVisible();
  await setupWizard.getByRole("button", { name: "Save connection + Continue" }).click();

  await setupWizard.getByRole("button", { name: "Start new agent" }).click();
  await setupWizard.getByLabel("Agent ID").fill("assistant-main");
  await setupWizard.getByLabel("Agent name").fill("Assistant");
  await setupWizard.getByRole("radio", { name: "Local connector" }).check();
  await setupWizard
    .getByRole("combobox", { name: "Assistant model" })
    .selectOption(ASSISTANT_MODEL_ID);
  await setupWizard.getByRole("button", { name: "Apply setup + Continue" }).click();

  await setupWizard.getByRole("button", { name: "Finish setup" }).click();
  await setupWizard.getByRole("button", { name: "Go to Boards" }).click();

  await expect(setupWizard).toBeHidden();
  await waitForWsConnected(page);
}

async function openCockpitTemplate(page: Page): Promise<void> {
  await page.locator('[data-tour-id="nav-cockpit"]').click();
  await expect(page.locator(".mc-cockpit-grid")).toBeVisible();
  const loadTemplateButton = page.getByRole("button", { name: "Load Ops Template" });
  const usagePanel = page.getByTestId("mc-usage-panel");
  await expect(loadTemplateButton.or(usagePanel).first()).toBeVisible();
  if (await loadTemplateButton.isVisible()) {
    await loadTemplateButton.click();
  }
  await expect(usagePanel).toBeVisible();
}

async function setUsageMode(
  request: APIRequestContext,
  payload: {
    mode: "available" | "unavailable" | "missing-optional" | "invalid-required";
    age_minutes?: number;
  }
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/usage-mode`, {
    headers: {
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    data: payload,
  });
  expect(response.ok()).toBeTruthy();
}

test.describe("mission-control usage charts @p4 @core", () => {
  test("renders today/week usage summaries and breakdowns when contract is available", async ({
    page,
    request,
  }) => {
    await completeLocalOnboarding(page);
    await setUsageMode(request, { mode: "available", age_minutes: 2 });
    await openCockpitTemplate(page);

    await page.getByRole("button", { name: "Refresh all" }).first().click();
    await expect(page.getByTestId("usage-not-available")).not.toBeVisible();
    await expect(page.getByTestId("usage-summary-today-cost")).toContainText("$");
    await expect(page.getByTestId("usage-trend-label")).not.toContainText("unavailable");
    await expect(page.getByTestId("usage-correlation-status")).toContainText("available");
  });

  test("shows stale warning when usage data is older than 15 minutes", async ({
    page,
    request,
  }) => {
    await completeLocalOnboarding(page);
    await setUsageMode(request, { mode: "available", age_minutes: 20 });
    await openCockpitTemplate(page);

    await page.getByRole("button", { name: "Refresh all" }).first().click();
    await expect(page.getByTestId("usage-not-available")).not.toBeVisible();
    await expect(page.getByText("Data is older than 15 minutes.")).toBeVisible();
  });

  test("shows concise budget warning when usage thresholds are breached", async ({
    page,
    request,
  }) => {
    await completeLocalOnboarding(page);
    await setUsageMode(request, { mode: "available", age_minutes: 2 });
    await openCockpitTemplate(page);

    await page.getByRole("button", { name: "Refresh all" }).first().click();
    await expect(page.getByTestId("usage-not-available")).not.toBeVisible();
    await expect(page.locator(".mc-usage-warning-list li")).toHaveCount(1);
    await expect(page.locator(".mc-usage-warning-list li").first()).toContainText("budget");
  });

  test("shows explicit not-available state when usage contract is unavailable", async ({
    page,
    request,
  }) => {
    await completeLocalOnboarding(page);
    await setUsageMode(request, { mode: "unavailable" });
    await openCockpitTemplate(page);

    await page.getByRole("button", { name: "Refresh all" }).first().click();
    await expect(page.getByTestId("usage-not-available")).toContainText("Not available");
  });

  test("keeps summaries visible when optional correlation slices are missing", async ({
    page,
    request,
  }) => {
    await completeLocalOnboarding(page);
    await setUsageMode(request, { mode: "missing-optional", age_minutes: 18 });
    await openCockpitTemplate(page);

    await page.getByRole("button", { name: "Refresh all" }).first().click();
    await expect(page.getByTestId("usage-not-available")).not.toBeVisible();
    await expect(page.getByTestId("usage-summary-today-cost")).toContainText("$");
    await expect(page.getByTestId("usage-correlation-status")).toContainText(
      "unavailable from gateway contract"
    );
    await expect(page.getByText("Data is older than 15 minutes.")).toBeVisible();
  });

  test("limits trend claims when usage data is older than 60 minutes", async ({
    page,
    request,
  }) => {
    await completeLocalOnboarding(page);
    await setUsageMode(request, { mode: "available", age_minutes: 90 });
    await openCockpitTemplate(page);

    await page.getByRole("button", { name: "Refresh all" }).first().click();
    await expect(page.getByTestId("usage-not-available")).not.toBeVisible();
    await expect(page.getByTestId("usage-trend-label")).toContainText("Trend limited");
  });

  test("disables charts when required usage fields are missing", async ({ page, request }) => {
    await completeLocalOnboarding(page);
    await setUsageMode(request, { mode: "invalid-required" });
    await openCockpitTemplate(page);

    await page.getByRole("button", { name: "Refresh all" }).first().click();
    await expect(page.getByTestId("usage-not-available")).toContainText(
      "missing required fields"
    );
  });
});
