import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * ExecAss Office scenario against the versioned v1.1 mock contract:
 * projection rendering, ordinary no-prompt work, external wait, honest
 * partial completion, one-confirmation continuation (work continues, the
 * card leaves, no re-prompt), clarification quick answer, signed intake
 * (delegation + conversational), receipt proof inspection, freeze/resume,
 * and the no-new-nav rule.
 */

const FORBIDDEN_CARD_TERMS = [
  "runbook",
  "bridge",
  "PID",
  "canonical session",
  "source_refs",
  "routing config",
  "proof_hex",
  "digest",
];

test("@core @execass-office the Office renders the projection and completes decision continuation", async ({
  page,
}) => {
  await completeQuickstartLocalOnboarding(page);

  const navItemsBefore = await page.locator('[data-tour-id^="nav-"]').count();
  await expect(page.getByTitle("3F · Office chatter")).toBeVisible();
  await page.locator('[data-tour-id="nav-assistant"]').click();

  const office = page.getByTestId("execass-office");
  await expect(office).toBeVisible();

  // Briefing leads with what needs the boss.
  await expect(office.locator(".mc-execass-briefing h2")).toContainText("2");

  // Needs You renders both typed attention items in plain language.
  const cards = page.getByTestId("execass-attention-card");
  await expect(cards).toHaveCount(2);
  await expect(cards.first()).toContainText("permanent");
  await expect(cards.first()).toContainText("ExecAss recommends");

  // No internal machinery leaks into the cards.
  const cardCount = await cards.count();
  for (let index = 0; index < cardCount; index += 1) {
    for (const term of FORBIDDEN_CARD_TERMS) {
      await expect(cards.nth(index)).not.toContainText(term);
    }
  }

  // Ordinary work runs without prompts; external wait is not on the boss.
  const motionRows = page.getByTestId("execass-motion-row");
  await expect(motionRows).toHaveCount(2);
  await expect(office.locator(".mc-execass-phase.is-ext")).toBeVisible();

  // Honest partial completion is visible, not sugar-coated.
  await expect(office.getByText(/96% done/).first()).toBeVisible();

  // One confirmation: the dangerous action states its consequence and
  // continues after a single yes.
  await cards.first().getByRole("button", { name: /yep/i }).click();
  await expect(page.getByTestId("execass-attention-card")).toHaveCount(1);
  await expect(office.getByText("Confirmed - closing the account now").first()).toBeVisible();
  // The confirmation never re-appears for the unchanged action.
  await expect(office.getByText("Closing the old Mailchimp account is permanent.")).toHaveCount(0);

  // Clarification resolves with one tap on an option.
  await page
    .getByTestId("execass-attention-card")
    .getByRole("button", { name: "Harbor House" })
    .click();
  await expect(page.getByTestId("execass-attention-card")).toHaveCount(0);
  await expect(office.getByText("Nothing needs you. Reassuringly quiet.")).toBeVisible();

  // Receipts: every completion claim offers proof one click away.
  await page
    .getByTestId("execass-done-row")
    .first()
    .getByRole("button", { name: /see the proof/i })
    .click();
  await expect(page.getByTestId("execass-proof")).toContainText("rcpt-");

  // Delegation through the ask-box lands as durable work.
  await office
    .getByLabel("Delegate an outcome to ExecAss")
    .fill("Chase the two unpaid invoices from June, politely");
  await office.getByRole("button", { name: /hand it off/i }).click();
  await expect(
    office.getByText("Chase the two unpaid invoices from June, politely").first(),
  ).toBeVisible();

  // A question comes back as conversation, not fake work.
  await office
    .getByLabel("Delegate an outcome to ExecAss")
    .fill("Are the June invoices already paid?");
  await office.getByRole("button", { name: /hand it off/i }).click();
  await expect(page.getByTestId("execass-reply")).toContainText(
    "already paid",
  );

  // The freeze switch stops the floor and resumes it, with proofs.
  await office.getByRole("button", { name: /freeze/i }).click();
  await office
    .getByRole("button", { name: /yes - everybody freeze/i })
    .click();
  await expect(office.getByText(/everybody froze/i)).toBeVisible();
  await office.getByRole("button", { name: /resume the floor/i }).click();
  await expect(office.getByText("● on duty")).toBeVisible();

  // The Office adds no nav entry: it replaces, it does not sprawl.
  const navItemsAfter = await page.locator('[data-tour-id^="nav-"]').count();
  expect(navItemsAfter).toBe(navItemsBefore);
});
