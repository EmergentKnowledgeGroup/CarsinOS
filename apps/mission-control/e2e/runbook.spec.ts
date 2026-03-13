import { expect, test, type Page } from "./testHarness";

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

async function moveWizardToConnectionStep(page: Page): Promise<void> {
  await openWizard(page);
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Step 2 of 6")).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Step 3 of 6")).toBeVisible();
}

async function completeLocalOnboarding(page: Page): Promise<void> {
  await moveWizardToConnectionStep(page);

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
}

async function enableRunbook(page: Page, options?: { strategy?: boolean }): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  if (options?.strategy) {
    await page.getByRole("checkbox", { name: "Strategy hub module" }).check();
  }
  await page.getByRole("checkbox", { name: "Runbook hub module" }).check();
  await page.keyboard.press("Escape");
}

test.describe("mission-control runbook @core", () => {
  test("hides Runbook until enabled and renders the shared runbook surface", async ({
    page,
  }) => {
    await completeLocalOnboarding(page);

    await expect(page.locator('[data-tour-id="nav-runbook"]')).toHaveCount(0);

    await enableRunbook(page);

    await expect(page.locator('[data-tour-id="nav-runbook"]')).toBeVisible();
    await page.locator('[data-tour-id="nav-runbook"]').click();
    await expect(page.getByTestId("runbook-page")).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Approval gate for incident recovery session/i })
    ).toBeVisible();

    await page.getByRole("button", { name: "Review approval" }).click();
    await expect(page.locator('[data-tour-id="nav-focus"]')).toHaveClass(/mc-nav-item-active/);
    await expect(page.getByText(/Approval requested:/).first()).toBeVisible();
  });

  test("adds runbook entry points across boards, calendar, and focus without replacing those tabs", async ({
    page,
  }) => {
    await completeLocalOnboarding(page);
    await enableRunbook(page, { strategy: true });
    await page.locator('[data-tour-id="nav-runbook"]').click();
    await expect(page.getByTestId("runbook-page")).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Approval gate for incident recovery session/i })
    ).toBeVisible();

    await page.locator('[data-tour-id="nav-boards"]').click();
    await expect(page.locator('[data-tour-id="nav-boards"]')).toHaveClass(/mc-nav-item-active/);
    await page
      .locator(".mc-card-title", { hasText: "Investigate gateway health" })
      .first()
      .click();
    await expect(page.locator(".mc-board-runbook-panel")).toBeVisible();
    await page
      .locator(".mc-board-runbook-panel")
      .getByRole("button", { name: "Open in Runbook" })
      .click();
    await expect(page.getByTestId("runbook-page")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Investigate gateway health" }).first()
    ).toBeVisible();

    await page.getByRole("button", { name: "Open board card" }).click();
    await expect(page.locator('[data-tour-id="nav-boards"]')).toHaveClass(/mc-nav-item-active/);

    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.getByRole("tab", { name: /Schedule/ }).click();
    const calendarRow = page.locator(".mc-table tbody tr", {
      hasText: "Gateway heartbeat check",
    });
    await expect(calendarRow.locator(".mc-cal-runbook-panel")).toBeVisible();
    await calendarRow.getByRole("button", { name: "Open in Runbook" }).click();
    await expect(page.getByTestId("runbook-page")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Gateway heartbeat check" }).first()
    ).toBeVisible();

    await page.locator('[data-tour-id="nav-focus"]').click();
    const focusItem = page.locator(".mc-focus-item", {
      hasText: "Approval requested:",
    });
    await expect(focusItem.locator(".mc-focus-runbook-panel")).toBeVisible();
    await focusItem.getByRole("button", { name: "Open Runbook" }).click();
    await expect(page.getByTestId("runbook-page")).toBeVisible();
    await expect(
      page
        .getByRole("heading", { name: "Approval gate for incident recovery session" })
        .first()
    ).toBeVisible();
  });
});
