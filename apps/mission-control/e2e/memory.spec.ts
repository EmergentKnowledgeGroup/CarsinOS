import { expect, test } from "./testHarness";
import { clickAdvancedNav, completeQuickstartLocalOnboarding } from "./onboardingFlow";

test.describe("mission-control memory lane integration", () => {
  test("keeps assistant memory isolated and exposes explainability drilldown", async ({
    page,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });

    await expect(page.locator('[data-tour-id="nav-memory"]')).toBeHidden();

    await page.locator('[data-tour-id="nav-config"]').click();
    await page.getByText("2. Choose what pages show").click();
    await page.getByRole("checkbox", { name: "Memory page" }).check();
    await page.keyboard.press("Escape");

    await clickAdvancedNav(page, "memory");
    await expect(page.getByTestId("memory-page")).toBeVisible();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByTestId("memory-agent-select").selectOption("agent-root");
    await expect(
      page.getByText("This agent doesn't have memory set up yet.")
    ).toBeVisible();

    await page.getByTestId("memory-agent-select").selectOption("default");
    await expect(page.getByText("lane: available")).toBeVisible();
    const localAssistantPreferenceCard = page
      .locator(".mc-memory-list-item", {
        hasText: "Local Assistant prefers a narrow, operator-readable incident summary.",
      })
      .first();
    await expect(localAssistantPreferenceCard).toBeVisible();

    const heartbeatDriftCard = page
      .locator(".mc-memory-list-item", {
        hasText:
          "Local Assistant resolved the gateway heartbeat drift during the last reliability pass.",
      })
      .first();
    await heartbeatDriftCard.click();

    await page.getByRole("button", { name: "Reasoning", exact: true }).click();
    await expect(page.getByText("turn-default-001")).toBeVisible();
    const citationPill = page.getByRole("button", { name: "runtime card excerpt", exact: true });
    await expect(citationPill).toBeVisible();
    await citationPill.click();
    await expect(
      page.getByRole("heading", { name: "Citation drilldown" })
    ).toBeVisible();

    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByRole("button", { name: /Open Assistant/i }).click();
    await expect(page.locator('[data-tour-id="assistant-page"]')).toBeVisible();

    await clickAdvancedNav(page, "memory");
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByTestId("memory-agent-select").selectOption("agent-root");
    await expect(
      page.getByText("This agent doesn't have memory set up yet.")
    ).toBeVisible();
    await expect(page.locator("body")).not.toContainText(
      "Local Assistant resolved the gateway heartbeat drift during the last reliability pass."
    );
  });
});
