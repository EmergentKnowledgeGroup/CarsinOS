import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

test("calendar selection is safe and responsive @core @public-release", async ({ page }) => {
  let runRequests = 0;
  await page.route("**/api/v1/mission-control/calendar/week", async (route) => {
    const response = await route.fetch();
    const payload = await response.json();
    payload.always_running = [payload.next_up[0]];
    await route.fulfill({ response, json: payload });
  });
  page.on("request", (request) => {
    if (request.method() === "POST" && /\/api\/v1\/jobs\/[^/]+\/run$/.test(request.url())) {
      runRequests += 1;
    }
  });

  await completeQuickstartLocalOnboarding(page, {
    beforeGoto: async (nextPage) => {
      await nextPage.addInitScript(() => {
        window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
      });
    },
  });

  await page.locator('[data-tour-id="nav-calendar"]').click();
  await page.getByRole("tab", { name: "Week View" }).click();
  await page.getByRole("button", { name: /Gateway heartbeat check/ }).first().click();

  await expect(page.getByRole("dialog", { name: "Gateway heartbeat check" })).toBeVisible();
  expect(runRequests).toBe(0);
  await page.waitForTimeout(500);

  await page.screenshot({
    path: "../../runtime/reports/public-release/calendar-details-desktop.png",
  });

  await page.getByRole("button", { name: "Run job now" }).click();
  await expect.poll(() => runRequests).toBe(1);

  await page.setViewportSize({ width: 375, height: 812 });
  await page.locator('[data-tour-id="nav-calendar"]').click();
  await page.getByRole("tab", { name: "Week View" }).click();
  await page.getByRole("button", { name: /Gateway heartbeat check/ }).first().click();
  await expect(page.getByRole("dialog", { name: "Gateway heartbeat check" })).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)
  ).toBe(true);
  await page.waitForTimeout(500);
  await page.screenshot({
    path: "../../runtime/reports/public-release/calendar-details-mobile.png",
  });
});
