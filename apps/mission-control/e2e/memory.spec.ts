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

test.describe("mission-control memory lane integration", () => {
  test("keeps assistant memory isolated and exposes explainability drilldown", async ({
    page,
  }) => {
    await completeLocalOnboarding(page);

    await expect(page.locator('[data-tour-id="nav-memory"]')).toHaveCount(0);

    await page.locator('[data-tour-id="nav-config"]').click();
    await page.getByRole("checkbox", { name: "Memory hub module" }).check();
    await page.keyboard.press("Escape");

    await expect(page.locator('[data-tour-id="nav-memory"]')).toBeVisible();
    await page.locator('[data-tour-id="nav-memory"]').click();
    await expect(page.getByTestId("memory-page")).toBeVisible();
    await expect(
      page.getByText("This assistant does not have an MNO lane bound yet.")
    ).toBeVisible();

    await page.getByTestId("memory-agent-select").selectOption("lyra");
    await expect(page.getByText("lane: available")).toBeVisible();
    const lyraPreferenceCard = page
      .locator(".mc-memory-list-item", {
        hasText: "Lyra prefers a narrow, operator-readable incident summary.",
      })
      .first();
    await expect(lyraPreferenceCard).toBeVisible();

    const heartbeatDriftCard = page
      .locator(".mc-memory-list-item", {
        hasText: "Lyra resolved the gateway heartbeat drift during the last reliability pass.",
      })
      .first();
    await heartbeatDriftCard.click();

    await expect(page.getByText("turn-lyra-001")).toBeVisible();
    const citationPill = page.getByRole("button", { name: "runtime card excerpt", exact: true });
    await expect(citationPill).toBeVisible();
    await citationPill.click();
    await expect(
      page.getByRole("heading", { name: "Citation drilldown" })
    ).toBeVisible();

    await page.getByRole("button", { name: /Open Assistant/i }).click();
    await expect(page.getByLabel("Assistant agent")).toHaveValue("lyra");

    await page.locator('[data-tour-id="nav-memory"]').click();
    await page.getByTestId("memory-agent-select").selectOption("agent-root");
    await expect(
      page.getByText("This assistant does not have an MNO lane bound yet.")
    ).toBeVisible();
    await expect(page.locator("body")).not.toContainText(
      "Lyra resolved the gateway heartbeat drift during the last reliability pass."
    );
  });
});
