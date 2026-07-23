import { expect, test, type Page } from "./testHarness";
import { clickAdvancedNav, completeQuickstartLocalOnboarding } from "./onboardingFlow";

async function enableRunbook(page: Page, options?: { strategy?: boolean }): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  await page.getByText("2. Choose what pages show").click();
  if (options?.strategy) {
    await page.getByRole("checkbox", { name: "Strategy page" }).check();
  }
  await page.getByRole("checkbox", { name: "Runbook page" }).check();
  await page.keyboard.press("Escape");
}

test.describe("mission-control runbook @core", () => {
  test("hides Runbook until enabled and renders the shared runbook surface", async ({
    page,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });

    await expect(page.locator('[data-tour-id="nav-runbook"]')).toHaveCount(0);

    await enableRunbook(page);

    await clickAdvancedNav(page, "runbook");
    await expect(page.getByTestId("runbook-page")).toBeVisible();
    const approvalRunbookCard = page.getByRole("button", {
      name: /Approval gate for incident recovery session/i,
    });
    await expect(approvalRunbookCard).toBeVisible();
    await approvalRunbookCard.click();

    const runbookMetrics = await page.locator(".mc-content-area").evaluate((node) => {
      const area = node as HTMLElement;
      return {
        scrollDelta: area.scrollHeight - area.clientHeight,
      };
    });
    expect(runbookMetrics.scrollDelta).toBeLessThanOrEqual(10);

    await page.getByRole("button", { name: "Review approval" }).click();
    await expect(page.locator('[data-tour-id="nav-focus"]')).toHaveClass(/mc-nav-item-active/);
    await expect(page.getByText(/Approval requested:/).first()).toBeVisible();
  });

  test("adds runbook entry points across boards, calendar, and focus without replacing those tabs", async ({
    page,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });
    await enableRunbook(page, { strategy: true });
    await clickAdvancedNav(page, "runbook");
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
    await expect(page.getByRole("button", { name: "Back to list" })).toBeVisible();

    await page.getByRole("button", { name: "Open board card" }).click();
    await expect(page.locator('[data-tour-id="nav-boards"]')).toHaveClass(/mc-nav-item-active/);

    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.getByRole("tab", { name: /Schedule/ }).click();
    const calendarRow = page.locator(".mc-table tbody tr", {
      hasText: "Gateway heartbeat check",
    });
    await expect(calendarRow.locator(".mc-cal-runbook-panel")).toBeVisible();
    await expect(
      calendarRow.getByRole("button", { name: "Open in Runbook" })
    ).toBeVisible();

    await page.locator('[data-tour-id="nav-focus"]').click();
    const focusItem = page.locator(".mc-focus-item", {
      hasText: "Approval requested:",
    });
    await expect(focusItem.locator(".mc-focus-runbook-panel")).toBeVisible();
    await expect(
      focusItem.getByRole("button", { name: "Open Runbook" })
    ).toBeVisible();
  });
});
