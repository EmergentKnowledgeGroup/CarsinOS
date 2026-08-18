import { expect, test } from "./testHarness";
import { clickAdvancedNav, completeQuickstartLocalOnboarding } from "./onboardingFlow";
import { MISSION_CONTROL_TABS } from "../src/app/tabs";
import { mkdir } from "node:fs/promises";

const VIEWPORTS = [
  { width: 375, height: 812 },
  { width: 768, height: 1024 },
  { width: 1024, height: 768 },
  { width: 1440, height: 900 },
];

test.beforeAll(async () => {
  await mkdir("../../runtime/reports/claude-design-polish/after", { recursive: true });
});

async function enableEverySurface(page: import("@playwright/test").Page) {
  await page.locator('[data-tour-id="nav-config"]').click();
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.getByText("2. Choose what pages show").click();
  for (const label of ["Memory page", "Strategy page", "Runbook page", "Connectors page", "Live Feed panel"]) {
    const checkbox = page.getByRole("checkbox", { name: label });
    if (await checkbox.count() && !(await checkbox.isChecked())) await checkbox.check();
  }
  await page.keyboard.press("Escape");
}

for (const viewport of VIEWPORTS) {
  test(`visual evidence across every surface at ${viewport.width}px @public-release`, async ({ page }) => {
    test.setTimeout(180_000);
    const browserErrors: string[] = [];
    page.on("console", (message) => {
      const text = message.text();
      const isKnownCockpitGridWarning = text.startsWith("Cannot update a component") && text.includes("CockpitPage App App");
      if (message.type() === "error" && !text.startsWith("Failed to load resource:") && !isKnownCockpitGridWarning) {
        browserErrors.push(text);
      }
    });
    page.on("pageerror", (error) => browserErrors.push(error.message));

    await page.setViewportSize(viewport);
    await completeQuickstartLocalOnboarding(page);
    await enableEverySurface(page);

    for (const item of MISSION_CONTROL_TABS) {
      if (item.tier === "advanced") await clickAdvancedNav(page, item.tab);
      else await page.locator(`[data-tour-id="nav-${item.tab}"]`).click();

      const pane = page.locator(`.mc-tab-pane[data-active-tab="${item.tab}"]`);
      await expect(pane).toBeVisible();
      await page.evaluate(() => window.scrollTo(0, 0));
      await pane.evaluate((element) => element.scrollTo(0, 0));
      await page.waitForTimeout(150);

      const rootOverflow = await page.evaluate(() =>
        document.documentElement.scrollWidth - document.documentElement.clientWidth
      );
      expect(rootOverflow, `${item.label} must not cause page-level horizontal overflow`).toBeLessThanOrEqual(1);

      await page.screenshot({
        path: `../../runtime/reports/claude-design-polish/after/${item.tab}-${viewport.width}.png`,
        fullPage: false,
      });
    }

    expect(browserErrors, "browser console and page errors").toEqual([]);
  });
}
