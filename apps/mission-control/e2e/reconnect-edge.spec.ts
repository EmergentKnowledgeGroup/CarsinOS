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

async function postE2eControl(
  request: APIRequestContext,
  path: string,
  payload: Record<string, unknown>
): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}${path}`, {
    headers: {
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    data: payload,
  });
  expect(response.ok()).toBeTruthy();
}

test.describe("mission-control reconnect edge handling @p3", () => {
  test("handles malformed frames and rapid flaps without state explosion", async ({
    page,
    request,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });
    await waitForWsConnected(page);

    await postE2eControl(request, "/api/v1/e2e/ws-malformed", {
      raw: "{definitely-not-json",
    });

    await postE2eControl(request, "/api/v1/e2e/ws-event", {
      event_type: "job.updated",
      entity: "job",
      payload: {
        domain: "jobs",
        severity: "normal",
        job_id: "post-malformed-job",
      },
    });
    await page.locator('[data-tour-id="nav-events"]').click();
    await expect(page.getByText("Realtime Event Stream")).toBeVisible();
    await expect(page.getByText("post-malformed-job")).toBeVisible();

    await postE2eControl(request, "/api/v1/e2e/ws-flap", {
      count: 3,
      interval_ms: 120,
      code: 1012,
    });
    await waitForWsConnected(page);

    await postE2eControl(request, "/api/v1/e2e/ws-burst", {
      count: 500,
      event_type: "job.updated",
      entity: "job",
      payload: {
        domain: "jobs",
        severity: "normal",
        summary: "reconnect-edge-burst",
      },
    });

    const eventsSubtitle = page
      .locator("article.mc-surface")
      .filter({ has: page.getByRole("heading", { name: "Realtime Event Stream" }) })
      .locator("header p")
      .first();
    await expect
      .poll(async () => (await eventsSubtitle.textContent())?.trim() ?? "")
      .toBe("400 events");
  });
});
