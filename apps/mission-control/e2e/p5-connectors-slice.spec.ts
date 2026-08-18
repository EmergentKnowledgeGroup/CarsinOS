import { expect, test, type Page } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * Both Basement rooms on the connectors route ride the optional Connectors
 * page: their lamps only exist while it is enabled in Config.
 */
async function setConnectorsPage(page: Page, enabled: boolean): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  const checkbox = page.getByRole("checkbox", { name: "Connectors page" });
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

function activeConnectorsTab(page: Page) {
  return page.locator(".mc-connectors-tab-bar button.active");
}

/**
 * P5 Basement · Connectors room slice: the Connectors and Setup rooms share
 * one route but keep distinct stable identities — Connectors lands on
 * connector management (Registry), Setup lands on Setup, internal tab
 * clicks never move the lamp, and only the Connectors room offers the
 * registry-backed Pin to Office. The pinned shortcut walks back by stable
 * id, refuses honestly while the Connectors page is disabled, and clears
 * that refusal the moment it is restored. Verified at desktop and 390px
 * with console-error and horizontal-overflow assertions.
 */

test("@core @p5-connectors connectors/setup room identity, pin-to-office, and the office shortcut hold at desktop and 390px", async ({
  page,
}) => {
  test.setTimeout(90_000);
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

  await completeQuickstartLocalOnboarding(page);

  // Setup is the recovery authority and remains available independently;
  // only the optional Connectors room waits for its feature switch.
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(0);
  await expect(page.locator('button[title="BF · Setup"]')).toHaveCount(1);
  await setConnectorsPage(page, true);
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(1);
  await expect(page.locator('button[title="BF · Setup"]')).toHaveCount(1);

  // The Connectors room lights exactly one lamp and lands on management.
  const activeRooms = page.locator(".mc-nav-item-active");
  await page.locator('button[title="BF · Connectors"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await expect(page.getByTestId("connectors-page")).toBeVisible();
  await expect(activeConnectorsTab(page)).toHaveText(/^Registry/);
  await expect(page.getByTestId("connectors-page")).toContainText(
    "Installed registry",
  );

  // The Setup room is its own stable identity and lands on its distinct
  // product surface, not the connector tabs; it offers no Connectors pin —
  // one door must not ambiguously open both rooms.
  await page.locator('button[title="BF · Setup"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Setup");
  await expect(page.getByTestId("setup-room-page")).toBeVisible();
  await expect(page.getByTestId("setup-room-page")).toContainText(
    "Gateway connection",
  );
  await expect(page.locator(".mc-connectors-tab-bar")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Pin Connectors to Office" }),
  ).toHaveCount(0);

  // Connector quick setup stays reachable inside the Connectors room, and
  // internal tab clicks never move the lamp.
  await page.locator('button[title="BF · Connectors"]').click();
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await expect(activeConnectorsTab(page)).toHaveText(/^Registry/);
  await page
    .locator(".mc-connectors-tab-bar")
    .getByRole("button", { name: "Setup", exact: true })
    .click();
  await expect(activeConnectorsTab(page)).toHaveText("Setup");
  await expect(page.getByTestId("connectors-page")).toContainText(
    "Quick Setup",
  );
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await page
    .locator(".mc-connectors-tab-bar")
    .getByRole("button", { name: /^Catalog/ })
    .click();
  await expect(activeConnectorsTab(page)).toHaveText(/^Catalog/);
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");

  // Returning after visiting the Setup room is a real room change: it
  // relands on Registry and offers the pin.
  await page.locator('button[title="BF · Setup"]').click();
  await expect(page.getByTestId("setup-room-page")).toBeVisible();
  await page.locator('button[title="BF · Connectors"]').click();
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await expect(activeConnectorsTab(page)).toHaveText(/^Registry/);
  const pin = page.getByRole("button", { name: "Pin Connectors to Office" });
  await expect(pin).toBeVisible();

  // A full default canvas refuses the Basement shortcut visibly and leaves
  // the exact persisted config untouched.
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
    path: "../../runtime/qa/p5-connectors-slice/connectors-full-canvas-refusal.png",
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
        (placement: { id?: string }) => placement.id === "connectors",
      );
    }),
  ).toBe(false);

  // Free one medium default block. Pin now succeeds, and pinning again
  // reports the truth instead of pretending a second change.
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
    path: "../../runtime/qa/p5-connectors-slice/connectors-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas and names its floor.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-connectors");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await shortcut.scrollIntoViewIfNeeded();
  await shortcut.screenshot({
    path: "../../runtime/qa/p5-connectors-slice/office-shortcut-block.png",
  });

  // The mounted Office consumes config events live; no route remount may be
  // required for a shortcut hide/show to be truthful.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "connectors"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-connectors")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "connectors"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-connectors")).toBeVisible();

  // Opening the shortcut lands back in the Connectors room by stable id —
  // lamp, landing surface, and pin included.
  await shortcut.getByRole("button", { name: "Open Connectors" }).click();
  await expect(page.getByTestId("connectors-page")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await expect(activeConnectorsTab(page)).toHaveText(/^Registry/);

  // The pin is config: it survives a full reload. Browser runs keep the
  // gateway token in memory, so onboarding runs again after the reload.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-connectors");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open Connectors" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await expect(activeConnectorsTab(page)).toHaveText(/^Registry/);

  // Turning the Connectors page off removes only its lamp; Setup remains
  // available to restore it, while the persisted Connectors door refuses.
  await setConnectorsPage(page, false);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(0);
  await expect(page.locator('button[title="BF · Setup"]')).toHaveCount(1);
  await page
    .getByTestId("office-block-connectors")
    .getByRole("button", { name: "Open Connectors" })
    .click();
  await expect(
    page.getByTestId("office-block-connectors").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await page.screenshot({
    path: "../../runtime/qa/p5-connectors-slice/connectors-disabled-door.png",
    fullPage: true,
  });

  // Restoring the Connectors page clears the stale refusal without another
  // click: an enabled door may not stay labeled unavailable.
  await setConnectorsPage(page, true);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(1);
  await expect(
    page.getByTestId("office-block-connectors").getByRole("status"),
  ).toHaveCount(0);
  await expect(
    page
      .getByTestId("office-block-connectors")
      .getByRole("button", { name: "Open Connectors" }),
  ).toBeVisible();
  await page.screenshot({
    path: "../../runtime/qa/p5-connectors-slice/connectors-restored-door.png",
    fullPage: true,
  });

  // Narrow width: both rooms keep readable marks, the pin stays reachable,
  // and nothing overflows horizontally.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-connectors"]').click();
  const mobileConnectorsRoom = page.locator('button[title="BF · Connectors"]');
  await expect(mobileConnectorsRoom).toHaveClass(/mc-nav-item-active/);
  const connectorsMark = mobileConnectorsRoom.locator(".mc-nav-room-mark");
  const setupMark = page.locator(
    'button[title="BF · Setup"] .mc-nav-room-mark',
  );
  await expect(connectorsMark).toHaveText("C");
  await expect(connectorsMark).toBeVisible();
  await expect(setupMark).toHaveText("S");
  await expect(setupMark).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Pin Connectors to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  const marksInViewport = await Promise.all(
    [connectorsMark, setupMark].map((mark) =>
      mark.evaluate((node) => {
        const rect = node.getBoundingClientRect();
        return (
          rect.width > 0 &&
          rect.height > 0 &&
          rect.left >= 0 &&
          rect.right <= window.innerWidth
        );
      }),
    ),
  );
  expect(marksInViewport).toEqual([true, true]);
  const tabGeometry = await page
    .locator(".mc-connectors-tab-bar")
    .evaluate((nav) => {
      const navRect = nav.getBoundingClientRect();
      return Array.from(nav.querySelectorAll("button")).map((button) => {
        const rect = button.getBoundingClientRect();
        return {
          label: button.textContent?.trim() ?? "",
          visible:
            rect.width > 0 &&
            rect.height > 0 &&
            rect.left >= navRect.left &&
            rect.right <= navRect.right &&
            rect.top >= navRect.top &&
            rect.bottom <= navRect.bottom,
        };
      });
    });
  expect(tabGeometry).toHaveLength(5);
  expect(tabGeometry.map((tab) => tab.label)).toEqual([
    "Setup",
    "Catalog (3)",
    "Import",
    "Registry (0)",
    "Manage",
  ]);
  expect(tabGeometry.every((tab) => tab.visible)).toBe(true);
  await page.screenshot({
    path: "../../runtime/qa/p5-connectors-slice/connectors-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
