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

async function enableConnectors(page: Page): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  await page.getByRole("checkbox", { name: "Connectors hub module" }).check();
  await page.keyboard.press("Escape");
}

test.describe("mission-control connectors registry", () => {
  test("imports, converts, publishes, assigns, and authenticates a connector", async ({
    page,
  }) => {
    await completeLocalOnboarding(page);

    await expect(page.locator('[data-tour-id="nav-connectors"]')).toHaveCount(0);

    await enableConnectors(page);

    await expect(page.locator('[data-tour-id="nav-connectors"]')).toBeVisible();
    await page.locator('[data-tour-id="nav-connectors"]').click();
    await expect(page.getByTestId("connectors-page")).toBeVisible();

    await page.getByLabel("Display name").fill("GitHub Ops");
    await page.getByLabel("Slug").fill("github-ops");
    await page.getByLabel("Source JSON").fill(
      JSON.stringify({
        openapi: "3.1.0",
        info: {
          title: "GitHub Ops",
        },
        paths: {
          "/issues": {
            get: {
              operationId: "listIssues",
              summary: "List issues",
            },
            post: {
              operationId: "createIssue",
              summary: "Create issue",
            },
          },
        },
      })
    );

    await page.getByTestId("connectors-import-submit").click();
    await expect(page.getByRole("heading", { name: "GitHub Ops" })).toBeVisible();

    await page.getByTestId("connectors-convert-submit").click();
    await expect(page.getByText("List issues")).toBeVisible();
    await expect(page.getByText("Create issue").first()).toBeVisible();

    await page.getByRole("checkbox", { name: "Enable immediately after publish" }).check();
    await page.getByTestId("connectors-publish-submit").click();
    await expect(page.getByRole("heading", { name: "Publish connector tools" })).toBeVisible();
    await page.getByRole("button", { name: "Publish", exact: true }).click();

    await expect(
      page.locator(".mc-connectors-tool-row", { hasText: "List issues" }).first()
    ).toBeVisible();
    await expect(
      page.locator(".mc-connectors-tool-row", { hasText: "Create issue" }).first()
    ).toBeVisible();

    await page.getByTestId("connectors-assignment-agent").selectOption("lyra");
    await page.getByTestId("connectors-assignment-submit").click();
    await expect(page.locator(".mc-connectors-list-row", { hasText: "Lyra" }).first()).toBeVisible();

    await page.getByTestId("connectors-auth-secret-ref").fill("secrets/connectors/github-ops");
    await page.getByTestId("connectors-auth-binding-submit").click();
    await expect(
      page
        .locator(".mc-connectors-list-row", { hasText: "secrets/connectors/github-ops" })
        .first()
    ).toBeVisible();

    await page.getByTestId("connectors-health-refresh").click();
    await expect(page.getByText("auth required")).toHaveCount(0);

    const resumeButtons = page.getByRole("button", { name: "Resume" });
    if ((await resumeButtons.count()) > 0) {
      await resumeButtons.first().click();
      await expect(page.getByText("resumed")).toBeVisible();
    }

    await page
      .locator(".mc-connectors-tool-button", { hasText: "List issues" })
      .first()
      .click();
    await expect(
      page.locator(".mc-connectors-json-panel .chip", {
        hasText: "connector.github-ops.listissues",
      })
    ).toBeVisible();
  });
});
