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

  test("connects via deterministic stub gateway and loads baseline", async ({ page }) => {
    await completeLocalOnboarding(page);

    await expect(page.getByText("Investigate gateway health")).toBeVisible();

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
});
