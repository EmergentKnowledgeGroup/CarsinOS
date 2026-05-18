import { expect, test, type Page } from "./testHarness";
import { clickAdvancedNav, completeQuickstartLocalOnboarding } from "./onboardingFlow";

async function enableConnectors(page: Page): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  await page.getByText("2. Choose what pages show").click();
  await page.getByRole("checkbox", { name: "Connectors page" }).check();
  await page.keyboard.press("Escape");
}

test.describe("mission-control connectors registry", () => {
  test("quick setup connects Discord without dropping the user into expert registry work", async ({
    page,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });

    await enableConnectors(page);
    await clickAdvancedNav(page, "connectors");

    await page
      .locator(".mc-connectors-quick-card", { hasText: "Discord" })
      .first()
      .click();

    await expect(
      page.getByRole("dialog", { name: "Simple Integration Setup" })
    ).toBeVisible();
    await page.getByLabel("Discord bot token").fill("discord-test-token");
    await page.getByLabel("Agent that should answer").selectOption("default");
    await page.getByRole("button", { name: "Save + check connection" }).click();

    await expect(
      page.getByRole("heading", { name: /Discord: Connected and waiting for first message/i })
    ).toBeVisible();
    await expect(
      page
        .getByRole("dialog", { name: "Simple Integration Setup" })
        .getByText(/Discord is connected and listening/i)
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Go to Assistant" })).toBeVisible();
  });

  test("imports, converts, publishes, assigns, and authenticates a connector", async ({
    page,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });

    await expect(page.locator('[data-tour-id="nav-connectors"]')).toHaveCount(0);

    await enableConnectors(page);

    await clickAdvancedNav(page, "connectors");
    await expect(page.getByTestId("connectors-page")).toBeVisible();
    await page.getByRole("button", { name: /^Import$/ }).click();

    const importPanel = page.locator("article", {
      has: page.getByRole("heading", { name: "Import connector" }),
    });

    await importPanel.getByLabel("Display name").fill("GitHub Ops");
    await importPanel.getByLabel("Slug").fill("github-ops");
    await importPanel.getByLabel("Source JSON").fill(
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

    await page.getByRole("button", { name: "Health" }).click();
    await expect(
      page.locator(".mc-connectors-tool-row", { hasText: "List issues" }).first()
    ).toBeVisible();
    await expect(
      page.locator(".mc-connectors-tool-row", { hasText: "Create issue" }).first()
    ).toBeVisible();

    await page.getByRole("button", { name: "Auth" }).click();
    await page.getByTestId("connectors-assignment-agent").selectOption("default");
    await page.getByTestId("connectors-assignment-submit").click();
    await expect(
      page.locator(".mc-connectors-list-row", { hasText: "Local Assistant" }).first()
    ).toBeVisible();

    await page.getByTestId("connectors-auth-secret-ref").fill("secrets/connectors/github-ops");
    await page.getByTestId("connectors-auth-binding-submit").click();
    await expect(
      page
        .locator(".mc-connectors-list-row", { hasText: "secrets/connectors/github-ops" })
        .first()
    ).toBeVisible();

    await page.getByRole("button", { name: "Health" }).click();
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
