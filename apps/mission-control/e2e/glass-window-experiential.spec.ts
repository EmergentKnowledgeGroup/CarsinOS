import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * P3 experiential: crab report cards with honest freshness, authoritative
 * deep links (run -> run history), reduced-motion compliance, and the
 * narrow-width reef summary. Desktop and 390px with console-error and
 * horizontal-overflow assertions.
 */

test("@core @glass-window-experiential report cards, deep links, and reduced motion hold at desktop and 390px", async ({
  page,
}) => {
  const browserErrors: string[] = [];
  const requestedRunbookDetails: string[] = [];
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
  page.on("request", (request) => {
    if (request.url().includes("/api/v1/mission-control/runbooks/")) {
      requestedRunbookDetails.push(request.url());
    }
  });

  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-window"]').click();
  const windowFloor = page.getByRole("region", { name: "The Window" });
  await expect(windowFloor).toBeVisible();

  // The distinct big crab leads the reef.
  const crabs = page.getByTestId("reef-crab");
  await expect(crabs.first()).toContainText("ExecAss");
  await expect(crabs.first()).toHaveClass(/is-execass/);

  // Report card: honest observation, then the authoritative deep link.
  await crabs.first().click();
  const card = page.getByTestId("reef-report-card");
  await expect(card).toBeVisible();
  await expect(card).toContainText("Working");
  await expect(card).toContainText("Observed just now");
  await expect(card).toContainText("mood: focused");

  // The unknown crab stays honestly unknown with nothing to open.
  await page.getByRole("button", { name: "Reef's report card" }).click();
  await expect(card).toContainText("No recent observation");
  await expect(card).toContainText("Nothing to open for this crab.");

  // Opening moves focus into the card; immediate Escape closes it and hands
  // focus back to the crab without requiring a Tab first.
  await expect(card).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(card).not.toBeVisible();
  await expect(
    page.getByRole("button", { name: "Reef's report card" }),
  ).toBeFocused();

  await page.screenshot({
    path: "../../runtime/qa/glass-p3-experiential/reef-desktop.png",
    fullPage: true,
  });

  // Reduced motion: the busy crab's scuttle is disabled.
  await page.emulateMedia({ reducedMotion: "reduce" });
  const animation = await page
    .locator(".mc-reef-agent[data-activity='busy'] .mc-reef-crab")
    .first()
    .evaluate((element) => getComputedStyle(element).animationName);
  expect(animation).toBe("none");
  await page.emulateMedia({ reducedMotion: null });

  // With the run history room switched off, the link refuses honestly.
  await page.getByRole("button", { name: "ExecAss's report card" }).click();
  await page.getByRole("button", { name: "Open the run history" }).click();
  await expect(card).toContainText("switched off in Config");

  // Once the room is on, the deep link lands on the authoritative surface.
  await page.locator('[data-tour-id="nav-config"]').click();
  await page.getByText("2. Choose what pages show").click();
  await page.getByRole("checkbox", { name: "Runbook page" }).check();
  await page.keyboard.press("Escape");
  // The card is still open from the refused attempt; the link now works.
  await page.getByRole("button", { name: "Open the run history" }).click();
  await expect(page.getByTestId("runbook-page")).toBeVisible();
  await expect(
    page.getByTestId("runbook-page").getByRole("button", {
      name: "Back to list",
    }),
  ).toBeVisible();
  expect(
    requestedRunbookDetails.some((url) =>
      url.endsWith("/assistant_session_run/run-assistant-001"),
    ),
  ).toBe(true);

  // Narrow width: the reef summarizes before it expands; nothing overflows.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-window"]').click();
  await expect(windowFloor).toBeVisible();
  const summary = windowFloor.locator(".mc-reef-collapse summary");
  await expect(summary).toContainText("on the floor");
  await expect(page.getByTestId("reef-crab").first()).not.toBeVisible();
  await summary.click();
  await expect(page.getByTestId("reef-crab").first()).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/glass-p3-experiential/window-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
