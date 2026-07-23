import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * Deferred foundation UX: the Office block canvas is config (arrange mode
 * reorders/resizes/hides/pins registry blocks and survives reload), and the
 * theme studio activates glass themes onto the Office surface. Verified at
 * desktop and 390px with console-error and horizontal-overflow assertions.
 */

test("@core @glass-foundation arrange mode, pin-to-office, and the theme studio hold at desktop and 390px", async ({
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
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const office = page.getByTestId("execass-office");
  await expect(office).toBeVisible();

  // The canvas renders registry blocks in default order.
  const blockIds = () =>
    page
      .locator("[data-testid^='office-block-']")
      .evaluateAll((sections) =>
        sections.map((section) =>
          section.getAttribute("data-testid")!.replace("office-block-", ""),
        ),
      );
  expect(await blockIds()).toEqual(["needs-you", "in-motion", "done", "next"]);

  // Arrange: reorder, resize, hide, and pin back from the library.
  await office.getByRole("button", { name: "Arrange office" }).click();
  await office.getByRole("button", { name: "Move In motion earlier" }).click();
  expect(await blockIds()).toEqual(["in-motion", "needs-you", "done", "next"]);

  await office.getByRole("button", { name: "Resize Next" }).click();
  await expect(page.getByTestId("office-block-next")).toHaveClass(
    /mc-block-m/,
  );

  await office
    .getByRole("button", { name: "Hide Done since you checked" })
    .click();
  expect(await blockIds()).toEqual(["in-motion", "needs-you", "next"]);
  // Pinning a block whose placement was only hidden restores it in place.
  await office
    .getByRole("button", { name: "Pin Done since you checked to Office" })
    .click();
  expect(await blockIds()).toEqual(["in-motion", "needs-you", "done", "next"]);
  await office.getByRole("button", { name: "Arrange office" }).click();
  await expect(
    office.getByRole("button", { name: "Move In motion earlier" }),
  ).toHaveCount(0);

  // The layout is config: it survives a full reload. Browser runs keep the
  // gateway token in memory, so onboarding runs again after the reload.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(office).toBeVisible();
  expect(await blockIds()).toEqual(["in-motion", "needs-you", "done", "next"]);

  await page.screenshot({
    path: "../../runtime/qa/glass-foundation-ux/office-arranged-desktop.png",
    fullPage: true,
  });

  // Theme studio: activating Carbon lands on the Office surface as tokens.
  await page.locator('.mc-topbar-right button[title="Settings"]').click();
  await page.getByText("4. Theme studio").click();
  await expect(page.getByTestId("theme-active")).toContainText(
    "Follow system",
  );
  await page.getByRole("button", { name: "Use Carbon (After Hours)" }).click();
  await expect(page.getByTestId("theme-active")).toContainText(
    "Carbon (After Hours)",
  );
  await page.screenshot({
    path: "../../runtime/qa/glass-foundation-ux/theme-studio-desktop.png",
    fullPage: true,
  });
  await page
    .locator(".mc-settings-modal")
    .getByRole("button")
    .first()
    .press("Escape");
  await page.keyboard.press("Escape");
  await expect(office).toHaveAttribute("style", /--claw: ?#FF5A1F/i);
  await page.screenshot({
    path: "../../runtime/qa/glass-foundation-ux/office-carbon-desktop.png",
    fullPage: true,
  });

  // Narrow width: the office stacks, arrange still works, nothing overflows.
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(office).toBeVisible();
  await office.getByRole("button", { name: "Arrange office" }).click();
  await expect(
    office.getByRole("button", { name: "Move In motion later" }),
  ).toBeVisible();
  await office.getByRole("button", { name: "Arrange office" }).click();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/glass-foundation-ux/office-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
