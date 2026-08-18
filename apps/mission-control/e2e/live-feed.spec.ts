import { expect, test, type APIRequestContext, type Page } from "./testHarness";
import {
  completeQuickstartLocalOnboarding,
  GATEWAY_URL,
  TEST_TOKEN,
} from "./onboardingFlow";
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
    recovery_retention_window_ms: 30 * 60_000,
    recovery_log_max_bytes: 50 * 1024 * 1024,
    mark_read_undo_window_ms: 5 * 60_000,
  },
};

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
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(
          (payload: { config: typeof OPS_CONFIG; recoveryKey: string }) => {
            window.localStorage.removeItem(payload.recoveryKey);
            window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
            window.localStorage.setItem("mc-opsux-runtime-v1", JSON.stringify(payload.config));
          },
          { config: OPS_CONFIG, recoveryKey: LIVE_FEED_RECOVERY_STORAGE_KEY }
        );
      },
    });
    await expect(page.getByTestId("live-feed-toggle")).toBeVisible();
    await waitForWsConnected(page);

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
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(
          (payload: { config: typeof OPS_CONFIG; recoveryKey: string }) => {
            window.localStorage.removeItem(payload.recoveryKey);
            window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
            window.localStorage.setItem("mc-opsux-runtime-v1", JSON.stringify(payload.config));
          },
          { config: OPS_CONFIG, recoveryKey: LIVE_FEED_RECOVERY_STORAGE_KEY }
        );
      },
    });
    await expect(page.getByTestId("live-feed-toggle")).toBeVisible();
    await waitForWsConnected(page);
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

  test("event volume and severity copy never flip incident mode; composed facts do", async ({ page, request }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(
          (payload: { config: typeof OPS_CONFIG; recoveryKey: string }) => {
            window.localStorage.removeItem(payload.recoveryKey);
            window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
            window.localStorage.setItem("mc-opsux-runtime-v1", JSON.stringify(payload.config));
          },
          { config: OPS_CONFIG, recoveryKey: LIVE_FEED_RECOVERY_STORAGE_KEY }
        );
      },
    });
    await expect(page.getByTestId("live-feed-toggle")).toBeVisible();
    await waitForWsConnected(page);

    await expect(page.locator(".mc-topbar")).not.toHaveClass(/mc-topbar-incident/);

    // An event burst and a critical-severity notice are copy and volume,
    // not authoritative facts. Neither may raise the fire alarm.
    await emitWsBurst(request, {
      count: 8,
      event_type: "system.alert",
      entity: "system",
      payload: {
        domain: "system",
        severity: "high",
        summary: "high-burst-not-a-fact",
      },
    });
    await emitWsEvent(request, {
      event_type: "system.alert",
      entity: "system",
      payload: {
        domain: "system",
        severity: "critical",
        summary: "critical-copy-not-a-fact",
      },
    });
    await page.getByTestId("live-feed-toggle").click();
    await expect(page.getByText("critical-copy-not-a-fact")).toBeVisible();
    await expect(page.locator(".mc-topbar")).not.toHaveClass(/mc-topbar-incident/);
    await expect(page.locator('[data-testid="incident-band"]')).toHaveCount(0);

    // A real authoritative fact — an open core circuit breaker — still
    // raises incident mode through the composed posture.
    const opsResponse = await request.post(`${GATEWAY_URL}/api/v1/e2e/ops-state`, {
      headers: { Authorization: `Bearer ${TEST_TOKEN}` },
      data: {
        circuit_breakers: [
          {
            scope: "provider",
            target_id: "openai-live-feed-fact",
            state: "open",
            consecutive_failures: 3,
            cooldown_until: Date.now() + 5 * 60_000,
            last_error_code: "timeout_live_feed",
            updated_at: Date.now(),
          },
        ],
      },
    });
    expect(opsResponse.ok()).toBeTruthy();
    await emitWsEvent(request, {
      event_type: "job.updated",
      entity: "job",
      payload: { job_id: "job-heartbeat" },
    });

    await expect(page.locator(".mc-topbar")).toHaveClass(/mc-topbar-incident/, {
      timeout: 20_000,
    });
    await expect(page.locator('[data-testid="incident-band"]')).toBeVisible();
    await expect(page.locator('[data-testid="incident-band"]')).toContainText(
      "openai-live-feed-fact",
    );
  });
});
