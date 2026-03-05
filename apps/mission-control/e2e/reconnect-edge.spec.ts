import {
  expect,
  test,
  type APIRequestContext,
  type Page,
} from "@playwright/test";

const E2E_APP_URL = "/?e2e=1";
const GATEWAY_URL = "http://127.0.0.1:19789";
const TEST_TOKEN = "stub-token-001";
const ASSISTANT_MODEL_ID = "qwen3.5-9b-instruct";

async function openWizard(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
  });
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
  await expect(page.getByText("Step 2 of 8")).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Step 3 of 8")).toBeVisible();

  await page.getByLabel("Gateway URL").fill(GATEWAY_URL);
  await page.getByLabel("Gateway token").fill(TEST_TOKEN);
  await page.getByRole("button", { name: "Save + Connect" }).click();
  await expect(page.getByText(/Connection status:\s*Connected/)).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();

  await expect(page.getByText("Step 4 of 8")).toBeVisible();
  await page.getByRole("button", { name: /Use Selected Agent|Create Agent/ }).click();
  await expect(page.getByText(/Agent status:\s*Ready/)).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();

  await expect(page.getByText("Step 5 of 8")).toBeVisible();
  await page.getByRole("radio", { name: "Local connector" }).check();
  await page
    .getByPlaceholder("Or paste assistant model ID manually")
    .fill(ASSISTANT_MODEL_ID);
  await page.getByRole("button", { name: "Apply Provider Setup" }).click();
  await expect(page.getByText(/Provider status:\s*Ready/)).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();

  await expect(page.getByText("Step 6 of 8")).toBeVisible();
  await page.getByRole("button", { name: "Apply Routing" }).click();
  await expect(page.getByText(/Routing status:\s*Ready/)).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();

  await expect(page.getByText("Step 7 of 8")).toBeVisible();
  await page.getByRole("button", { name: "Finalize" }).click();
  await expect(page.getByText("Step 8 of 8")).toBeVisible();
  await page.getByRole("button", { name: "Go to Boards" }).click();

  await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeHidden();
  await waitForWsConnected(page);
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
    await completeLocalOnboarding(page);

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
