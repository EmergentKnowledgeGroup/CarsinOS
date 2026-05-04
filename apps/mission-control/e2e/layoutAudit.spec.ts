import { test, expect } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";
import { MISSION_CONTROL_TABS } from "../src/app/tabs";

test("audit desktop tab overflow", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await completeQuickstartLocalOnboarding(page);

  await page.locator('[data-tour-id="nav-config"]').click();
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  for (const label of [
    "Memory page",
    "Strategy page",
    "Runbook page",
    "Connectors page",
    "Live Feed panel",
  ]) {
    const checkbox = page.getByRole("checkbox", { name: label });
    if (await checkbox.count()) {
      if (!(await checkbox.isChecked())) {
        await checkbox.check();
      }
    }
  }
  await page.keyboard.press("Escape");

  const results: Array<Record<string, unknown>> = [];
  for (const item of MISSION_CONTROL_TABS) {
    await page.locator(`[data-tour-id="nav-${item.tab}"]`).click();
    await page.waitForTimeout(250);
    const metrics = await page.locator(".mc-content-area").evaluate((node) => {
      const area = node as HTMLElement;
      const root = document.documentElement;
      const activePane = Array.from(area.querySelectorAll<HTMLElement>(".mc-tab-pane")).find(
        (pane) => window.getComputedStyle(pane).display !== "none"
      );
      const paneChildren = activePane ? Array.from(activePane.children) : [];
      return {
        areaDelta: area.scrollHeight - area.clientHeight,
        areaClientHeight: area.clientHeight,
        areaScrollHeight: area.scrollHeight,
        viewportDelta: root.scrollHeight - root.clientHeight,
        paneDelta: activePane ? activePane.scrollHeight - activePane.clientHeight : null,
        paneChildSummary: paneChildren.map((child) => {
          const el = child as HTMLElement;
          return {
            className: el.className,
            clientHeight: el.clientHeight,
            scrollHeight: el.scrollHeight,
            delta: el.scrollHeight - el.clientHeight,
          };
        }),
      };
    });
    results.push({ tab: item.tab, label: item.label, ...metrics });
  }

  console.log(JSON.stringify(results, null, 2));
});
