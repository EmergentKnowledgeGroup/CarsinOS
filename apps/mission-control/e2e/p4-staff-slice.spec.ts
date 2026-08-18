import { expect, test, type Page } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

/**
 * The Staff Directory room is always visible; enabling the Strategy hub
 * lights the strategy-gated Presets and Org surfaces on the Team page.
 */
async function enableStrategyPage(page: Page): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  await page.getByText("2. Choose what pages show").click();
  await page.getByRole("checkbox", { name: "Strategy page" }).check();
  await page.keyboard.press("Escape");
}

/**
 * P4 Staff Directory room slice: Team keeps its Agents, People & Routing,
 * and strategy-gated Presets/Org surfaces with their create/edit/remove,
 * routing, role-card, and memory-binding behavior; it gains the
 * registry-backed Pin to Office affordance; and the pinned shortcut walks
 * back to the Staff Directory room by stable id — refusing honestly when
 * the floor is disabled. Verified at desktop and 390px with console-error
 * and horizontal-overflow assertions.
 */

test("@core @p4-staff staff parity, pin-to-office, and the office shortcut hold at desktop and 390px", async ({
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

  // The Staff Directory room lights exactly one lamp by stable id.
  await enableStrategyPage(page);
  const activeRooms = page.locator(".mc-nav-item-active");
  await page.locator('button[title="2F · Staff Directory"]').click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Staff Directory");
  await expect(page.getByTestId("team-page")).toBeVisible();

  // Parity: every Team surface opens with its own real content. Only
  // authoritative persistent agents appear — no task-worker labels.
  const teamPage = page.getByTestId("team-page");
  await expect(teamPage).toContainText("Operations Director");
  await expect(teamPage).toContainText("Memory lane: mno-default");
  await expect(
    teamPage.getByRole("button", { name: "Role Card" }).first(),
  ).toBeVisible();
  await teamPage
    .locator(".mc-team-card")
    .filter({ hasText: "Local Assistant" })
    .getByRole("button", { name: "Role Card" })
    .click();
  const roleCard = page.getByRole("dialog", {
    name: "Local Assistant — Role Card",
  });
  await expect(roleCard).toContainText("Manager Chain");
  await expect(roleCard).toContainText("Root");
  await page.keyboard.press("Escape");
  await expect(roleCard).toHaveCount(0);

  // People & Routing is now an exact stable-room handoff to Basement ·
  // Directory / Front Desk. The affordance stays reachable; the authoritative
  // editable surface lives in the Directory room and lights its lamp.
  await teamPage.getByRole("button", { name: "People & Routing" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute(
    "title",
    "BF · Directory / Front Desk",
  );
  const mailPage = page.getByTestId("mail-page");
  await expect(mailPage).toContainText("People And Routing");
  await expect(mailPage).toContainText("local-operator");
  await mailPage.getByRole("button", { name: "Routing Setup" }).click();
  await expect(mailPage).toContainText("Humans currently active in routing.");

  // Returning to the Staff room resumes the Team parity checks.
  await page.locator('button[title="2F · Staff Directory"]').click();
  await expect(activeRooms).toHaveAttribute("title", "2F · Staff Directory");

  await teamPage.getByRole("button", { name: "Presets", exact: true }).click();
  await expect(teamPage).toContainText("Bootstrap Presets");
  await teamPage.getByRole("button", { name: "Org", exact: true }).click();
  await expect(teamPage).toContainText("Org View");
  await expect(teamPage).toContainText("Local Assistant");
  await teamPage.getByRole("button", { name: "Agents", exact: true }).click();

  // A full default canvas refuses the fourth shortcut visibly and leaves the
  // exact persisted config untouched.
  const pin = page.getByRole("button", { name: "Pin Staff Directory to Office" });
  await expect(pin).toBeVisible();
  const fullCanvasConfig = await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = [
      { id: "needs-you", size: "l", visible: true },
      { id: "in-motion", size: "m", visible: true },
      { id: "done", size: "m", visible: true },
      { id: "next", size: "s", visible: true },
      { id: "boards", size: "s", visible: true },
      { id: "calendar", size: "s", visible: true },
      { id: "strategy", size: "s", visible: true },
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
    path: "../../runtime/qa/p4-staff-slice/staff-full-canvas-refusal.png",
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
        (placement: { id?: string }) => placement.id === "staff",
      );
    }),
  ).toBe(false);

  // Free one small default block. Pin now succeeds, and pinning again reports
  // the truth instead of pretending a second change.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "next" ? { ...placement, visible: false } : placement,
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
    path: "../../runtime/qa/p4-staff-slice/staff-pinned-desktop.png",
    fullPage: true,
  });

  // The pinned shortcut appears on the Office canvas and names its floor.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-staff");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Trenches");

  // The mounted Office consumes config events live; it does not require a
  // route remount before a Staff shortcut hide/show is truthful.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "staff" ? { ...placement, visible: false } : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-staff")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "staff" ? { ...placement, visible: true } : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-staff")).toBeVisible();
  await shortcut.scrollIntoViewIfNeeded();
  await shortcut.screenshot({
    path: "../../runtime/qa/p4-staff-slice/office-shortcut-block.png",
  });

  // Opening the shortcut lands back in the Staff Directory room, lamp included.
  await shortcut.getByRole("button", { name: "Open Staff Directory" }).click();
  await expect(page.getByTestId("team-page")).toBeVisible();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "2F · Staff Directory");

  // The pin is config: it survives a full reload. Browser runs keep the
  // gateway token in memory, so onboarding runs again after the reload.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("office-block-staff")).toBeVisible();

  // If the destination floor is later disabled, the persisted door stays
  // visible but refuses honestly instead of becoming a silent dead button.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.floorOverrides = { ...config.floorOverrides, trenches: { hidden: true } };
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="2F · Staff Directory"]')).toHaveCount(0);
  await page
    .getByTestId("office-block-staff")
    .getByRole("button", { name: "Open Staff Directory" })
    .click();
  await expect(
    page.getByTestId("office-block-staff").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await page.screenshot({
    path: "../../runtime/qa/p4-staff-slice/staff-disabled-door.png",
    fullPage: true,
  });

  // Restore the floor for the narrow-width proof.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    if (config.floorOverrides) delete config.floorOverrides.trenches;
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.locator('button[title="2F · Staff Directory"]')).toHaveCount(1);
  await expect(
    page.getByTestId("office-block-staff").getByRole("status"),
  ).toHaveCount(0);
  await expect(
    page
      .getByTestId("office-block-staff")
      .getByRole("button", { name: "Open Staff Directory" }),
  ).toBeVisible();
  await page.screenshot({
    path: "../../runtime/qa/p4-staff-slice/staff-restored-door.png",
    fullPage: true,
  });

  // Narrow width: the Staff Directory room keeps a readable mark, the pin
  // stays reachable, and nothing overflows horizontally.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('[data-tour-id="nav-team"]').click();
  const mobileStaffRoom = page.locator('button[title="2F · Staff Directory"]');
  await expect(mobileStaffRoom).toHaveClass(/mc-nav-item-active/);
  await expect(mobileStaffRoom.locator(".mc-nav-room-mark")).toHaveText("SD");
  await expect(
    page.getByRole("button", { name: "Pin Staff Directory to Office" }),
  ).toBeVisible();
  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await page.screenshot({
    path: "../../runtime/qa/p4-staff-slice/staff-390.png",
    fullPage: true,
  });

  expect(browserErrors).toEqual([]);
});
