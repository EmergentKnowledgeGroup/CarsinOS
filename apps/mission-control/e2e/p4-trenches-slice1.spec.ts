import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * P4 Trenches slice 1: stable room ids own elevator navigation identity
 * (exactly one lit room even when two rooms share a product surface), the
 * Boards room can be pinned to the Office, and the pinned shortcut walks
 * back to Boards by room id. Verified at desktop and 390px with
 * console-error and horizontal-overflow assertions.
 */

test("@core @p4-trenches room identity, boards pin-to-office, and the office shortcut hold at desktop and 390px", async ({
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

  // Two rooms share the team surface; the lamp must light exactly one,
  // keyed by stable room id, not by route.
  const activeRooms = page.locator(".mc-nav-item-active");
  await page.locator('button[title="2F · Staff Directory"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Staff Directory");

  await page.locator('button[title="BF · Models & Providers"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Models & Providers",
  );

  // Elevator floor shortcuts select the floor's default room by id.
  await page.locator("body").press("2");
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Boards");

  // Boards keeps its existing toolbar (parity) and gains Pin to Office.
  await expect(page.locator(".mc-board-toolbar")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "+ New Card" }),
  ).toBeVisible();
  const pin = page.getByRole("button", { name: "Pin Boards to Office" });
  await expect(pin).toBeVisible();
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "On the Office canvas.",
  );
  await page.screenshot({
    path: "../../runtime/qa/p4-trenches-slice1/boards-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas and names its floor.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-boards");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Trenches");
  await page.screenshot({
    path: "../../runtime/qa/p4-trenches-slice1/office-shortcut-desktop.png",
    fullPage: true,
  });
  await shortcut.scrollIntoViewIfNeeded();
  await shortcut.screenshot({
    path: "../../runtime/qa/p4-trenches-slice1/office-shortcut-block.png",
  });

  // Opening the shortcut lands back in the Boards room, lamp included.
  await shortcut.getByRole("button", { name: "Open Boards" }).click();
  await expect(page.locator(".mc-board-toolbar")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Boards");

  // The pin is config: it survives a full reload. Browser runs keep the
  // gateway token in memory, so onboarding runs again after the reload.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("office-block-boards")).toBeVisible();

  // Narrow width: Boards with the pin affordance stays honest, nothing
  // overflows horizontally.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-boards"]').click();
  const mobileBoardsRoom = page.locator('button[title="2F · Boards"]');
  await expect(mobileBoardsRoom).toHaveClass(/mc-nav-item-active/);
  await expect(
    mobileBoardsRoom.locator(".mc-nav-room-mark"),
  ).toHaveText("B");
  await expect(
    page.getByRole("button", { name: "Pin Boards to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/p4-trenches-slice1/boards-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
