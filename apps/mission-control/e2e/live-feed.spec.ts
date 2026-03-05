import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

const E2E_APP_URL = "/?e2e=1";
const GATEWAY_URL = "http://127.0.0.1:19789";
const TEST_TOKEN = "stub-token-001";
const ASSISTANT_MODEL_ID = "qwen3.5-9b-instruct";
const LIVE_FEED_RECOVERY_STORAGE_KEY = "mc-live-feed-recovery-v1";

const OPS_CONFIG = {
  schema_version: "mc-opsux-runtime-v1",
  controls: {
    global_kill_switch: false,
    live_feed_drawer: true,
    incident_auto_trigger: true,
    usage_charts: false,
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
  await page.addInitScript((payload: { config: typeof OPS_CONFIG; recoveryKey: string }) => {
    window.localStorage.removeItem(payload.recoveryKey);
    window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
    window.localStorage.setItem("mc-opsux-runtime-v1", JSON.stringify(payload.config));
  }, { config: OPS_CONFIG, recoveryKey: LIVE_FEED_RECOVERY_STORAGE_KEY });
  await page.goto(E2E_APP_URL);
  await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeVisible();
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

  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Step 2 of 6")).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Step 3 of 6")).toBeVisible();

  await page.getByLabel("Gateway URL").fill(GATEWAY_URL);
  await page.getByLabel("Gateway token").fill(TEST_TOKEN);
  await page.getByRole("button", { name: /Save \+ Connect/ }).click();
  await expect(page.getByText(/Connection status:\s*Connected/)).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();

  await expect(page.getByText("Step 4 of 6")).toBeVisible();
  await page.getByLabel("Agent ID").fill("assistant-main");
  await page.getByLabel("Agent name").fill("Assistant");
  await page.getByRole("radio", { name: "Local connector" }).check();
  await page
    .getByPlaceholder("Or paste assistant model ID manually")
    .fill(ASSISTANT_MODEL_ID);
  await page.getByRole("button", { name: "Continue" }).click();

  await expect(page.getByText("Step 5 of 6")).toBeVisible();
  await page.getByRole("button", { name: "Finalize" }).click();
  await expect(page.getByText("Step 6 of 6")).toBeVisible();
  await page.getByRole("button", { name: "Go to Boards" }).click();

  await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeHidden();
  await expect(page.getByTestId("live-feed-toggle")).toBeVisible();
  await waitForWsConnected(page);
}

async function emitWsEvent(
  request: APIRequestContext,
  payload: {
    event_type?: string;
    entity?: string;
    payload?: Record<string, unknown>;
  }
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/ws-event`, {
    headers: {
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
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
  }
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/ws-burst`, {
    headers: {
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    data: payload,
  });
  expect(response.ok()).toBeTruthy();
}

test.describe("mission-control live feed + incident automation @p2", () => {
  test("live feed supports pause + unread behavior", async ({ page, request }) => {
    await completeLocalOnboarding(page);

    await page.getByTestId("live-feed-toggle").click();
    await expect(page.getByTestId("live-feed-drawer")).toHaveAttribute("data-open", "true");

    await emitWsEvent(request, {
      event_type: "job.updated",
      entity: "job",
      payload: {
        domain: "jobs",
        severity: "normal",
        summary: "auto-read-open-drawer",
      },
    });

    await expect(page.getByText("auto-read-open-drawer")).toBeVisible();
    await expect(page.locator(".mc-live-feed-toggle-badge")).toHaveCount(0);

    await emitWsEvent(request, {
      event_type: "gateway.notice",
      entity: "system",
      payload: {
        domain: "system",
        severity: "normal",
        summary: "large-payload-summary",
        blob: "x".repeat(120_000),
      },
    });
    await expect(page.getByText("large-payload-summary")).toBeVisible();

    await page.getByTestId("live-feed-pause").click();
    await emitWsEvent(request, {
      event_type: "job.updated",
      entity: "job",
      payload: {
        domain: "jobs",
        severity: "high",
        summary: "paused-unread-event",
      },
    });

    await expect(page.locator(".mc-live-feed-toggle-badge")).toContainText("1");
    await expect(page.getByText("paused-unread-event")).toBeVisible();

    await page.getByTestId("live-feed-pause").click();
    await expect(page.locator(".mc-live-feed-toggle-badge")).toHaveCount(0);

    await page.getByTestId("live-feed-toggle").click();
    await expect(page.getByTestId("live-feed-drawer")).toHaveAttribute("data-open", "false");

    await emitWsEvent(request, {
      event_type: "approval.requested",
      entity: "approval",
      payload: {
        domain: "approvals",
        severity: "high",
        summary: "closed-drawer-unread-event",
      },
    });

    await expect(page.locator(".mc-live-feed-toggle-badge")).toBeVisible();
  });

  test("soft clear keeps events recoverable with undo", async ({ page, request }) => {
    await completeLocalOnboarding(page);
    await page.getByTestId("live-feed-toggle").click();

    await emitWsEvent(request, {
      event_type: "gateway.notice",
      payload: {
        domain: "system",
        severity: "normal",
        summary: "recoverable-event-one",
      },
    });
    await emitWsEvent(request, {
      event_type: "gateway.notice",
      payload: {
        domain: "system",
        severity: "normal",
        summary: "recoverable-event-two",
      },
    });

    await expect(page.getByText("recoverable-event-one")).toBeVisible();
    await expect(page.getByText("recoverable-event-two")).toBeVisible();

    await page.getByTestId("live-feed-soft-clear").click();
    await expect(page.getByText("No events yet.")).toBeVisible();

    await page.getByTestId("live-feed-undo-soft-clear").click();
    await expect(page.getByText("recoverable-event-one")).toBeVisible();

    await page.getByTestId("live-feed-soft-clear").click();
    await page.getByTestId("live-feed-restore-history").click();
    await expect(page.getByText("recoverable-event-two")).toBeVisible();
  });

  test("incident auto-trigger honors cooldown but re-enters on critical", async ({ page, request }) => {
    await completeLocalOnboarding(page);

    await expect(page.locator(".mc-topbar")).not.toHaveClass(/mc-topbar-incident/);

    await emitWsBurst(request, {
      count: 5,
      event_type: "system.alert",
      entity: "system",
      payload: {
        domain: "system",
        severity: "high",
        summary: "high-burst",
      },
    });

    await expect(page.locator(".mc-topbar")).toHaveClass(/mc-topbar-incident/);

    await page.locator(".mc-incident-toggle").click();
    await expect(page.locator(".mc-topbar")).not.toHaveClass(/mc-topbar-incident/);

    await emitWsBurst(request, {
      count: 5,
      event_type: "system.alert",
      entity: "system",
      payload: {
        domain: "system",
        severity: "high",
        summary: "high-burst-cooldown",
      },
    });

    await expect(page.locator(".mc-topbar")).not.toHaveClass(/mc-topbar-incident/);

    await emitWsEvent(request, {
      event_type: "system.alert",
      entity: "system",
      payload: {
        domain: "system",
        severity: "critical",
        summary: "critical-bypass-cooldown",
      },
    });

    await expect(page.locator(".mc-topbar")).toHaveClass(/mc-topbar-incident/);
  });
});
