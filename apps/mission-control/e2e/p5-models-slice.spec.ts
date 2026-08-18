import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

function activeTeamSection(page: import("./testHarness").Page) {
  return page
    .getByTestId("team-page")
    .locator(".mc-page-section-tabs button.mc-page-section-btn-active");
}

async function dismissVisibleToasts(page: import("./testHarness").Page) {
  await page.locator(".mc-toast-dismiss").evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLButtonElement).click();
    }
  });
}

/**
 * P5 Basement · Models & Providers room slice: the Staff and Models rooms
 * share the team route but keep distinct stable identities — Staff lands on
 * Agents, Models lands on the new bounded read over authoritative provider
 * capability, auth-profile, model-discovery, and assignment facts. Internal
 * section clicks never move the lamp, each room offers only its own pin,
 * and the pinned shortcut walks back by stable id with the full honest
 * refusal/restore lifecycle. Verified at desktop and 390px with
 * console-error, requestfailed, and inner-rect containment assertions.
 */

test("@core @p5-models staff/models room identity, the models surface, pin-to-office, and the office shortcut hold at desktop and 390px", async ({
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

  // Both rooms on the shared team route exist with distinct identities.
  await expect(page.locator('button[title="2F · Staff Directory"]')).toHaveCount(1);
  await expect(
    page.locator('button[title="BF · Models & Providers"]'),
  ).toHaveCount(1);

  // The Models room lights exactly one lamp and lands on the Models surface.
  const activeRooms = page.locator(".mc-nav-item-active");
  await page.locator('button[title="BF · Models & Providers"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Models & Providers",
  );
  await expect(page.getByTestId("team-page")).toBeVisible();
  await expect(activeTeamSection(page)).toHaveText("Models & Providers");
  await expect(page.getByTestId("team-page")).not.toContainText(
    "Meet Your Agents",
  );

  // The surface presents authoritative facts honestly: capability chips,
  // discovered models with real assignments, and an explicit no-profiles
  // state instead of a pretend-healthy one.
  const ollamaCard = page.getByTestId("models-provider-ollama");
  await expect(ollamaCard).toBeVisible();
  await expect(ollamaCard).toContainText("streaming");
  await expect(ollamaCard).toContainText("Context window: unknown");
  await expect(ollamaCard).toContainText(
    "No configured profiles for this provider.",
  );
  await expect(ollamaCard).toContainText("qwen3.5-9b-instruct");
  await expect(ollamaCard).toContainText("in use: Root, Local Assistant");
  await expect(ollamaCard).toContainText("Assigned agents");
  await expect(page.getByTestId("models-provider-lmstudio")).toContainText(
    "No agents currently assigned.",
  );

  // The Staff room stays its own identity and lands on Agents.
  await page.locator('button[title="2F · Staff Directory"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Staff Directory");
  await expect(activeTeamSection(page)).toHaveText("Agents");
  await expect(page.getByTestId("team-page")).toContainText(
    "Meet Your Agents",
  );

  // Internal section clicks never move the lamp, and each room offers only
  // its own pin — one door must not ambiguously open both rooms.
  await page
    .getByTestId("team-page")
    .locator(".mc-page-section-tabs")
    .getByRole("button", { name: "Models & Providers" })
    .click();
  await expect(activeTeamSection(page)).toHaveText("Models & Providers");
  await expect(activeRooms).toHaveAttribute("title", "2F · Staff Directory");
  await expect(
    page.getByRole("button", { name: "Pin Staff Directory to Office" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Pin Models & Providers to Office" }),
  ).toHaveCount(0);

  // Returning to the Models room is a real room change: it relands on the
  // Models surface and swaps to the Models-only pin.
  await page.locator('button[title="BF · Models & Providers"]').click();
  await expect(activeTeamSection(page)).toHaveText("Models & Providers");
  await expect(
    page.getByRole("button", { name: "Pin Staff Directory to Office" }),
  ).toHaveCount(0);
  const pin = page.getByRole("button", {
    name: "Pin Models & Providers to Office",
  });
  await expect(pin).toBeVisible();

  // A full default canvas refuses the shortcut visibly and leaves the exact
  // persisted config untouched.
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
  expect(
    await page.evaluate(() => localStorage.getItem("mc-glass-config-v1")),
  ).toBe(fullCanvasConfig);
  expect(
    await page.evaluate(() => {
      const config = JSON.parse(
        localStorage.getItem("mc-glass-config-v1") ?? "{}",
      );
      return config.layout?.some(
        (placement: { id?: string }) => placement.id === "models",
      );
    }),
  ).toBe(false);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-models-slice/models-full-canvas-refusal.png",
    fullPage: true,
  });

  // Free one medium default block. Pin now succeeds, and a repeat pin is
  // byte-identical config plus the honest already-pinned note.
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
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-models-slice/models-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas and names its floor.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-models");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await shortcut.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-models-slice/office-shortcut-block.png",
    fullPage: true,
  });

  // The mounted Office consumes config events live; no route remount may be
  // required for a shortcut hide/show to be truthful.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "models"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-models")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "models"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-models")).toBeVisible();

  // Opening the shortcut lands back in the Models room by stable id — lamp,
  // landing surface, and Models-only pin included.
  await shortcut
    .getByRole("button", { name: "Open Models & Providers" })
    .click();
  await expect(page.getByTestId("team-page")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Models & Providers",
  );
  await expect(activeTeamSection(page)).toHaveText("Models & Providers");

  // The pin is config: it survives a full reload, and the shortcut still
  // opens the exact room and landing surface afterwards.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-models");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open Models & Providers" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Models & Providers",
  );
  await expect(activeTeamSection(page)).toHaveText("Models & Providers");

  // Hiding the Basement floor removes the lamp; the persisted door stays
  // visible but refuses honestly instead of dying silently.
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
  await expect(
    page.locator('button[title="BF · Models & Providers"]'),
  ).toHaveCount(0);
  await page
    .getByTestId("office-block-models")
    .getByRole("button", { name: "Open Models & Providers" })
    .click();
  await expect(
    page.getByTestId("office-block-models").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-models-slice/models-disabled-door.png",
    fullPage: true,
  });

  // Restoring the floor clears the stale refusal without another click.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.basement;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(
    page.locator('button[title="BF · Models & Providers"]'),
  ).toHaveCount(1);
  await expect(
    page.getByTestId("office-block-models").getByRole("status"),
  ).toHaveCount(0);
  await expect(
    page
      .getByTestId("office-block-models")
      .getByRole("button", { name: "Open Models & Providers" }),
  ).toBeVisible();
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-models-slice/models-restored-door.png",
    fullPage: true,
  });

  // Narrow width: both room marks stay readable inside the viewport, every
  // section tab stays inside its own container, the pin stays reachable,
  // and nothing overflows horizontally.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('button[title="BF · Models & Providers"]').click();
  const mobileModelsRoom = page.locator(
    'button[title="BF · Models & Providers"]',
  );
  await expect(mobileModelsRoom).toHaveClass(/mc-nav-item-active/);
  const mobileModelsMark = mobileModelsRoom.locator(".mc-nav-room-mark");
  const mobileStaffMark = page.locator(
    'button[title="2F · Staff Directory"] .mc-nav-room-mark',
  );
  await expect(mobileModelsMark).toBeVisible();
  await expect(mobileModelsMark).toHaveText("MP");
  await expect(mobileStaffMark).toBeVisible();
  await expect(mobileStaffMark).toHaveText("SD");
  const marksInViewport = await page.evaluate(() => {
    const titles = ["BF · Models & Providers", "2F · Staff Directory"];
    return titles.map((title) => {
      const mark = document.querySelector(
        `button[title="${title}"] .mc-nav-room-mark`,
      );
      if (!mark) return false;
      const rect = mark.getBoundingClientRect();
      const style = window.getComputedStyle(mark);
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        style.display !== "none" &&
        style.visibility !== "hidden" &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth &&
        rect.top >= 0 &&
        rect.bottom <= window.innerHeight
      );
    });
  });
  expect(marksInViewport).toEqual([true, true]);
  const mobileTabs = page
    .getByTestId("team-page")
    .locator(".mc-page-section-tabs button");
  await expect(mobileTabs).toHaveCount(3);
  await expect(mobileTabs).toHaveText([
    "Agents",
    "People & Routing",
    "Models & Providers",
  ]);
  for (const tab of await mobileTabs.all()) {
    await expect(tab).toBeVisible();
  }
  const tabsContained = await page.evaluate(() => {
    const bar = document.querySelector(
      '[data-testid="team-page"] .mc-page-section-tabs',
    );
    if (!bar) return null;
    const barRect = bar.getBoundingClientRect();
    const barStyle = window.getComputedStyle(bar);
    if (
      barRect.width <= 0 ||
      barRect.height <= 0 ||
      barStyle.display === "none" ||
      barStyle.visibility === "hidden"
    ) {
      return null;
    }
    return Array.from(bar.querySelectorAll("button")).map((button) => {
      const rect = button.getBoundingClientRect();
      const style = window.getComputedStyle(button);
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        style.display !== "none" &&
        style.visibility !== "hidden" &&
        rect.left >= barRect.left - 1 &&
        rect.right <= barRect.right + 1 &&
        rect.top >= barRect.top - 1 &&
        rect.bottom <= barRect.bottom + 1
      );
    });
  });
  expect(tabsContained).not.toBeNull();
  expect(tabsContained).not.toContain(false);
  await expect(
    page.getByRole("button", { name: "Pin Models & Providers to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await dismissVisibleToasts(page);
  await page.screenshot({
    path: "../../runtime/qa/p5-models-slice/models-390.png",
    fullPage: true,
  });

  // The provider grid must also shrink below its former 300px hard minimum
  // when shell chrome leaves a narrower nested content area.
  await page.setViewportSize({ width: 320, height: 844 });
  const providerCardsContained = await page.evaluate(() => {
    const grid = document.querySelector(".mc-models-provider-grid");
    if (!grid) return null;
    const gridRect = grid.getBoundingClientRect();
    const cards = Array.from(
      grid.querySelectorAll(".mc-models-provider-card"),
    );
    if (gridRect.width <= 0 || cards.length === 0) return null;
    return cards.map((card) => {
      const rect = card.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= gridRect.left - 1 &&
        rect.right <= gridRect.right + 1
      );
    });
  });
  expect(providerCardsContained).not.toBeNull();
  expect(providerCardsContained).not.toContain(false);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth > window.innerWidth,
    ),
  ).toBe(false);

  expect(browserErrors).toEqual([]);
});
