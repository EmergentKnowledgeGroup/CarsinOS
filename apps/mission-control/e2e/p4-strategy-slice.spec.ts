import { expect, test, type Page } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/** The Plan room is always visible; enabling the hub lights its surfaces. */
async function enableStrategyPage(page: Page): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  await page.getByText("2. Choose what pages show").click();
  await page.getByRole("checkbox", { name: "Strategy page" }).check();
  await page.keyboard.press("Escape");
}

/**
 * P4 Plan/Strategy room slice: Strategy keeps its five section surfaces and
 * draft/mutation machinery, gains the registry-backed Pin to Office
 * affordance, and the pinned shortcut walks back to the Plan room by stable
 * id — refusing honestly when the floor is disabled. Verified at desktop
 * and 390px with console-error and horizontal-overflow assertions.
 */

test("@core @p4-strategy plan parity, pin-to-office, and the office shortcut hold at desktop and 390px", async ({
  page,
}) => {
  const browserErrors: string[] = [];
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (
      message.type() === "error" &&
      !message.text().startsWith("Failed to load resource:")
    ) {
      browserErrors.push(message.text());
    }
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      browserErrors.push(`${response.status()} ${response.url()}`);
    }
  });

  await completeQuickstartLocalOnboarding(page);

  // The Plan room lights exactly one lamp by stable id once the hub is on.
  await enableStrategyPage(page);
  const activeRooms = page.locator(".mc-nav-item-active");
  await page.locator('button[title="2F · Plan"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Plan");
  await expect(page.getByTestId("strategy-page")).toBeVisible();

  // Parity: every Strategy section opens its panel.
  for (const label of [
    "Overview",
    "Goals & Projects",
    "Tasks",
    "Task Detail",
    "Insights",
  ]) {
    const tab = page.getByRole("tab", { name: label, exact: true });
    await tab.click();
    await expect(tab).toHaveAttribute("aria-selected", "true");
  }

  // Pin succeeds, and pinning again reports the truth instead of a change.
  const pin = page.getByRole("button", { name: "Pin Plan to Office" });
  await expect(pin).toBeVisible();
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "On the Office canvas.",
  );
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "Already on the Office canvas.",
  );
  await page.screenshot({
    path: "../../runtime/qa/p4-strategy-slice/strategy-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas and names its floor.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-strategy");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Trenches");
  await shortcut.scrollIntoViewIfNeeded();
  await shortcut.screenshot({
    path: "../../runtime/qa/p4-strategy-slice/office-shortcut-block.png",
  });

  // Opening the shortcut lands back in the Plan room, lamp included.
  await shortcut.getByRole("button", { name: "Open Plan" }).click();
  await expect(page.getByTestId("strategy-page")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Plan");

  // The pin is config: it survives a full reload. Browser runs keep the
  // gateway token in memory, so onboarding runs again after the reload.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("office-block-strategy")).toBeVisible();

  // If the destination floor is later disabled, the persisted door stays
  // visible but refuses honestly instead of becoming a silent dead button.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.floorOverrides = { ...config.floorOverrides, trenches: { hidden: true } };
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="2F · Plan"]')).toHaveCount(0);
  await page
    .getByTestId("office-block-strategy")
    .getByRole("button", { name: "Open Plan" })
    .click();
  await expect(
    page.getByTestId("office-block-strategy").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await page.screenshot({
    path: "../../runtime/qa/p4-strategy-slice/strategy-disabled-door.png",
    fullPage: true,
  });

  // Restore the floor for the narrow-width proof.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.trenches;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="2F · Plan"]')).toHaveCount(1);
  await expect(
    page.getByTestId("office-block-strategy").getByRole("status"),
  ).toHaveCount(0);
  await expect(
    page
      .getByTestId("office-block-strategy")
      .getByRole("button", { name: "Open Plan" }),
  ).toBeVisible();
  await page.screenshot({
    path: "../../runtime/qa/p4-strategy-slice/strategy-restored-door.png",
    fullPage: true,
  });

  // Narrow width: the Plan room keeps a readable mark, the pin stays
  // reachable, and nothing overflows horizontally.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-strategy"]').click();
  const mobilePlanRoom = page.locator('button[title="2F · Plan"]');
  await expect(mobilePlanRoom).toHaveClass(/mc-nav-item-active/);
  await expect(mobilePlanRoom.locator(".mc-nav-room-mark")).toHaveText("P");
  await expect(
    page.getByRole("button", { name: "Pin Plan to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/p4-strategy-slice/strategy-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
