import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

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
    await page.getByRole("checkbox", { name: "Memory page" }).check();
    await page.keyboard.press("Escape");

    await expect(page.locator('[data-tour-id="nav-memory"]')).toBeVisible();
    await page.locator('[data-tour-id="nav-memory"]').click();
    await expect(page.getByTestId("memory-page")).toBeVisible();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await expect(
      page.getByText("This agent doesn't have memory set up yet.")
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

    await page.getByRole("button", { name: "Reasoning", exact: true }).click();
    await expect(page.getByText("turn-lyra-001")).toBeVisible();
    const citationPill = page.getByRole("button", { name: "runtime card excerpt", exact: true });
    await expect(citationPill).toBeVisible();
    await citationPill.click();
    await expect(
      page.getByRole("heading", { name: "Citation drilldown" })
    ).toBeVisible();

    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByRole("button", { name: /Open Assistant/i }).click();
    await expect(page.locator('[data-tour-id="assistant-page"]')).toBeVisible();

    await page.locator('[data-tour-id="nav-memory"]').click();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByTestId("memory-agent-select").selectOption("agent-root");
    await expect(
      page.getByText("This agent doesn't have memory set up yet.")
    ).toBeVisible();
    await expect(page.locator("body")).not.toContainText(
      "Lyra resolved the gateway heartbeat drift during the last reliability pass."
    );
  });
});
