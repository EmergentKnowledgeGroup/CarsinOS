import { fileURLToPath } from "node:url";

import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

function activeMailSection(page: import("./testHarness").Page) {
  return page
    .getByTestId("mail-page")
    .locator('.mc-sub-tabs button[aria-selected="true"]');
}

function qaArtifact(name: string) {
  return fileURLToPath(
    new URL(`../../../runtime/qa/p5-directory-slice/${name}`, import.meta.url),
  );
}

async function dismissVisibleToasts(page: import("./testHarness").Page) {
  await page.locator(".mc-toast-dismiss").evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLButtonElement).click();
    }
  });
}

async function prepareMobileEvidence(page: import("./testHarness").Page) {
  const hideGuides = page.getByRole("button", { name: "Hide quick guides" });
  if (await hideGuides.isVisible().catch(() => false)) {
    await hideGuides.click();
  }
  // The crash sentinel exists only in E2E mode; it is not product chrome and
  // must not manufacture a false mobile-header collision in visual evidence.
  await page.locator('[data-testid="e2e-crash-active-tab"]').evaluateAll(
    (controls) => {
      for (const control of controls) {
        (control as HTMLElement).style.display = "none";
      }
    },
  );
}

/**
 * P5 Basement · Directory / Front Desk room slice: the stable directory room
 * owns the mail route and lands on Front Desk, the one rehomed People &
 * Routing authority (server-backed people, links, assignments, policies)
 * with the real save path. Direct Agent Mail and the temporarily retained
 * File locks stay one tap away with exact 8/6 pagination including live
 * shrink and regrowth. The hidden directory shortcut pins config-only with
 * full-canvas byte immutability, and the Office door executes the exact
 * stable-room landing through disable/restore and reload. Verified at
 * desktop and 390px with console-error, requestfailed, DF-mark/badge
 * non-overlap, and inner-rect containment assertions.
 */

