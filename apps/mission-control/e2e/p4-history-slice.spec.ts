import { expect, test, type Page } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * The History & Receipts room rides the optional Runbook page: its lamp only
 * exists in the elevator while the Runbook page is enabled in Config.
 */
async function setRunbookPage(page: Page, enabled: boolean): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  const checkbox = page.getByRole("checkbox", { name: "Runbook page" });
  if (!(await checkbox.isVisible())) {
    await page.getByText("2. Choose what pages show").click();
  }
  if (enabled) {
    await checkbox.check();
  } else {
    await checkbox.uncheck();
  }
  await page.keyboard.press("Escape");
}

/**
 * P4 History & Receipts room slice: Runbook keeps its browse filters, status
 * summary lenses, refresh, pagination, and detail Overview/Flow/Artifacts/
 * History surfaces with authoritative source facts, warnings, linked
 * entities, and actions; it gains the registry-backed Pin to Office
 * affordance on the ready surface only; and the pinned shortcut walks back
 * to the room by stable id — refusing honestly while the Runbook page is
 * disabled and clearing that refusal the moment it is restored. Verified at
 * desktop and 390px with console-error and horizontal-overflow assertions.
 */

test("@core @p4-history history parity, pin-to-office, and the office shortcut hold at desktop and 390px", async ({
  page,
}) => {
  test.setTimeout(90_000);
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

  // The room lamp does not exist until the Runbook page is enabled.
  await expect(
    page.locator('button[title="2F · History & Receipts"]'),
  ).toHaveCount(0);
  await setRunbookPage(page, true);
  const activeRooms = page.locator(".mc-nav-item-active");
  await page.locator('button[title="2F · History & Receipts"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · History & Receipts");
  await expect(page.getByTestId("runbook-page")).toBeVisible();

  // Parity: browse keeps its live status lenses and every filter operable.
  const runbookPage = page.getByTestId("runbook-page");
  const listItems = page.locator(".mc-runbook-list-item");
  await expect(listItems).toHaveCount(4);
  await page
    .locator(".mc-runbook-summary-card", { hasText: "Waiting" })
    .click();
  await expect(listItems).toHaveCount(1);
  await expect(listItems).toContainText(
    "Approval gate for incident recovery session",
  );
  await page.getByRole("button", { name: "Reset filters" }).click();
  await expect(listItems).toHaveCount(4);
  const filters = runbookPage.locator(".mc-runbook-filter-bar");
  await filters.locator("input").fill("incident recovery");
  await expect(listItems).toHaveCount(1);
  await expect(listItems).toContainText(
    "Approval gate for incident recovery session",
  );
  await page.getByRole("button", { name: "Reset filters" }).click();
  await filters.locator("select").nth(0).selectOption("scheduled_job_run");
  await expect(listItems).toHaveCount(1);
  await expect(listItems).toContainText("Gateway heartbeat");
  await page.getByRole("button", { name: "Reset filters" }).click();
  await filters.locator("select").nth(1).selectOption("waiting");
  await expect(listItems).toHaveCount(1);
  await expect(listItems).toContainText(
    "Approval gate for incident recovery session",
  );
  await page.getByRole("button", { name: "Reset filters" }).click();
  await filters.locator("select").nth(2).selectOption("agent-root");
  await expect(listItems).toHaveCount(1);
  await expect(listItems).toContainText(
    "Approval gate for incident recovery session",
  );
  await page.getByRole("button", { name: "Reset filters" }).click();
  await page.getByRole("button", { name: "Refresh" }).click();
  await expect(listItems).toHaveCount(4);

  // Parity: a real detail proves Overview, Flow, Artifacts, and History.
  await page
    .locator(".mc-runbook-list-item", {
      hasText: "Approval gate for incident recovery session",
    })
    .click();
  await expect(runbookPage).toContainText("Back to list");
  await expect(runbookPage).toContainText(
    "Operator approval is still pending for the next command.",
  );
  await expect(
    page.getByRole("button", { name: "Review approval" }),
  ).toBeVisible();
  const overview = page.locator(".mc-runbook-tab-content");
  await expect(overview.locator(".mc-runbook-source-item")).toHaveCount(2);
  await expect(runbookPage.locator(".mc-runbook-warning-card")).toHaveCount(1);
  await page.getByRole("button", { name: "Flow (3)" }).click();
  const flow = page.locator(".mc-runbook-flow");
  await expect(flow.locator(".mc-runbook-step")).toHaveCount(3);
  await expect(flow).toContainText("Await approval");
  await expect(flow).toContainText("Execute run");
  await page.getByRole("button", { name: "Artifacts", exact: true }).click();
  await expect(page.locator(".mc-runbook-tab-content")).toContainText("2 item(s)");
  await page.getByRole("button", { name: "History (2)" }).click();
  const history = page.locator(".mc-runbook-history");
  await expect(history.locator(".mc-runbook-history-item")).toHaveCount(2);
  await expect(history).toContainText("Approval requested");
  await expect(history).toContainText("Session created");
  const detailMetrics = await page.locator(".mc-content-area").evaluate((node) => {
    const area = node as HTMLElement;
    return {
      scrollDelta: area.scrollHeight - area.clientHeight,
      horizontalOverflow: area.scrollWidth > area.clientWidth,
    };
  });
  expect(detailMetrics.scrollDelta).toBeLessThanOrEqual(10);
  expect(detailMetrics.horizontalOverflow).toBe(false);
  await page.getByRole("button", { name: "Overview" }).click();
  await page.getByRole("button", { name: "Review approval" }).click();
  await expect(page.locator('[data-tour-id="nav-focus"]')).toHaveClass(
    /mc-nav-item-active/,
  );
  await expect(page.getByText(/Approval requested:/).first()).toBeVisible();
  await page.locator('button[title="2F · History & Receipts"]').click();
  await page.getByRole("button", { name: "Back to list" }).click();
  await expect(runbookPage).toContainText("Browse Runbooks");

  // A full default canvas refuses the fifth shortcut visibly and leaves the
  // exact persisted config untouched.
  const pin = page.getByRole("button", {
    name: "Pin History & Receipts to Office",
  });
  await expect(pin).toBeVisible();
  const fullCanvasConfig = await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = [
      { id: "needs-you", size: "l", visible: true },
      { id: "in-motion", size: "m", visible: true },
      { id: "done", size: "m", visible: true },
      { id: "next", size: "s", visible: false },
      { id: "boards", size: "s", visible: true },
      { id: "calendar", size: "s", visible: true },
      { id: "strategy", size: "s", visible: true },
      { id: "staff", size: "s", visible: true },
    ];
    const serialized = JSON.stringify(config);
    localStorage.setItem(key, serialized);
    window.dispatchEvent(new Event("mc-glass-config-changed"));
    return serialized;
  });
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note.is-error")).toHaveText(
    "Pinning this would exceed the six-column, four-row Office canvas.",
  );
  await page.screenshot({
    path: "../../runtime/qa/p4-history-slice/history-full-canvas-refusal.png",
    fullPage: true,
  });
  expect(
    await page.evaluate(() => localStorage.getItem("mc-glass-config-v1")),
  ).toBe(fullCanvasConfig);
  expect(
    await page.evaluate(() => {
      const config = JSON.parse(
        localStorage.getItem("mc-glass-config-v1") ?? "{}",
      );
      return config.layout?.some(
        (placement: { id?: string }) => placement.id === "history",
      );
    }),
  ).toBe(false);

  // Free one medium default block. Pin now succeeds, and pinning again reports
  // the truth instead of pretending a second change.
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
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "Already on the Office canvas.",
  );
  await page.screenshot({
    path: "../../runtime/qa/p4-history-slice/history-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas and names its floor.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-history");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Trenches");
  await shortcut.scrollIntoViewIfNeeded();
  await shortcut.screenshot({
    path: "../../runtime/qa/p4-history-slice/office-shortcut-block.png",
  });

  // The mounted Office consumes config events live; a Staff-slice lesson —
  // no route remount may be required for a shortcut hide/show to be truthful.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "history"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-history")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "history"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-history")).toBeVisible();

  // Opening the shortcut lands back in the room, lamp included.
  await shortcut
    .getByRole("button", { name: "Open History & Receipts" })
    .click();
  await expect(page.getByTestId("runbook-page")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · History & Receipts");

  // The pin is config: it survives a full reload. Browser runs keep the
  // gateway token in memory, so onboarding runs again after the reload.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-history");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open History & Receipts" })
    .click();
  await expect(page.getByTestId("runbook-page")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · History & Receipts");

  // Turning the Runbook page off removes the room lamp; the persisted door
  // stays visible but refuses honestly instead of becoming silently dead.
  await setRunbookPage(page, false);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(
    page.locator('button[title="2F · History & Receipts"]'),
  ).toHaveCount(0);
  await page
    .getByTestId("office-block-history")
    .getByRole("button", { name: "Open History & Receipts" })
    .click();
  await expect(
    page.getByTestId("office-block-history").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await page.screenshot({
    path: "../../runtime/qa/p4-history-slice/history-disabled-door.png",
    fullPage: true,
  });

  // Restoring the Runbook page clears the stale refusal without another
  // click: an enabled door may not stay labeled unavailable.
  await setRunbookPage(page, true);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(
    page.locator('button[title="2F · History & Receipts"]'),
  ).toHaveCount(1);
  await expect(
    page.getByTestId("office-block-history").getByRole("status"),
  ).toHaveCount(0);
  await expect(
    page
      .getByTestId("office-block-history")
      .getByRole("button", { name: "Open History & Receipts" }),
  ).toBeVisible();
  await page.screenshot({
    path: "../../runtime/qa/p4-history-slice/history-restored-door.png",
    fullPage: true,
  });

  // Narrow width: the room keeps a readable mark, the pin stays reachable,
  // and nothing overflows horizontally.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-runbook"]').click();
  const mobileHistoryRoom = page.locator(
    'button[title="2F · History & Receipts"]',
  );
  await expect(mobileHistoryRoom).toHaveClass(/mc-nav-item-active/);
  await expect(mobileHistoryRoom.locator(".mc-nav-room-mark")).toHaveText(
    "HR",
  );
  await expect(
    page.getByRole("button", { name: "Pin History & Receipts to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/p4-history-slice/history-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
