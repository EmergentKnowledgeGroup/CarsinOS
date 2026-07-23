import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

const FORBIDDEN_AMBIENT_TERMS = [
  "proof_hex",
  "metadata_json",
  "recipient_principal",
  "attachment",
  "tool output",
];

test("@core @glass-window renders safe Reef presence and Agent Mail chatter", async ({
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
  await page.locator('[data-tour-id="nav-window"]').click();

  const windowFloor = page.getByRole("region", { name: "The Window" });
  await expect(windowFloor).toBeVisible();
  await expect(
    windowFloor.getByRole("region", { name: "Reef presence" }),
  ).toContainText("ExecAss");
  await expect(windowFloor.getByText("Working", { exact: true })).toBeVisible();
  await expect(
    windowFloor.getByText("No recent observation", { exact: true }),
  ).toBeVisible();
  await expect(windowFloor).not.toContainText("offline");

  const chatter = windowFloor.getByRole("region", {
    name: "Office Chatter",
  });
  await expect(chatter).toContainText(
    "The launch brief moved into active work.",
  );
  for (const term of FORBIDDEN_AMBIENT_TERMS) {
    await expect(chatter).not.toContainText(term);
  }

  await chatter
    .getByLabel("Add a safe owner note")
    .fill("Keep the final review attached to this workstream.");
  await chatter.getByRole("button", { name: "Send" }).click();
  await expect(chatter).toContainText(
    "Keep the final review attached to this workstream.",
  );
  await page.screenshot({
    path: "../../runtime/qa/glass-office-p3/window-desktop.png",
    fullPage: true,
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await expect(windowFloor).toBeVisible();
  await expect(
    windowFloor.getByText("No recent observation", { exact: true }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/glass-office-p3/window-mobile.png",
    fullPage: true,
  });
  expect(browserErrors).toEqual([]);
});
