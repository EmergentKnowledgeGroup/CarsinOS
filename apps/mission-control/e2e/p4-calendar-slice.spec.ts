import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * P4 Calendar room slice: Calendar keeps its Week View/Schedule/Active Jobs
 * surfaces and ExecAss heartbeat panel, gains the registry-backed Pin to
 * Office affordance, and the pinned shortcut walks back to the Calendar
 * room by stable id. Verified at desktop and 390px with console-error and
 * horizontal-overflow assertions.
 */

test("@core @p4-calendar calendar parity, pin-to-office, and the office shortcut hold at desktop and 390px", async ({
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

  // The Calendar room lights exactly one lamp by stable id.
  const activeRooms = page.locator(".mc-nav-item-active");
  await page.locator('button[title="2F · Calendar"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Calendar");

  // Parity: heartbeat panel and all three Calendar tabs stay present.
  await expect(
    page.locator('[aria-label="ExecAss heartbeat setup"]'),
  ).toBeVisible();
  await expect(page.getByRole("tab", { name: /Week View/ })).toBeVisible();
  const scheduleTab = page.getByRole("tab", { name: /Schedule/ });
  const activeJobsTab = page.getByRole("tab", { name: /Active Jobs/ });
  await scheduleTab.click();
  await expect(scheduleTab).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".mc-table")).toBeVisible();
  await activeJobsTab.click();
  await expect(activeJobsTab).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".mc-cal-active")).toBeVisible();
  await page.getByRole("tab", { name: /Week View/ }).click();

  // Pin succeeds, and pinning again reports the truth instead of a change.
  const pin = page.getByRole("button", { name: "Pin Calendar to Office" });
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
    path: "../../runtime/qa/p4-calendar-slice/calendar-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas and names its floor.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-calendar");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Trenches");
  await shortcut.scrollIntoViewIfNeeded();
  await shortcut.screenshot({
    path: "../../runtime/qa/p4-calendar-slice/office-shortcut-block.png",
  });

  // Opening the shortcut lands back in the Calendar room, lamp included.
  await shortcut.getByRole("button", { name: "Open Calendar" }).click();
  await expect(page.locator(".mc-calendar-page")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Calendar");

  // The pin is config: it survives a full reload. Browser runs keep the
  // gateway token in memory, so onboarding runs again after the reload.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("office-block-calendar")).toBeVisible();

  // If the destination floor is later disabled, the persisted door stays
  // visible but refuses honestly instead of becoming a silent dead button.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.floorOverrides = { ...config.floorOverrides, trenches: { hidden: true } };
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="2F · Calendar"]')).toHaveCount(0);
  await page
    .getByTestId("office-block-calendar")
    .getByRole("button", { name: "Open Calendar" })
    .click();
  await expect(
    page.getByTestId("office-block-calendar").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await page.screenshot({
    path: "../../runtime/qa/p4-calendar-slice/disabled-calendar-shortcut-desktop.png",
    fullPage: true,
  });

  // Restore the floor for the narrow-width Calendar proof.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.trenches;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="2F · Calendar"]')).toHaveCount(1);

  // Narrow width: the Calendar room keeps a readable mark, the pin stays
  // reachable, and nothing overflows horizontally.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-calendar"]').click();
  const mobileCalendarRoom = page.locator('button[title="2F · Calendar"]');
  await expect(mobileCalendarRoom).toHaveClass(/mc-nav-item-active/);
  await expect(
    mobileCalendarRoom.locator(".mc-nav-room-mark"),
  ).toHaveText("C");
  await expect(
    page.getByRole("button", { name: "Pin Calendar to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/p4-calendar-slice/calendar-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