test("@core @p5-directory directory room identity, Front Desk authority, Mail/File-lock parity, pin-to-office, and the office door hold at desktop and 390px", async ({
  page,
}) => {
  test.setTimeout(150_000);
  const browserErrors: string[] = [];
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      browserErrors.push(message.text());
    }
  });
  page.on("requestfailed", (request) => {
    browserErrors.push(
      `request failed: ${request.method()} ${request.url()} ${request.failure()?.errorText ?? "unknown"}`,
    );
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      browserErrors.push(`${response.status()} ${response.url()}`);
    }
  });
  // Every write against the immutability-sensitive authorities is recorded so
  // pinning can be proven config-only rather than merely count-equal.
  const sensitiveMutations: Array<{
    method: string;
    url: string;
    body: unknown;
  }> = [];
  page.on("request", (request) => {
    if (
      request.method() !== "GET" &&
      /\/api\/v1\/(config\/runtime|agent-mail|memory)/.test(request.url())
    ) {
      let body: unknown = null;
      try {
        body = request.postDataJSON();
      } catch {
        body = request.postData();
      }
      sensitiveMutations.push({
        method: request.method(),
        url: request.url(),
        body,
      });
    }
  });

  await completeQuickstartLocalOnboarding(page);

  // The Directory room exists once and lights exactly one lamp by stable id.
  const roomButton = page.locator(
    'button[title="BF · Directory / Front Desk"]',
  );
  await expect(roomButton).toHaveCount(1);
  const activeRooms = page.locator(".mc-nav-item-active");
  await roomButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Directory / Front Desk",
  );

  // Entering the directory room lands on Front Desk: the one rehomed People
  // & Routing authority with only server-backed people.
  const mailPage = page.getByTestId("mail-page");
  await expect(mailPage).toBeVisible();
  await expect(activeMailSection(page)).toHaveText("Front Desk");
  await expect(mailPage).toContainText("People And Routing");
  await expect(mailPage).toContainText("You");
  await expect(mailPage).toContainText("local-operator");
  await expect(mailPage).toContainText("needs assistant");

  // The ready Front Desk surface owns the directory pin; Messages and File
  // locks are never labeled as Directory data.
  const pin = page.getByRole("button", {
    name: "Pin Directory / Front Desk to Office",
  });
  await expect(pin).toBeVisible();

  // The real routing save path stays authoritative from its new home.
  await mailPage
    .locator(".mc-team-routing-card select")
    .first()
    .selectOption({ label: "Root" });
  await expect(mailPage).toContainText("assistant: Root");
  const mutationsBeforeRoutingSave = sensitiveMutations.length;
  await mailPage.getByRole("button", { name: "Save Routing" }).click();
  await expect(mailPage).toContainText("People and routing saved.");
  await expect(mailPage).toContainText("local operator");
  const routingMutations = sensitiveMutations.slice(mutationsBeforeRoutingSave);
  expect(routingMutations).toHaveLength(1);
  expect(routingMutations[0]).toMatchObject({
    method: "POST",
    body: {
      routing: {
        enabled: true,
        use_channel_defaults_as_fallback: false,
        local_operator_human_identity_id: "local-operator",
        dm_unmapped_policy: "approval_required",
        shared_unmapped_policy: "block",
        human_identities: [
          {
            human_identity_id: "local-operator",
            display_name: "You",
            enabled: true,
          },
        ],
        platform_identity_links: [],
        assistant_assignments: [
          {
            human_identity_id: "local-operator",
            assistant_agent_id: "agent-root",
            enabled: true,
          },
        ],
        lane_memory_policies: [],
      },
    },
  });
  expect(routingMutations[0]?.url).toMatch(/\/api\/v1\/config\/runtime$/);
  await mailPage.getByRole("button", { name: "Refresh" }).click();
  await expect(mailPage).toContainText("assistant: Root");

  // Both routing views render their own real content.
  await mailPage.getByRole("button", { name: "Routing Setup" }).click();
  await expect(mailPage).toContainText("Humans currently active in routing.");
  await expect(mailPage).toContainText("Unknown DMs");
  await expect(mailPage).toContainText("Unknown shared-space messages");
  await mailPage
    .getByRole("button", { name: "People", exact: true })
    .click();
  await expect(mailPage).toContainText("assistant: Root");
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-front-desk-desktop.png"),
    fullPage: true,
  });

  // Internal section choices persist; only a real room change relands.
  await mailPage.getByRole("tab", { name: "Messages" }).click();
  await expect(activeMailSection(page)).toContainText("Messages");
  await expect(mailPage).toContainText("Front desk handoff 1");
  await page.locator('button[title="2F · Staff Directory"]').click();
  await expect(page.getByTestId("team-page")).toBeVisible();
  await roomButton.click();
  await expect(activeMailSection(page)).toHaveText("Front Desk");
  await expect(
    page.getByRole("button", { name: "Pin Directory / Front Desk to Office" }),
  ).toBeVisible();

  // Direct Mail parity: exact eight-item pagination over nine live threads.
  await mailPage.getByRole("tab", { name: "Messages" }).click();
  await expect(
    page.getByRole("button", { name: "Pin Directory / Front Desk to Office" }),
  ).toHaveCount(0);
  const mailboxSelect = mailPage.locator(
    '.mc-mail-filters label:has-text("Mailbox") select',
  );
  const actingAsSelect = mailPage.locator(
    '.mc-mail-filters label:has-text("Acting as") select',
  );
  const threadItems = mailPage.locator(".mc-mail-thread-item");

  // The stateful fixture must honor the real mailbox/principal query instead
  // of returning one false dataset for every filter combination.
  await mailboxSelect.selectOption("inbox");
  await expect(threadItems).toHaveCount(5);
  await expect(threadItems.first()).toContainText("Front desk handoff 1");
  await expect(threadItems.last()).toContainText("Front desk handoff 5");
  await mailboxSelect.selectOption("outbox");
  await expect(threadItems).toHaveCount(4);
  await expect(threadItems.first()).toContainText("Front desk handoff 6");
  await expect(threadItems.last()).toContainText("Front desk handoff 9");
  await actingAsSelect.selectOption("default");
  await expect(threadItems).toHaveCount(5);
  await expect(threadItems.first()).toContainText("Front desk handoff 1");
  await mailboxSelect.selectOption("inbox");
  await expect(threadItems).toHaveCount(4);
  await expect(threadItems.first()).toContainText("Front desk handoff 6");
  await actingAsSelect.selectOption("");
  await mailboxSelect.selectOption("all");

  await expect(threadItems).toHaveCount(8);
  const pagination = mailPage.locator(".mc-mail-sidebar .mc-pagination");
  await expect(pagination.locator(".mc-pagination-info")).toHaveText("1 / 2");
  // Thread rows scroll inside the list; the list's clip region must end
  // above the pagination so no row can paint underneath it.
  expect(
    await page.evaluate(() => {
      const list = document.querySelector(".mc-mail-sidebar .mc-mail-thread-list");
      const paginationNode = document.querySelector(
        ".mc-mail-sidebar .mc-pagination",
      );
      if (!list || !paginationNode) return null;
      const listRect = list.getBoundingClientRect();
      const paginationRect = paginationNode.getBoundingClientRect();
      const overflowY = window.getComputedStyle(list).overflowY;
      return (
        (overflowY === "auto" || overflowY === "scroll") &&
        listRect.bottom <= paginationRect.top + 1
      );
    }),
  ).toBe(true);
  await pagination.getByRole("button", { name: "Next" }).click();
  await expect(pagination.locator(".mc-pagination-info")).toHaveText("2 / 2");
  await expect(threadItems).toHaveCount(1);
  await expect(threadItems.first()).toContainText("Front desk handoff 9");
  await pagination.getByRole("button", { name: "Prev" }).click();
  await expect(pagination.locator(".mc-pagination-info")).toHaveText("1 / 2");

  // Thread selection exposes the exact conversation with honest ack facts.
  await threadItems.filter({ hasText: "Front desk handoff 1" }).click();
  await expect(mailPage).toContainText("Latest note for Front desk handoff 1");
  await expect(mailPage).toContainText("0/1 acknowledged");

  // Acknowledge drives the real seam; reopening the thread shows the new
  // authoritative ack state.
  await mailPage.getByRole("button", { name: "Acknowledge" }).click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Message acknowledged." }),
  ).toBeVisible();
  await threadItems.filter({ hasText: "Front desk handoff 2" }).click();
  await threadItems.filter({ hasText: "Front desk handoff 1" }).click();
  await expect(mailPage).toContainText("1/1 acknowledged");

  // Compose sends through the controller, clears only after success, and the
  // authoritative thread list reflects the new latest message.
  const composeBody = mailPage.locator(".mc-mail-compose textarea");
  await mailPage.getByRole("button", { name: "Options" }).click();
  await mailPage
    .locator('.mc-mail-compose-options label:has-text("Sender") select')
    .selectOption("agent-root");
  await mailPage
    .locator(".mc-mail-compose-options")
    .getByRole("button", { name: "Local Assistant" })
    .click();
  await composeBody.fill("Fresh handoff note from the front desk");
  const mutationsBeforeSend = sensitiveMutations.length;
  await mailPage.getByRole("button", { name: "Send", exact: true }).click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Message sent." }),
  ).toBeVisible();
  await expect(composeBody).toHaveValue("");
  await expect(
    threadItems.filter({ hasText: "Front desk handoff 1" }),
  ).toContainText("Fresh handoff note from the front desk");
  const sendMutations = sensitiveMutations.slice(mutationsBeforeSend);
  expect(sendMutations).toHaveLength(1);
  expect(sendMutations[0]).toMatchObject({
    method: "POST",
    body: {
      body_text: "Fresh handoff note from the front desk",
      sender_principal: "agent-root",
      sender_kind: "agent",
      recipients: ["default"],
    },
  });
  expect(sendMutations[0]?.url).toMatch(
    /\/api\/v1\/agent-mail\/threads\/mail-thread-1\/messages$/,
  );

  // An empty send fails honestly without clearing anything or firing a
  // request that could pretend success.
  await dismissVisibleToasts(page);
  await mailPage.getByRole("button", { name: "Send", exact: true }).click();
  await expect(
    page
      .locator(".mc-toast")
      .filter({ hasText: "Message body cannot be empty." }),
  ).toBeVisible();
  await dismissVisibleToasts(page);

  // Search filters the authoritative list; the filtered empty state offers a
  // real clear that restores the full list.
  const searchInput = mailPage.locator(
    '.mc-mail-filters label:has-text("Search") input',
  );
  await searchInput.fill("handoff 9");
  await expect(threadItems).toHaveCount(1);
  await expect(threadItems.first()).toContainText("Front desk handoff 9");
  await searchInput.fill("zzz-no-such-thread");
  await expect(mailPage).toContainText(
    "No direct threads match your current filters.",
  );
  await mailPage.getByRole("button", { name: "Clear filters" }).click();
  await expect(threadItems).toHaveCount(8);

  // Creating a thread lands on the created conversation.
  await mailPage.getByRole("button", { name: "+ New Thread" }).click();
  const createDialog = page.getByRole("dialog", { name: "New Direct Thread" });
  await createDialog.locator("input").first().fill("Directory smoke thread");
  await createDialog
    .getByRole("button", { name: "Local Assistant" })
    .click();
  const mutationsBeforeThreadCreate = sensitiveMutations.length;
  await createDialog.getByRole("button", { name: "Create Thread" }).click();
  await expect(createDialog).toHaveCount(0);
  await expect(
    mailPage.locator(".mc-mail-thread-view h2"),
  ).toHaveText("Directory smoke thread");
  await expect(mailPage).toContainText("No messages in this thread yet.");
  const threadCreateMutations = sensitiveMutations.slice(
    mutationsBeforeThreadCreate,
  );
  expect(threadCreateMutations).toHaveLength(1);
  expect(threadCreateMutations[0]).toMatchObject({
    method: "POST",
    body: {
      kind: "direct",
      subject: "Directory smoke thread",
      participants: ["default"],
    },
  });
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-messages-desktop.png"),
    fullPage: true,
  });

  // Temporarily retained File locks: exact six-item pagination over seven
  // live leases, live shrink on release, and regrowth landing on the clamped
  // page instead of a resurrected later page.
  await mailPage.getByRole("tab", { name: "File locks" }).click();
  await expect(mailPage).toContainText("Advisory file locks");
  await expect(mailPage).toContainText("7 active file lock(s)");
  const leaseRows = mailPage.locator(".mc-mail-lease-list > li");
  await expect(leaseRows).toHaveCount(6);
  const leasePagination = mailPage.locator(".mc-lease-page .mc-pagination");
  await expect(leasePagination.locator(".mc-pagination-info")).toHaveText(
    "1 / 2",
  );
  await leasePagination.getByRole("button", { name: "Next" }).click();
  await expect(leaseRows).toHaveCount(1);
  await expect(leaseRows.first()).toContainText("src/area-7/**");
  await leaseRows.first().getByRole("button", { name: "Release" }).click();
  const releaseDialog = page.getByRole("dialog", {
    name: "Release file lock?",
  });
  await expect(releaseDialog).toContainText("src/area-7/**");
  const mutationsBeforeRelease = sensitiveMutations.length;
  await releaseDialog.getByRole("button", { name: "Release" }).click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Lease released." }),
  ).toBeVisible();
  // Live shrink to one page: the stored page clamps instead of lingering.
  await expect(mailPage).toContainText("6 active file lock(s)");
  await expect(leasePagination).toHaveCount(0);
  await expect(leaseRows).toHaveCount(6);
  await expect(leaseRows.first()).toContainText("src/area-1/**");
  const releaseMutations = sensitiveMutations.slice(mutationsBeforeRelease);
  expect(releaseMutations).toHaveLength(1);
  expect(releaseMutations[0]).toMatchObject({
    method: "POST",
    body: {},
  });
  expect(releaseMutations[0]?.url).toMatch(
    /\/api\/v1\/agent-mail\/leases\/mail-lease-\d+\/release$/,
  );

  // Reserve a new lock; regrowth shows page one, not a resurrected page two.
  await dismissVisibleToasts(page);
  await mailPage.getByRole("button", { name: "+ New file lock" }).click();
  const leaseDialog = page.getByRole("dialog", { name: "Reserve file lock" });
  await leaseDialog
    .locator('label:has-text("Glob Pattern") select')
    .selectOption({ label: "Docs (docs/**/*)" });
  await leaseDialog.getByRole("button", { name: "1h", exact: true }).click();
  const mutationsBeforeLeaseCreate = sensitiveMutations.length;
  await leaseDialog
    .getByRole("button", { name: "Reserve file lock" })
    .click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Lease created: docs/**/*" }),
  ).toBeVisible();
  await expect(mailPage).toContainText("7 active file lock(s)");
  await expect(leasePagination.locator(".mc-pagination-info")).toHaveText(
    "1 / 2",
  );
  await expect(leaseRows.first()).toContainText("src/area-1/**");
  const leaseCreateMutations = sensitiveMutations.slice(
    mutationsBeforeLeaseCreate,
  );
  expect(leaseCreateMutations).toHaveLength(1);
  expect(leaseCreateMutations[0]).toMatchObject({
    method: "POST",
    body: {
      glob_pattern: "docs/**/*",
      exclusive: false,
      ttl_ms: 3_600_000,
    },
  });
  expect(leaseCreateMutations[0]?.url).toMatch(
    /\/api\/v1\/agent-mail\/leases$/,
  );
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-file-locks-desktop.png"),
    fullPage: true,
  });

  // Pinning is config-only. The full-canvas fixture genuinely contains every
  // earlier registered shortcut, proves its capacity precondition, refuses
  // byte-for-byte, and no runtime-config/Mail/lease/note mutation fires.
  await mailPage.getByRole("tab", { name: "Front Desk" }).click();
  const mutationsBeforePinFlow = JSON.parse(
    JSON.stringify(sensitiveMutations),
  ) as typeof sensitiveMutations;
  const fullCanvas = await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    const earlierShortcuts = [
      "boards",
      "calendar",
      "strategy",
      "staff",
      "history",
      "connectors",
      "models",
      "events",
      "breakers",
    ];
    // in-motion (2x2) + next (2x1) + nine 2x1 shortcuts = 24 of 24 cells.
    config.layout = [
      { id: "needs-you", size: "l", visible: false },
      { id: "in-motion", size: "m", visible: true },
      { id: "done", size: "m", visible: false },
      { id: "next", size: "s", visible: true },
      ...earlierShortcuts.map((id) => ({ id, size: "s", visible: true })),
    ];
    const serialized = JSON.stringify(config);
    localStorage.setItem(key, serialized);
    window.dispatchEvent(new Event("mc-glass-config-changed"));
    return {
      serialized,
      visibleEarlierShortcuts: config.layout.filter(
        (placement: { id: string; visible: boolean }) =>
          earlierShortcuts.includes(placement.id) && placement.visible,
      ).length,
    };
  });
  expect(fullCanvas.visibleEarlierShortcuts).toBe(9);
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note.is-error")).toHaveText(
    "Pinning this would exceed the six-column, four-row Office canvas.",
  );
  expect(
    await page.evaluate(() => localStorage.getItem("mc-glass-config-v1")),
  ).toBe(fullCanvas.serialized);
  expect(
    await page.evaluate(() => {
      const config = JSON.parse(
        localStorage.getItem("mc-glass-config-v1") ?? "{}",
      );
      return config.layout?.some(
        (placement: { id?: string }) => placement.id === "directory",
      );
    }),
  ).toBe(false);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-full-canvas-refusal.png"),
    fullPage: true,
  });

  // Freeing one medium block admits the tenth shortcut; a repeat pin stays
  // byte-identical and honest.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "in-motion"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator(".mc-pin-to-office-note")).toHaveCount(0);
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "On the Office canvas.",
  );
  const pinnedConfig = await page.evaluate(() =>
    localStorage.getItem("mc-glass-config-v1"),
  );
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "Already on the Office canvas.",
  );
  expect(
    await page.evaluate(() => localStorage.getItem("mc-glass-config-v1")),
  ).toBe(pinnedConfig);
  expect(sensitiveMutations).toEqual(mutationsBeforePinFlow);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-pinned-desktop.png"),
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas, names its floor, and
  // consumes config events live without a route remount.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-directory");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "directory"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-directory")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "directory"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(shortcut).toBeVisible();
  await shortcut.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("office-shortcut-block.png"),
    fullPage: true,
  });

  // Opening the door lands the exact stable room and Front Desk section.
  await shortcut
    .getByRole("button", { name: "Open Directory / Front Desk" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Directory / Front Desk",
  );
  await expect(mailPage).toBeVisible();
  await expect(activeMailSection(page)).toHaveText("Front Desk");

  // The pin is config: it survives a full reload and still opens the exact
  // room and Front Desk section afterwards.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-directory");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open Directory / Front Desk" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Directory / Front Desk",
  );
  await expect(activeMailSection(page)).toHaveText("Front Desk");

  // A disabled Basement floor turns the persisted door into an honest
  // refusal instead of a silent dead button.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.floorOverrides = {
      ...config.floorOverrides,
      basement: { hidden: true },
    };
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(roomButton).toHaveCount(0);
  await page
    .getByTestId("office-block-directory")
    .getByRole("button", { name: "Open Directory / Front Desk" })
    .click();
  await expect(
    page.getByTestId("office-block-directory").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-disabled-door.png"),
    fullPage: true,
  });

  // Restoring the floor clears the stale refusal without a clearing click,
  // and the restored action must actually execute to the exact destination.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.basement;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(roomButton).toHaveCount(1);
  await expect(
    page.getByTestId("office-block-directory").getByRole("status"),
  ).toHaveCount(0);
  await page
    .getByTestId("office-block-directory")
    .getByRole("button", { name: "Open Directory / Front Desk" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Directory / Front Desk",
  );
  await expect(activeMailSection(page)).toHaveText("Front Desk");
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-restored-door.png"),
    fullPage: true,
  });

  // Narrow width: a readable nonzero DF mark that does not overlap its live
  // unread badge, contained section tabs, and inner geometry for the Front
  // Desk people card and routing controls.
  await page.setViewportSize({ width: 390, height: 844 });
  await prepareMobileEvidence(page);
  await roomButton.scrollIntoViewIfNeeded();
  await expect(roomButton).toHaveClass(/mc-nav-item-active/);
  const mark = roomButton.locator(".mc-nav-room-mark");
  await expect(mark).toHaveText("DF");
  await expect(mark).toBeVisible();
  expect(
    await mark.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth &&
        rect.top >= 0 &&
        rect.bottom <= window.innerHeight
      );
    }),
  ).toBe(true);
  await expect(roomButton.locator(".mc-nav-badge")).toBeVisible();
  expect(
    await roomButton.evaluate((button) => {
      const markRect = button
        .querySelector(".mc-nav-room-mark")!
        .getBoundingClientRect();
      const badgeRect = button
        .querySelector(".mc-nav-badge")!
        .getBoundingClientRect();
      return (
        markRect.width > 0 &&
        badgeRect.width > 0 &&
        (markRect.right <= badgeRect.left ||
          badgeRect.right <= markRect.left ||
          markRect.bottom <= badgeRect.top ||
          badgeRect.bottom <= markRect.top)
      );
    }),
  ).toBe(true);

  const frontDeskGeometry = await page.evaluate(() => {
    const containsX = (outer: DOMRect, inner: DOMRect, slack = 1): boolean =>
      inner.width > 0 &&
      inner.height > 0 &&
      inner.left >= outer.left - slack &&
      inner.right <= outer.right + slack;
    const mail = document.querySelector('[data-testid="mail-page"]');
    if (!mail) return { ok: false as const, reason: "no mail page" };
    const mailRect = mail.getBoundingClientRect();
    const tabs = Array.from(
      mail.querySelectorAll(".mc-sub-tabs .mc-sub-tab"),
    ).map((tab) => containsX(mailRect, tab.getBoundingClientRect()));
    const surface = mail.querySelector(".mc-team-routing-surface");
    if (!surface) return { ok: false as const, reason: "no routing surface" };
    const surfaceRect = surface.getBoundingClientRect();
    const card = surface.querySelector(".mc-team-routing-card");
    const cardOk = card
      ? containsX(surfaceRect, card.getBoundingClientRect())
      : false;
    const controls = Array.from(
      surface.querySelectorAll(".mc-strategy-inline-actions button"),
    ).map((control) => containsX(surfaceRect, control.getBoundingClientRect()));
    return { ok: true as const, tabs, cardOk, controls };
  });
  expect(frontDeskGeometry.ok).toBe(true);
  if (frontDeskGeometry.ok) {
    expect(frontDeskGeometry.tabs.length).toBe(3);
    expect(frontDeskGeometry.tabs).not.toContain(false);
    expect(frontDeskGeometry.cardOk).toBe(true);
    expect(frontDeskGeometry.controls.length).toBeGreaterThan(0);
    expect(frontDeskGeometry.controls).not.toContain(false);
  }
  const firstRoutingCard = mailPage.locator(".mc-team-routing-card").first();
  await firstRoutingCard.scrollIntoViewIfNeeded();
  await expect(firstRoutingCard).toBeInViewport();
  const routingCardVisible = await firstRoutingCard.evaluate((card) => {
    const cardRect = card.getBoundingClientRect();
    const surface = card.closest(".mc-team-routing-surface");
    if (!surface) return false;
    const surfaceRect = surface.getBoundingClientRect();
    return (
      cardRect.width > 0 &&
      cardRect.height > 0 &&
      cardRect.left >= surfaceRect.left - 1 &&
      cardRect.right <= surfaceRect.right + 1 &&
      cardRect.top < window.innerHeight &&
      cardRect.bottom > 0
    );
  });
  expect(routingCardVisible).toBe(true);
  await roomButton.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-front-desk-390.png"),
    fullPage: true,
  });

  // Mobile Messages: list/detail switching, contained rows, a visible
  // scrolled-into-view pagination, and a contained compose.
  await mailPage.getByRole("tab", { name: "Messages" }).click();
  await mailPage
    .locator('.mc-mail-filters label:has-text("Mailbox") select')
    .selectOption("all");
  await expect(mailPage.locator(".mc-mail-grid")).toHaveClass(
    /mc-mobile-list-open/,
  );
  const mobilePagination = mailPage.locator(".mc-mail-sidebar .mc-pagination");
  await mobilePagination.scrollIntoViewIfNeeded();
  await expect(mobilePagination).toBeInViewport();
  expect(
    await mobilePagination.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth
      );
    }),
  ).toBe(true);
  const mobileListGeometry = await page.evaluate(() => {
    const sidebar = document.querySelector(".mc-mail-sidebar");
    if (!sidebar) return null;
    const sidebarRect = sidebar.getBoundingClientRect();
    return Array.from(sidebar.querySelectorAll(".mc-mail-thread-item")).map(
      (item) => {
        const rect = item.getBoundingClientRect();
        return (
          rect.width > 0 &&
          rect.left >= sidebarRect.left - 1 &&
          rect.right <= sidebarRect.right + 1
        );
      },
    );
  });
  expect(mobileListGeometry).not.toBeNull();
  expect(mobileListGeometry).not.toContain(false);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-messages-390.png"),
    fullPage: true,
  });
  await mailPage
    .locator(".mc-mail-thread-item")
    .filter({ hasText: "Front desk handoff 1" })
    .click();
  await expect(mailPage.locator(".mc-mail-grid")).toHaveClass(
    /mc-mobile-detail-open/,
  );
  const composeRect = await mailPage
    .locator(".mc-mail-compose textarea")
    .evaluate((node) => {
      const grid = node.closest(".mc-mail-thread-view");
      if (!grid) return null;
      const outer = grid.getBoundingClientRect();
      const inner = node.getBoundingClientRect();
      return (
        inner.width > 0 &&
        inner.left >= outer.left - 1 &&
        inner.right <= outer.right + 1
      );
    });
  expect(composeRect).toBe(true);
  await mailPage.getByRole("button", { name: "Back to threads" }).click();
  await expect(mailPage.locator(".mc-mail-grid")).toHaveClass(
    /mc-mobile-list-open/,
  );

  // Mobile File locks: contained rows and pagination, then no horizontal
  // overflow anywhere on the page.
  await mailPage.getByRole("tab", { name: "File locks" }).click();
  const mobileLeaseGeometry = await page.evaluate(() => {
    const list = document.querySelector(".mc-mail-lease-list");
    if (!list) return null;
    const listRect = list.getBoundingClientRect();
    return Array.from(list.querySelectorAll(":scope > li")).map((row) => {
      const rect = row.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.left >= listRect.left - 1 &&
        rect.right <= listRect.right + 1
      );
    });
  });
  expect(mobileLeaseGeometry).not.toBeNull();
  expect(mobileLeaseGeometry).not.toContain(false);
  const mobileLeasePagination = mailPage.locator(
    ".mc-lease-page .mc-pagination",
  );
  await mobileLeasePagination.scrollIntoViewIfNeeded();
  await expect(mobileLeasePagination).toBeInViewport();
  expect(
    await mobileLeasePagination.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth
      );
    }),
  ).toBe(true);
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: qaArtifact("directory-file-locks-390.png"),
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
