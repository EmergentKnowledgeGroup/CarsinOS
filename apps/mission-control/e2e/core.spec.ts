import { expect, test, type Page } from "@playwright/test";

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

async function completeLocalOnboarding(
  page: Page,
  options?: {
    startFromConnectionStep?: boolean;
  }
): Promise<void> {
  if (!options?.startFromConnectionStep) {
    await moveWizardToConnectionStep(page);
  }

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

test.describe("mission-control core onboarding + crash-proofing @core", () => {
  test("auto-opens onboarding, supports dismiss, and can reopen from settings", async ({ page }) => {
    await openWizard(page);

    await page.getByRole("button", { name: "Dismiss (24h)" }).click();
    await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeHidden();

    await page.locator('[data-tour-id="nav-config"]').click();
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
    await page.getByRole("button", { name: "Setup Wizard" }).click();

    await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeVisible();
  });

  test("keeps onboarding token plaintext only during active entry and does not expose it after setup", async ({
    page,
  }) => {
    await moveWizardToConnectionStep(page);

    const tokenField = page.getByLabel("Gateway token").first();
    await expect(tokenField).toHaveAttribute("type", "text");
    await tokenField.fill(TEST_TOKEN);
    await expect(tokenField).toHaveValue(TEST_TOKEN);

    await completeLocalOnboarding(page, { startFromConnectionStep: true });
    await expect(page.locator("body")).not.toContainText(TEST_TOKEN);

    await page.locator('[data-tour-id="nav-config"]').click();
    await page.getByRole("button", { name: "Setup Wizard" }).click();
    await page.getByRole("button", { name: "Continue" }).click();
    await page.getByRole("button", { name: "Continue" }).click();
    await expect(page.getByText("Step 3 of 6")).toBeVisible();
    await expect(page.getByLabel("Gateway token").first()).not.toHaveValue(TEST_TOKEN);
  });

  test("continues from connect step without manual Save + Connect click", async ({ page }) => {
    await moveWizardToConnectionStep(page);

    await page.getByLabel("Gateway URL").fill(GATEWAY_URL);
    await page.getByLabel("Gateway token").fill(TEST_TOKEN);
    await page.getByRole("button", { name: "Continue" }).click();
    await expect(page.getByText("Step 4 of 6")).toBeVisible();

    await page.getByLabel("Agent ID").fill("assistant-continue");
    await page.getByLabel("Agent name").fill("Assistant Continue");
    await page.getByRole("radio", { name: "Local connector" }).check();
    await page
      .getByPlaceholder("Or paste assistant model ID manually")
      .fill(ASSISTANT_MODEL_ID);
    await page.getByRole("button", { name: "Continue" }).click();

    await expect(page.getByText("Step 5 of 6")).toBeVisible();
    await expect(
      page
        .locator(".mc-onboarding-checklist li")
        .filter({ hasText: "Connection verified" })
        .first()
    ).toHaveClass(/done/);
  });

  test("continues after manual Save + Connect even when token input is cleared", async ({ page }) => {
    await moveWizardToConnectionStep(page);

    await page.getByLabel("Gateway URL").fill(GATEWAY_URL);
    await page.getByLabel("Gateway token").fill(TEST_TOKEN);
    await page.getByRole("button", { name: /Save \+ Connect/ }).click();
    await expect(page.getByText(/Connection status:\s*Connected/)).toBeVisible();
    await expect(page.getByLabel("Gateway token")).toHaveValue("");

    await page.getByRole("button", { name: "Continue" }).click();
    await expect(page.getByText("Step 4 of 6")).toBeVisible();
  });

  test("connects via deterministic stub gateway and loads baseline", async ({ page }) => {
    await completeLocalOnboarding(page);

    await expect(page.getByText("Investigate gateway health")).toBeVisible();

    await page.locator('[data-tour-id="nav-config"]').click();
    await expect(page.getByText(/health:\s*up/)).toBeVisible();
    await expect(page.getByText(/ws:\s*connected/)).toBeVisible();
  });

  test("enables Strategy and follows linked work back from runtime surfaces @core", async ({
    page,
  }) => {
    await completeLocalOnboarding(page);

    await page.locator('[data-tour-id="nav-strategy"]').click();
    await expect(page.getByText("Strategy hub is disabled")).toBeVisible();

    await page.locator('[data-tour-id="nav-config"]').click();
    await page.getByRole("checkbox", { name: "Strategy hub module" }).check();
    await page.keyboard.press("Escape");

    await page.locator('[data-tour-id="nav-strategy"]').click();
    await expect(page.getByTestId("strategy-page")).toBeVisible();
    await expect(page.getByRole("heading", { name: "Goals + Projects" })).toBeVisible();
    await expect(
      page
        .locator(".mc-strategy-task-list")
        .getByRole("button", { name: /Investigate gateway health/i })
        .first()
    ).toBeVisible();

    await page.locator('[data-tour-id="nav-boards"]').click();
    await page.getByText("Investigate gateway health").first().click();
    await expect(page.locator(".mc-board-strategy-panel")).toBeVisible();
    await page.getByRole("button", { name: "Open in Strategy" }).click();
    await expect(page.getByTestId("strategy-page")).toBeVisible();

    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.getByRole("tab", { name: /Schedule/ }).click();
    await expect(page.getByRole("button", { name: "Open task" }).first()).toBeVisible();
    await page.getByRole("button", { name: "Open task" }).first().click();
    await expect(page.getByTestId("strategy-page")).toBeVisible();

    await page.locator('[data-tour-id="nav-focus"]').click();
    await expect(page.getByRole("button", { name: "Open task" }).first()).toBeVisible();
    await page.getByRole("button", { name: "Open task" }).first().click();
    await expect(page.getByTestId("strategy-page")).toBeVisible();
  });

  test("reset tab state preserves global connection settings", async ({ page }) => {
    await completeLocalOnboarding(page);

    await page.getByTestId("e2e-crash-active-tab").click();
    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.locator('[data-tour-id="nav-boards"]').click();

    await expect(page.getByRole("heading", { name: "This tab crashed." })).toBeVisible();
    await page.getByRole("button", { name: "Reset tab state" }).click();
    await expect(page.getByText("Crash Recovery")).toBeHidden();

    await page.locator('[data-tour-id="nav-config"]').click();
    await expect(page.getByText(/health:\s*up/)).toBeVisible();
    await expect(page.getByText(/ws:\s*connected/)).toBeVisible();
  });

  test("recovers from deterministic tab crash through tab boundary retry", async ({ page }) => {
    await completeLocalOnboarding(page);

    await page.getByTestId("e2e-crash-active-tab").click();
    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.locator('[data-tour-id="nav-boards"]').click();

    await expect(page.getByText("Crash Recovery")).toBeVisible();
    await expect(page.getByRole("heading", { name: "This tab crashed." })).toBeVisible();

    await page.getByRole("button", { name: "Retry" }).click();
    await expect(page.getByText("Crash Recovery")).toBeHidden();
    await expect(page.getByText("Investigate gateway health")).toBeVisible();
  });

  test("guided tour shows explicit progress and covers events and config", async ({ page }) => {
    await completeLocalOnboarding(page);

    await page.locator('[data-tour-id="topbar-tour"]').click();
    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("1/13");
    await expect(page.getByRole("heading", { name: "Boards = task execution" })).toBeVisible();

    await page.getByRole("button", { name: "Next" }).click();
    await page.getByRole("button", { name: "Next" }).click();
    await page.getByRole("button", { name: "Next" }).click();

    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("4/13");
    await expect(page.getByRole("heading", { name: "Events = runtime activity" })).toBeVisible();

    for (let index = 0; index < 6; index += 1) {
      await page.getByRole("button", { name: "Next" }).click();
    }

    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("10/13");
    await expect(page.getByRole("heading", { name: "Strategy = management layer" })).toBeVisible();

    await page.getByRole("button", { name: "Next" }).click();
    await page.getByRole("button", { name: "Next" }).click();

    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("12/13");
    await expect(
      page.getByRole("heading", { name: "Config = connection + recovery controls" })
    ).toBeVisible();
  });
});
