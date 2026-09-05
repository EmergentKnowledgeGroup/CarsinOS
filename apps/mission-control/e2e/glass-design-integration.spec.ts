import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding, openWizard } from "./onboardingFlow";

test("@core Glass Window composer and topbar remain reachable on short screens", async ({ page }) => {
  await completeQuickstartLocalOnboarding(page);
  await page.getByRole("button", { name: "Force crash active tab" }).evaluateAll(
    (buttons) => buttons.forEach((button) => { button.style.display = "none"; }),
  );
  await page.locator('[data-tour-id="nav-window"]').click();
  while (await page.locator(".mc-toast-dismiss").count()) await page.locator(".mc-toast-dismiss").first().click();
  for (const width of [800, 1000, 390, 1600]) {
    await page.setViewportSize({ width, height: 650 });
    await page.locator(".mc-window-floor details").evaluateAll((items) => items.forEach((item) => { (item as HTMLDetailsElement).open = true; }));
    const composer = page.locator(".mc-chatter-compose input");
    await page.locator(".mc-window-floor").evaluate((el) => { el.scrollTop = 0; });
    await page.locator(".mc-window-floor").hover({ position: { x: 4, y: 4 } });
    await page.mouse.wheel(0, 100);
    await expect.poll(() => page.locator(".mc-window-floor").evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
    await composer.scrollIntoViewIfNeeded();
    await expect(composer).toBeInViewport();
    await composer.click({ trial: true });
    for (const button of await page.locator(".mc-topbar button:visible").all()) {
      const bounds = await button.boundingBox();
      expect(bounds!.x).toBeGreaterThanOrEqual(0);
      expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
    }
    await page.screenshot({ path: `../../runtime/qa/glass-design/review-window-${width}.png` });
  }
});

test("@core Glass Office design stays usable across floors and at 390px", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  await page.addInitScript(() => localStorage.setItem("mc-theme-name", "phosphor"));
  await completeQuickstartLocalOnboarding(page);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "obsidian-dark");
  // The harness-only crash sentinel is not application chrome.
  await page.getByRole("button", { name: "Force crash active tab" }).evaluateAll(
    (buttons) => buttons.forEach((button) => { button.style.display = "none"; }),
  );
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("execass-office")).toBeVisible();
  await expect(page.locator(".mc-nav-brand")).toContainText("CarsinOS");
  await expect(page.locator(".mc-office-workspace")).not.toHaveAttribute("open");
  await expect(page.getByTestId("office-block-needs-you")).toBeVisible();
  await page.locator(".mc-tab-help-dismiss:visible").first().click();
  while (await page.locator(".mc-toast-dismiss").count()) {
    await page.locator(".mc-toast-dismiss").first().click();
  }
  await expect(page.locator(".mc-glass-shell")).toHaveAttribute("data-theme", "dark");
  await page.getByRole("button", { name: "Toggle Glass Office after-hours theme" }).click();
  await expect(page.locator(".mc-glass-shell")).toHaveAttribute("data-theme", "light");
  await expect(page.getByTestId("execass-office")).toHaveAttribute("data-theme", "light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", /-light$/);
  await page.screenshot({ path: "../../runtime/qa/glass-design/office-light.png" });
  await page.getByRole("button", { name: "Toggle Glass Office after-hours theme" }).click();
  await expect(page.locator(".mc-glass-shell")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({ path: "../../runtime/qa/glass-design/office-desktop.png" });
  await page.getByText("Assistant chat & shared instructions", { exact: true }).click();
  await expect(page.getByLabel("Assistant provider")).toBeVisible();
  await page.getByText("Assistant chat & shared instructions", { exact: true }).click();

  for (const [route, image] of [["window", "window"], ["boards", "trenches"], ["calendar", "calendar"], ["team", "staff"], ["connectors", "basement"]]) {
    await page.locator(`[data-tour-id="nav-${route}"]`).click();
    await expect(page.locator(`[data-tour-id="nav-${route}"]`)).toHaveAttribute("aria-current", "page");
    await page.screenshot({ path: `../../runtime/qa/glass-design/${image}-desktop.png` });
    if (route === "window") {
      for (const width of [720, 1000]) {
        await page.setViewportSize({ width, height: 900 });
        for (const selector of [".mc-reef-panel", ".mc-chatter-panel", ".mc-chatter-compose"]) {
          const bounds = await page.locator(selector).boundingBox();
          expect(bounds).not.toBeNull();
          expect(bounds!.x).toBeGreaterThanOrEqual(0);
          expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
        }
      }
      await page.setViewportSize({ width: 1600, height: 1000 });
    }
  }
  await page.getByTitle("BF · Policy", { exact: true }).click();
  await page.screenshot({ path: "../../runtime/qa/glass-design/policy-desktop.png" });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("office-block-needs-you")).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: "../../runtime/qa/glass-design/office-mobile.png", fullPage: true });
  await page.locator('[data-tour-id="nav-config"]').click();
  await expect(page.getByRole("button", { name: "Open setup wizard" })).toBeVisible();
  await page.keyboard.press("Escape");
  expect(errors).toEqual([]);
});


test("Glass Office first run presents onboarding", async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  expect(await openWizard(page)).toBe(true);
  await page.getByRole("button", { name: "Force crash active tab" }).evaluateAll(
    (buttons) => buttons.forEach((button) => { button.style.display = "none"; }),
  );
  await page.screenshot({ path: "../../runtime/qa/glass-design/onboarding.png" });
});
