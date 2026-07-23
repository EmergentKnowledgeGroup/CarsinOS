import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * The Assistant's Desk slide-over: walking over from the ExecAss persona,
 * conversational intake logged for the sitting, decision escalation with a
 * real revise resolution against the versioned mock contract, and the plan
 * over the shoulder. Desktop and 390px, console-error and overflow checked.
 */

test("@core @assistant-desk walking over, talking it through, and revising hold at desktop and 390px", async ({
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

  // Walking over from the persona opens the desk; it adds no nav entry.
  const navItemsBefore = await page.locator('[data-tour-id^="nav-"]').count();
  await office
    .getByRole("button", { name: "Walk to the Assistant's Desk" })
    .click();
  const desk = page.getByTestId("assistant-desk");
  await expect(desk).toBeVisible();
  expect(await page.locator('[data-tour-id^="nav-"]').count()).toBe(
    navItemsBefore,
  );

  // A question is a conversation, not fake work - and it stays on the desk.
  await desk
    .getByLabel("Say something to ExecAss")
    .fill("Are the June invoices already paid?");
  await desk.getByRole("button", { name: "Send to ExecAss" }).click();
  await expect(desk).toContainText("Are the June invoices already paid?");
  await expect(desk).toContainText("already paid");
  await expect(desk).toContainText("lives on the desk for this sitting");

  // Walking back: Escape from the composer leaves the desk untouched.
  await desk.getByLabel("Say something to ExecAss").press("Escape");
  await expect(desk).not.toBeVisible();
  await expect(office).toBeVisible();

  // "Let's talk it through" escalates the decision to the desk with the
  // live context and the plan over the shoulder.
  const cards = page.getByTestId("execass-attention-card");
  await expect(cards).toHaveCount(2);
  await cards
    .first()
    .getByRole("button", { name: /talk it through/i })
    .click();
  await expect(desk).toBeVisible();
  await expect(desk).toContainText(
    "Closing the old Mailchimp account is permanent.",
  );
  await expect(desk).toContainText("Over the shoulder");
  await expect(desk).toContainText("The plan");
  // The earlier exchange survived the walk back and over again.
  await expect(desk).toContainText("already paid");

  await page.screenshot({
    path: "../../runtime/qa/glass-assistant-desk/desk-decision-desktop.png",
    fullPage: true,
  });

  // A revision resolves the decision for real: the card leaves Needs You.
  await desk
    .getByLabel("Revise this decision")
    .fill("Export the mailing list first, then close it");
  await desk.getByRole("button", { name: "Send revision" }).click();
  await expect(desk).toContainText(
    "Export the mailing list first, then close it",
  );
  await desk.getByRole("button", { name: "Back to the office" }).click();
  await expect(page.getByTestId("execass-attention-card")).toHaveCount(1);

  // Narrow width: the desk is a full-width destination, nothing overflows.
  await page.setViewportSize({ width: 390, height: 844 });
  await office
    .getByRole("button", { name: "Walk to the Assistant's Desk" })
    .click();
  await expect(desk).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/glass-assistant-desk/desk-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
