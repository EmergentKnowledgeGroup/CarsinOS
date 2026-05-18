import { mkdirSync } from "node:fs";
import path from "node:path";
import { expect, test, type Page } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

const FORBIDDEN_CARD_TERMS = [
  "runbook",
  "bridge",
  "PID",
  "canonical session",
  "source_refs",
  "routing config",
];

async function captureAssistantDeskScreenshots(page: Page) {
  if (process.env.CARSINOS_ASSISTANT_DESK_SCREENSHOTS !== "1") {
    return;
  }

  const reportDir =
    process.env.CARSINOS_ASSISTANT_DESK_REPORT_DIR?.trim() ||
    path.resolve(process.cwd(), "..", "..", "reports", "assistant-desk", "after");
  mkdirSync(reportDir, { recursive: true });
  while ((await page.locator(".mc-toast-dismiss").count()) > 0) {
    await page.locator(".mc-toast-dismiss").first().click();
  }

  for (const width of [375, 768, 1024, 1440]) {
    await page.setViewportSize({ width, height: width < 768 ? 860 : 800 });
    await expect(page.getByRole("region", { name: "Assistant Desk", exact: true })).toBeVisible();
    await page.screenshot({
      path: path.join(reportDir, `assistant-desk-${width}.png`),
      fullPage: true,
    });
  }
}

test.describe("Assistant Desk @assistant-desk", () => {
  test("keeps ExecAss work visible without adding another menu maze", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await completeQuickstartLocalOnboarding(page);

    const navItemsBefore = await page.locator('[data-tour-id^="nav-"]').count();

    await page.locator('[data-tour-id="nav-assistant"]').click();

    const statusStrip = page.getByRole("region", { name: "Assistant Desk status" });
    await expect(statusStrip).toBeVisible();
    await expect(statusStrip.getByRole("button", { name: /Needs you/ })).toBeVisible();
    await expect(statusStrip.getByRole("button", { name: /\+2 more/ })).toBeVisible();
    await expect(statusStrip.locator(".mc-assistant-desk-chip")).toHaveCount(5);

    await statusStrip.getByRole("button", { name: "Open Desk" }).click();
    const desk = page.getByRole("region", { name: "Assistant Desk", exact: true });
    await expect(desk).toBeVisible();
    await expect(desk.getByRole("heading", { name: "What ExecAss is juggling" })).toBeVisible();
    await expect(desk.getByRole("heading", { name: "Needs you" })).toBeVisible();
    await expect(desk.getByRole("heading", { name: "Working" })).toBeVisible();
    await expect(desk.getByRole("heading", { name: "Done recently" })).toBeVisible();
    await expect(desk.locator(".mc-assistant-desk-bucket")).toHaveCount(3);
    const deskBox = await desk.boundingBox();
    expect(deskBox?.y ?? Number.POSITIVE_INFINITY).toBeLessThan(520);

    const cardText = await desk.locator(".mc-assistant-desk-card").allTextContents();
    const joinedCardText = cardText.join(" ");
    for (const term of FORBIDDEN_CARD_TERMS) {
      expect(joinedCardText).not.toContain(term);
    }

    await expect(desk.locator(".mc-assistant-desk-details")).toHaveCount(0);
    await desk.getByRole("button", { name: "Details" }).first().click();
    await expect(desk.locator(".mc-assistant-desk-details")).toHaveCount(1);

    await desk.getByRole("button", { name: /Transcript/ }).first().click();
    const transcript = page.getByRole("dialog", { name: /transcript/ });
    await expect(transcript).toBeVisible();
    await expect(transcript.getByText("this item")).toBeVisible();
    await expect(transcript).not.toContainText("**this item**");

    await page.keyboard.press("Escape");
    await expect(transcript).toBeHidden();

    const navItemsAfter = await page.locator('[data-tour-id^="nav-"]').count();
    expect(navItemsAfter).toBe(navItemsBefore);

    await captureAssistantDeskScreenshots(page);
  });
});
