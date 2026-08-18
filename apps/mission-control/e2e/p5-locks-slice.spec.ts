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
    new URL(`../../../runtime/qa/p5-locks-slice/${name}`, import.meta.url),
  );
}

async function dismissVisibleToasts(page: import("./testHarness").Page) {
  await page.locator(".mc-toast-dismiss").evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLButtonElement).click();
    }
  });
}

async function prepareQaEvidence(page: import("./testHarness").Page) {
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

async function assertLocksSurfaceGeometry(
  page: import("./testHarness").Page,
) {
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  const geometry = await page.getByTestId("mail-page").evaluate((mail) => {
    const outer = mail.getBoundingClientRect();
    const visible = (rect: DOMRect) => rect.width > 0 && rect.height > 0;
    const containedX = (rect: DOMRect, slack = 1) =>
      visible(rect) &&
      rect.left >= outer.left - slack &&
      rect.right <= outer.right + slack;
    const selectors = [
      ".mc-tab-help-banner",
      ".mc-sub-tabs",
      ".mc-mail-lease-list",
      ".mc-mail-lease-list > li",
      ".mc-mail-lease-list button",
    ];
    const visibleNodes = selectors
      .flatMap((selector) => Array.from(mail.querySelectorAll(selector)))
      .map((node) => node.getBoundingClientRect())
      .filter((rect) => visible(rect));
    return {
      outerVisible: visible(outer),
      contained: visibleNodes.map((rect) => containedX(rect)),
    };
  });
  expect(geometry.outerVisible).toBe(true);
  expect(geometry.contained.length).toBeGreaterThan(8);
  expect(geometry.contained).not.toContain(false);
}

async function captureQaArtifact(
  page: import("./testHarness").Page,
  name: string,
) {
  await prepareQaEvidence(page);
  await page.screenshot({
    path: qaArtifact(name),
    fullPage: true,
  });
}

/**
 * P5 Basement · File locks room slice: the stable locks room shares the mail
 * route with Directory / Front Desk but owns its own lamp, lands on the
 * advisory lease machinery, and never claims filesystem enforcement. The one
 * Agent Mail controller keeps serving Directory and Agent rooms; lease reads
 * fail independently with an honest error and retry. Reserve and release
 * drive the real gateway lease routes with exact bodies and acting
 * principal, six-item pagination shrinks and regrows against live server
 * truth, and the hidden locks shortcut pins config-only with
 * twelfth-shortcut full-canvas byte immutability. The Office door executes
 * the exact stable-room landing through disable/restore and reload.
 * Verified at desktop and 390px with console-error, requestfailed, FL-mark,
 * and inner-rect containment assertions.
 */

test("@core @p5-locks locks room identity, advisory lease truth, exact lease mutations, pin-to-office, and the office door hold at desktop and 390px", async ({
  page,
}) => {
  test.setTimeout(180_000);
  const browserErrors: string[] = [];
  // One deliberately injected lease-read failure proves the honest error
  // surface; only that exact GET 500 against the lease route is excluded
  // from the error budget, and the exclusion count is itself asserted.
  let expectInjectedLeaseListFailure = false;
  let injectedLeaseFailureConsoleErrors = 0;
  let injectedLeaseFailureRequests = 0;
  let injectedLeaseFailureResponses = 0;
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      const sourceUrl = message.location().url;
      if (
        expectInjectedLeaseListFailure &&
        message.text().includes("500") &&
        /\/api\/v1\/agent-mail\/leases/.test(`${sourceUrl} ${message.text()}`)
      ) {
        injectedLeaseFailureConsoleErrors += 1;
        return;
      }
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
      if (
        expectInjectedLeaseListFailure &&
        response.status() === 500 &&
        response.request().method() === "GET" &&
        /\/api\/v1\/agent-mail\/leases(\?|$)/.test(response.url())
      ) {
        injectedLeaseFailureResponses += 1;
        return;
      }
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
      expectInjectedLeaseListFailure &&
      request.method() === "GET" &&
      /\/api\/v1\/agent-mail\/leases(\?|$)/.test(request.url())
    ) {
      injectedLeaseFailureRequests += 1;
    }
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

  await completeQuickstartLocalOnboarding(page, {
    beforeGoto: async (nextPage) => {
      await nextPage.addInitScript(() => {
        window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
      });
    },
  });

  // The File locks room exists once and lights exactly one lamp by stable id
  // while Directory shares the mail route without stealing it.
  const roomButton = page.locator('button[title="BF · File locks"]');
  await expect(roomButton).toHaveCount(1);
  const directoryButton = page.locator(
    'button[title="BF · Directory / Front Desk"]',
  );
  const activeRooms = page.locator(".mc-nav-item-active");
  await roomButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · File locks");

  // Entering the locks room lands on the advisory lease machinery with plain
  // truth: cooperative reservations, never filesystem enforcement.
  const mailPage = page.getByTestId("mail-page");
  await expect(mailPage).toBeVisible();
  await expect(activeMailSection(page)).toContainText("File locks");
  await expect(mailPage).toContainText("Advisory file locks");
  await expect(mailPage).toContainText(
    "Cooperative advisory reservations agents coordinate through Agent Mail.",
  );
  await expect(mailPage).toContainText(
    "Nothing is enforced at the filesystem level",
  );
  await expect(mailPage).toContainText("7 active file lock(s)");

  // Server-backed facts on the rows: holder, glob, shared vs exclusive, TTL.
  const leaseRows = mailPage.locator(".mc-mail-lease-list > li");
  await expect(leaseRows).toHaveCount(6);
  const firstRow = leaseRows.filter({ hasText: "src/area-1/**" });
  await expect(firstRow).toContainText("agent-root");
  await expect(firstRow).toContainText("exclusive");
  await expect(firstRow).toContainText("15m TTL");
  await expect(firstRow).toContainText("expires");
  await expect(leaseRows.filter({ hasText: "src/area-2/**" })).toContainText(
    "shared",
  );

  // The ready locks surface owns the locks pin; the Front Desk surface inside
  // this room does not surrender its identity to Directory's pin.
  const pin = page.getByRole("button", { name: "Pin File locks to Office" });
  await expect(pin).toBeVisible();
  await mailPage.getByRole("tab", { name: "Front Desk" }).click();
  await expect(
    page.getByRole("button", { name: "Pin Directory / Front Desk to Office" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Pin File locks to Office" }),
  ).toHaveCount(0);

  // Internal choices survive unrelated rerenders: an external Glass config
  // event must not reset the user's section.
  await mailPage.getByRole("tab", { name: "Messages" }).click();
  await expect(mailPage.locator(".mc-mail-thread-item").first()).toBeVisible();
  await page.evaluate(() => {
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(activeMailSection(page)).toContainText("Messages");

  // Leave-and-return obeys the newest explicit stable room: Directory lands
  // Front Desk, File locks relands the lease machinery.
  await directoryButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Directory / Front Desk",
  );
  await expect(activeMailSection(page)).toHaveText("Front Desk");
  await roomButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · File locks");
  await expect(activeMailSection(page)).toContainText("File locks");
  await assertLocksSurfaceGeometry(page);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-room-desktop.png");

  // A lease read failure must not erase Direct Mail truth: threads keep
  // rendering while File locks shows an honest error with a live retry.
  expectInjectedLeaseListFailure = true;
  await page.route(
    "**/api/v1/agent-mail/leases*",
    async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill({
          status: 500,
          contentType: "application/json",
          body: JSON.stringify({ error: "lease backend down" }),
        });
        return;
      }
      await route.fallback();
    },
    { times: 1 },
  );
  await mailPage.getByRole("tab", { name: "Messages" }).click();
  await mailPage.getByRole("button", { name: "Refresh" }).click();
  await expect(mailPage.locator(".mc-mail-thread-item").first()).toBeVisible();
  await mailPage.getByRole("tab", { name: "File locks" }).click();
  const leaseErrorPanel = mailPage.getByRole("alert").filter({
    hasText: "File locks couldn't be loaded",
  });
  await expect(leaseErrorPanel).toBeVisible();
  await expect(leaseErrorPanel).toContainText("500");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-error-desktop.png");
  await page.unroute("**/api/v1/agent-mail/leases*");
  expectInjectedLeaseListFailure = false;
  await leaseErrorPanel.getByRole("button", { name: "Retry" }).click();
  await expect(leaseErrorPanel).toHaveCount(0);
  await expect(mailPage).toContainText("7 active file lock(s)");

  // Reserve drives the real POST with the exact trimmed body, including the
  // acting principal, and the new row is refetched server truth.
  await mailPage.getByRole("button", { name: "+ New file lock" }).click();
  const leaseDialog = page.getByRole("dialog", { name: "Reserve file lock" });
  await leaseDialog
    .locator('label:has-text("Acting as") select')
    .selectOption("agent-root");
  await leaseDialog
    .locator('label:has-text("Glob Pattern") select')
    .selectOption("docs/**/*");
  await leaseDialog.getByRole("button", { name: "1h", exact: true }).click();
  await leaseDialog
    .locator('label:has-text("Note") input')
    .fill("locks room proof");
  await leaseDialog
    .locator('label:has-text("Exclusive") input[type="checkbox"]')
    .check();
  const mutationsBeforeLeaseCreate = sensitiveMutations.length;
  await leaseDialog
    .getByRole("button", { name: "Reserve file lock" })
    .click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Lease created: docs/**/*",
  );
  await expect(leaseDialog).toBeHidden();
  const leaseCreateMutations = sensitiveMutations.slice(
    mutationsBeforeLeaseCreate,
  );
  expect(leaseCreateMutations).toHaveLength(1);
  expect(leaseCreateMutations[0]?.method).toBe("POST");
  expect(leaseCreateMutations[0]?.url).toMatch(/\/api\/v1\/agent-mail\/leases$/);
  expect(leaseCreateMutations[0]?.body).toEqual({
    holder_principal: "agent-root",
    glob_pattern: "docs/**/*",
    exclusive: true,
    ttl_ms: 3_600_000,
    note: "locks room proof",
  });
  await expect(mailPage).toContainText("8 active file lock(s)");
  const leasePagination = mailPage.locator(".mc-lease-page .mc-pagination");
  await expect(leasePagination.locator(".mc-pagination-info")).toHaveText(
    "1 / 2",
  );
  await leasePagination.getByRole("button", { name: "Next" }).click();
  const createdRow = leaseRows.filter({ hasText: "docs/**/*" });
  await expect(createdRow).toHaveCount(1);
  await expect(createdRow).toContainText("agent-root");
  await expect(createdRow).toContainText("exclusive");
  await expect(createdRow).toContainText("1h TTL");
  await expect(createdRow).toContainText("locks room proof");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-facts-desktop.png");

  // Release is exactly one confirmation that states the consequence, sends
  // the exact lease id with the acting principal, and the shrink lands on
  // live server truth.
  await createdRow.getByRole("button", { name: "Release" }).click();
  const releaseDialog = page.getByRole("dialog", {
    name: "Release file lock?",
  });
  await expect(releaseDialog).toContainText("docs/**/*");
  await expect(releaseDialog).toContainText(
    "Other agents may begin writing to these paths.",
  );
  const mutationsBeforeRelease = sensitiveMutations.length;
  await releaseDialog.getByRole("button", { name: "Release" }).click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Lease released.",
  );
  await expect(releaseDialog).toBeHidden();
  const releaseMutations = sensitiveMutations.slice(mutationsBeforeRelease);
  expect(releaseMutations).toHaveLength(1);
  expect(releaseMutations[0]?.method).toBe("POST");
  expect(releaseMutations[0]?.url).toMatch(
    /\/api\/v1\/agent-mail\/leases\/mail-lease-\d+\/release$/,
  );
  expect(releaseMutations[0]?.body).toEqual({
    holder_principal: "agent-root",
  });
  await expect(mailPage).toContainText("7 active file lock(s)");
  await expect(createdRow).toHaveCount(0);

  // Second shrink: releasing the last seeded lease on page two collapses the
  // pagination and the stored page clamps to the first page.
  await expect(leasePagination.locator(".mc-pagination-info")).toHaveText(
    "2 / 2",
  );
  const areaSevenRow = leaseRows.filter({ hasText: "src/area-7/**" });
  await expect(areaSevenRow).toHaveCount(1);
  await areaSevenRow.getByRole("button", { name: "Release" }).click();
  await expect(releaseDialog).toContainText("src/area-7/**");
  await releaseDialog.getByRole("button", { name: "Release" }).click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Lease released.",
  );
  await expect(mailPage).toContainText("6 active file lock(s)");
  await expect(leasePagination).toHaveCount(0);
  await expect(leaseRows).toHaveCount(6);
  await expect(leaseRows.first()).toContainText("src/area-1/**");
  await expect(mailPage).not.toContainText("src/area-7/**");
  await dismissVisibleToasts(page);

  // Pinning is config-only. The full-canvas fixture genuinely contains every
  // earlier registered shortcut through Memory plant, proves its capacity
  // precondition, refuses byte-for-byte, and no lease/Mail/routing/memory
  // mutation fires anywhere in the pin flow.
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
      "directory",
      "memory",
    ];
    // next (2 cells) + eleven 2x1 shortcuts (22 cells) = 24 of 24.
    config.layout = [
      { id: "needs-you", size: "l", visible: false },
      { id: "in-motion", size: "m", visible: false },
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
  expect(fullCanvas.visibleEarlierShortcuts).toBe(11);
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
        (placement: { id?: string }) => placement.id === "locks",
      );
    }),
  ).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-full-canvas-refusal.png");

  // Freeing the small Next block admits the twelfth shortcut; a repeat pin
  // stays byte-identical and honest.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "next"
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
  await captureQaArtifact(page, "locks-pinned-desktop.png");

  // The pinned shortcut appears on the Office canvas, names its floor, and
  // consumes config events live without a route remount.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-locks");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "locks"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-locks")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "locks"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(shortcut).toBeVisible();
  await shortcut.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-office-shortcut.png");

  // Opening the door lands the exact stable room on the lease machinery.
  await shortcut.getByRole("button", { name: "Open File locks" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · File locks");
  await expect(mailPage).toBeVisible();
  await expect(activeMailSection(page)).toContainText("File locks");
  await expect(mailPage).toContainText("Advisory file locks");

  // The pin is config: it survives a full reload and still opens the exact
  // room afterwards.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-locks");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open File locks" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · File locks");
  await expect(activeMailSection(page)).toContainText("File locks");
  await expect(mailPage).toContainText("Advisory file locks");

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
    .getByTestId("office-block-locks")
    .getByRole("button", { name: "Open File locks" })
    .click();
  await expect(
    page.getByTestId("office-block-locks").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-disabled-door.png");

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
    page.getByTestId("office-block-locks").getByRole("status"),
  ).toHaveCount(0);
  await page
    .getByTestId("office-block-locks")
    .getByRole("button", { name: "Open File locks" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · File locks");
  await expect(activeMailSection(page)).toContainText("File locks");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-restored-door.png");

  // Narrow width: a readable nonzero FL mark that nothing paints over, plus
  // contained section tabs and lease rows.
  await page.setViewportSize({ width: 390, height: 844 });
  await roomButton.scrollIntoViewIfNeeded();
  await expect(roomButton).toHaveClass(/mc-nav-item-active/);
  const mark = roomButton.locator(".mc-nav-room-mark");
  await expect(mark).toHaveText("FL");
  await expect(mark).toBeVisible();
  expect(
    await mark.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      if (
        rect.width <= 0 ||
        rect.height <= 0 ||
        rect.left < 0 ||
        rect.right > window.innerWidth ||
        rect.top < 0 ||
        rect.bottom > window.innerHeight
      ) {
        return false;
      }
      // Nothing may paint over the mark's center: the hit target must be the
      // mark itself or its own room button.
      const centerHit = document.elementFromPoint(
        rect.left + rect.width / 2,
        rect.top + rect.height / 2,
      );
      return Boolean(
        centerHit && (centerHit === node || node.contains(centerHit) ||
          centerHit.contains(node)),
      );
    }),
  ).toBe(true);
  expect(
    await mark.evaluate((node) => {
      const markRect = node.getBoundingClientRect();
      return Array.from(document.querySelectorAll(".mc-nav-badge")).some(
        (badge) => {
          const badgeRect = badge.getBoundingClientRect();
          if (badgeRect.width <= 0 || badgeRect.height <= 0) {
            return false;
          }
          return !(
            markRect.right <= badgeRect.left ||
            markRect.left >= badgeRect.right ||
            markRect.bottom <= badgeRect.top ||
            markRect.top >= badgeRect.bottom
          );
        },
      );
    }),
  ).toBe(false);

  const lockListGeometry = await page.evaluate(() => {
    const containsX = (outer: DOMRect, inner: DOMRect, slack = 1): boolean =>
      inner.width > 0 &&
      inner.height > 0 &&
      inner.left >= outer.left - slack &&
      inner.right <= outer.right + slack;
    const mail = document.querySelector('[data-testid="mail-page"]');
    if (!mail) return { ok: false as const, reason: "no mail page" };
    const mailRect = mail.getBoundingClientRect();
    const tabs = Array.from(
      mail.querySelectorAll(".mc-sub-tabs button"),
    ).map((tab) => containsX(mailRect, tab.getBoundingClientRect()));
    const list = mail.querySelector(".mc-mail-lease-list");
    if (!list) return { ok: false as const, reason: "no lease list" };
    const listRect = list.getBoundingClientRect();
    const rows = Array.from(list.querySelectorAll(":scope > li")).map((row) =>
      containsX(listRect, row.getBoundingClientRect()),
    );
    return { ok: true as const, tabs, rows };
  });
  expect(lockListGeometry.ok).toBe(true);
  if (lockListGeometry.ok) {
    expect(lockListGeometry.tabs.length).toBe(3);
    expect(lockListGeometry.tabs).not.toContain(false);
    expect(lockListGeometry.rows.length).toBe(6);
    expect(lockListGeometry.rows).not.toContain(false);
  }
  await assertLocksSurfaceGeometry(page);
  const advisoryRow = mailPage
    .locator(".mc-mail-lease-list > li")
    .filter({ hasText: "src/area-1/**" });
  await advisoryRow.scrollIntoViewIfNeeded();
  await expect(advisoryRow).toBeInViewport();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-room-390.png");

  // The reserve modal works and stays contained at 390px, and the seventh
  // active lease brings the exact six-item pagination back on screen.
  await mailPage.getByRole("button", { name: "+ New file lock" }).click();
  const mobileLeaseDialog = page.getByRole("dialog", {
    name: "Reserve file lock",
  });
  await expect(mobileLeaseDialog).toBeVisible();
  expect(
    await mobileLeaseDialog.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth
      );
    }),
  ).toBe(true);
  // Dialog-level containment is not proof for its children: every field —
  // including the Exclusive checkbox that once clipped at the modal edge —
  // must sit inside the dialog box with a nonzero rect.
  const mobileDialogFieldGeometry = await mobileLeaseDialog.evaluate(
    (node) => {
      const dialogRect = node.getBoundingClientRect();
      const fields = Array.from(
        node.querySelectorAll("label, input, select, button"),
      );
      const hasExclusive = fields.some((field) =>
        field.textContent?.includes("Exclusive"),
      );
      const contained = fields
        .filter((field) => field.getBoundingClientRect().width > 0)
        .map((field) => {
          const rect = field.getBoundingClientRect();
          return (
            rect.left >= dialogRect.left - 1 &&
            rect.right <= dialogRect.right + 1
          );
        });
      return { hasExclusive, count: contained.length, contained };
    },
  );
  expect(mobileDialogFieldGeometry.hasExclusive).toBe(true);
  expect(mobileDialogFieldGeometry.count).toBeGreaterThan(5);
  expect(mobileDialogFieldGeometry.contained).not.toContain(false);
  await captureQaArtifact(page, "locks-modal-390.png");
  await mobileLeaseDialog
    .locator('label:has-text("Note") input')
    .fill("mobile proof");
  await mobileLeaseDialog
    .getByRole("button", { name: "Reserve file lock" })
    .click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Lease created: **/*",
  );
  await expect(mailPage).toContainText("7 active file lock(s)");
  const mobilePagination = mailPage.locator(".mc-lease-page .mc-pagination");
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

  // The release confirmation stays contained at 390px; cancelling keeps the
  // committed lease untouched.
  const mutationsBeforeMobileCancel = sensitiveMutations.length;
  await advisoryRow.scrollIntoViewIfNeeded();
  await advisoryRow.getByRole("button", { name: "Release" }).click();
  const mobileReleaseDialog = page.getByRole("dialog", {
    name: "Release file lock?",
  });
  await expect(mobileReleaseDialog).toContainText(
    "Other agents may begin writing to these paths.",
  );
  expect(
    await mobileReleaseDialog.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth
      );
    }),
  ).toBe(true);
  await mobileReleaseDialog.getByRole("button", { name: "Cancel" }).click();
  await expect(mobileReleaseDialog).toBeHidden();
  expect(sensitiveMutations.length).toBe(mutationsBeforeMobileCancel);
  await expect(mailPage).toContainText("7 active file lock(s)");
  await dismissVisibleToasts(page);

  // The Office door stays reachable and contained at 390px and still opens
  // the exact room.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const mobileShortcut = page.getByTestId("office-block-locks");
  await mobileShortcut.scrollIntoViewIfNeeded();
  await expect(mobileShortcut).toBeInViewport();
  expect(
    await mobileShortcut.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth
      );
    }),
  ).toBe(true);
  await mobileShortcut
    .getByRole("button", { name: "Open File locks" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · File locks");
  await expect(activeMailSection(page)).toContainText("File locks");
  await expect(mailPage).toContainText("Advisory file locks");

  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "locks-office-door-390.png");

  // The deliberate failure window swallowed exactly the one injected read.
  expect(injectedLeaseFailureConsoleErrors).toBe(1);
  expect(injectedLeaseFailureRequests).toBe(1);
  expect(injectedLeaseFailureResponses).toBe(1);
  expect(browserErrors).toEqual([]);
});
