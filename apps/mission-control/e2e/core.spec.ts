import { expect, test } from "./testHarness";
import {
  ASSISTANT_MODEL_ID,
  GATEWAY_URL,
  TEST_TOKEN,
  clickAdvancedNav,
  completeQuickstartLocalOnboarding,
  moveWizardToConnectionStep,
  openWizard,
  wizard,
} from "./onboardingFlow";

const CLAUDE_SETUP_TOKEN = `sk-ant-oat01-${"a".repeat(80)}`;
const CLAUDE_MODEL_ID = "claude-sonnet-4-5";

test.describe("mission-control core onboarding + crash-proofing @core", () => {
  test("settings modal uses caveman-friendly setup accordions without default scroll overload", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await completeQuickstartLocalOnboarding(page);

    await page.locator('[data-tour-id="nav-config"]').click();
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();

    const modal = page.locator(".mc-settings-modal");
    const modalBody = page.locator(".mc-settings-body");

    const modalBox = await modal.boundingBox();
    expect(modalBox?.width ?? 0).toBeGreaterThan(760);
    expect(modalBox?.width ?? 0).toBeLessThanOrEqual(920);
    await expect(page.getByText("Appearance")).toHaveCount(0);
    await expect(page.getByText("1. Connect this app")).toBeVisible();
    await expect(page.getByText("2. Choose what pages show")).toBeVisible();
    await expect(page.getByText("3. Shared assistant instructions")).toBeVisible();

    const defaultBodyLayout = await modalBody.evaluate((node) => {
      const element = node as HTMLElement;
      const style = window.getComputedStyle(element);
      return {
        overflowY: style.overflowY,
        scrollHeight: element.scrollHeight,
        clientHeight: element.clientHeight,
      };
    });

    expect(defaultBodyLayout.overflowY).toBe("auto");
    expect(defaultBodyLayout.scrollHeight - defaultBodyLayout.clientHeight).toBeLessThanOrEqual(1);

    await page.getByText("3. Shared assistant instructions").click();
    const promptField = page.locator(".mc-settings-prompt");
    await expect(promptField).toBeVisible();

    const promptMetrics = await promptField.evaluate((node) => {
      const element = node as HTMLTextAreaElement;
      const style = window.getComputedStyle(element);
      return {
        overflowY: style.overflowY,
        scrollHeight: element.scrollHeight,
        clientHeight: element.clientHeight,
      };
    });

    const promptBox = await promptField.boundingBox();

    expect(["auto", "hidden", "clip"]).toContain(promptMetrics.overflowY);
    expect(promptMetrics.clientHeight).toBeGreaterThan(160);
    expect(promptMetrics.clientHeight).toBeLessThan(280);
    expect(promptBox?.width ?? 0).toBeGreaterThan(700);
    expect(promptBox?.height ?? 0).toBeLessThan(280);
  });

  test("quick guides collapse globally and reopen only for the active page", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await completeQuickstartLocalOnboarding(page);
    const activeQuickGuide = page.locator(
      '.mc-tab-pane[style*="display: flex"] .mc-tab-help-banner'
    );

    await expect(activeQuickGuide).toBeVisible();
    await activeQuickGuide.getByRole("button", { name: "Hide quick guides" }).click();
    await expect(page.locator(".mc-tab-help-banner")).toHaveCount(0);

    await page.locator('[data-tour-id="nav-team"]').click();
    await expect(activeQuickGuide).toHaveCount(0);

    await page.getByRole("button", { name: "Show quick guide for this page" }).click();
    await expect(activeQuickGuide).toBeVisible();

    await page.locator('[data-tour-id="nav-boards"]').click();
    await expect(activeQuickGuide).toHaveCount(0);

    await page.locator('[data-tour-id="nav-team"]').click();
    await expect(activeQuickGuide).toBeVisible();
  });

  test("cockpit default layout stays spread across the screen and persists across tab changes", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await completeQuickstartLocalOnboarding(page);

    await clickAdvancedNav(page, "cockpit");
    await expect(page.locator(".mc-cockpit-grid")).toBeVisible();
    await expect(page.locator(".mc-rgl-canvas .react-grid-item")).toHaveCount(9);

    const initialMetrics = await page.locator(".mc-rgl-canvas").evaluate((node) => {
      const canvas = node as HTMLElement;
      const widgets = Array.from(canvas.querySelectorAll<HTMLElement>(".react-grid-item"));
      const topValues = widgets.map((widget) => Math.round(widget.getBoundingClientRect().top));
      return {
        widgetCount: widgets.length,
        uniqueRows: Array.from(new Set(topValues)).length,
      };
    });

    expect(initialMetrics.widgetCount).toBeGreaterThanOrEqual(9);
    expect(initialMetrics.uniqueRows).toBeLessThanOrEqual(2);

    await page.getByRole("button", { name: "Enter cockpit edit mode" }).click();
    const eventTailWidget = page
      .locator(".mc-cockpit-widget", {
        has: page.getByRole("heading", { name: "Event Tail" }),
      })
      .first();
    await eventTailWidget.getByRole("button", { name: "Move widget right" }).click();
    await page.getByRole("button", { name: "Exit cockpit edit mode" }).click();

    const storageBeforeTabChange = await page.evaluate(() =>
      window.localStorage.getItem("mc-cockpit-pages-v3")
    );

    await page.locator('[data-tour-id="nav-boards"]').click();
    await clickAdvancedNav(page, "cockpit");

    const storageAfterTabChange = await page.evaluate(() =>
      window.localStorage.getItem("mc-cockpit-pages-v3")
    );

    expect(storageAfterTabChange).toBe(storageBeforeTabChange);

    const contentArea = page.locator(".mc-content-area");
    const metrics = await contentArea.evaluate((node) => {
      const area = node as HTMLElement;
      return {
        overflowY: window.getComputedStyle(area).overflowY,
        scrollDelta: area.scrollHeight - area.clientHeight,
      };
    });

    expect(metrics.overflowY).toBe("auto");
    expect(metrics.scrollDelta).toBeLessThanOrEqual(1);
  });

  test("cockpit heals legacy stacked pages and supports horizontal resize in edit mode", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (targetPage) => {
        await targetPage.addInitScript(() => {
          window.localStorage.removeItem("mc-cockpit-pages-v1");
          window.localStorage.removeItem("mc-cockpit-pages-v2");
          window.localStorage.setItem(
            "mc-cockpit-pages-v3",
            JSON.stringify([
              {
                page_id: "page-legacy-collapsed",
                name: "Dashboard",
                widgets: [
                  { instance_id: "health-template", widget: "health", title: "Pinned Health Strip", position: { x: 0, y: 0, w: 10, h: 4 } },
                  { instance_id: "focus-template", widget: "focus", title: "Incident Queue", position: { x: 0, y: 4, w: 10, h: 4 } },
                  { instance_id: "breakers-template", widget: "breakers", title: "Breaker Radar", position: { x: 0, y: 8, w: 10, h: 4 } },
                  { instance_id: "jobs-template", widget: "jobs", title: "Scheduler Matrix", position: { x: 0, y: 12, w: 10, h: 4 } },
                  { instance_id: "channels-template", widget: "channels", title: "Channel Control", position: { x: 0, y: 16, w: 10, h: 4 } },
                  { instance_id: "profiles-template", widget: "profiles", title: "Agent Provider Routing", position: { x: 0, y: 20, w: 10, h: 4 } },
                  { instance_id: "skills-template", widget: "skills", title: "Skills Control", position: { x: 0, y: 24, w: 10, h: 4 } },
                  { instance_id: "plugins-template", widget: "plugins", title: "Plugins Control", position: { x: 0, y: 28, w: 10, h: 4 } },
                  { instance_id: "events-template", widget: "events", title: "Event Tail", position: { x: 0, y: 32, w: 10, h: 4 } },
                ],
              },
            ])
          );
        });
      },
    });

    await clickAdvancedNav(page, "cockpit");
    await expect(page.locator(".mc-rgl-canvas .react-grid-item")).toHaveCount(9);

    const healedLayout = await page.evaluate(() =>
      JSON.parse(window.localStorage.getItem("mc-cockpit-pages-v3") ?? "[]")
    );
    const healedWidgets = healedLayout[0]?.widgets ?? [];
    const distinctX = new Set(healedWidgets.map((widget: { position: { x: number } }) => widget.position.x));
    expect(distinctX.size).toBeGreaterThan(1);

    await page.getByRole("button", { name: "Enter cockpit edit mode" }).click();

    const eventTile = page
      .locator(".react-grid-item", {
        has: page.getByRole("heading", { name: "Event Tail" }),
      })
      .first();
    await expect(eventTile).toBeVisible();

    const eventHandle = eventTile.locator(".react-resizable-handle-e");
    await expect(eventHandle).toBeVisible();
    const startBox = await eventHandle.boundingBox();
    if (!startBox) {
      throw new Error("east resize handle not measurable");
    }
    await page.mouse.move(startBox.x + startBox.width / 2, startBox.y + startBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(startBox.x + 140, startBox.y + startBox.height / 2, { steps: 10 });
    await page.mouse.up();

    const resizedLayout = await page.evaluate(() =>
      JSON.parse(window.localStorage.getItem("mc-cockpit-pages-v3") ?? "[]")
    );
    const resizedEvent = (resizedLayout[0]?.widgets ?? []).find(
      (widget: { instance_id: string }) => widget.instance_id === "events-template"
    );
    expect(resizedEvent?.position.w ?? 0).toBeGreaterThan(2);
  });

  test("auto-opens onboarding, supports dismiss, and can reopen from settings", async ({ page }) => {
    await expect(await openWizard(page)).toBe(true);

    await page.getByRole("button", { name: "Dismiss (24h)" }).click();
    await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeHidden();

    await page.locator('[data-tour-id="nav-config"]').click();
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
    await page.getByRole("button", { name: "Setup Wizard" }).click();

    await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeVisible();
  });

  test("shows a create-agent handoff in Assistant when no agents exist yet", async ({ page }) => {
    await expect(await openWizard(page)).toBe(true);
    await page.getByRole("button", { name: "Dismiss (24h)" }).click();
    await expect(page.getByRole("heading", { name: "Setup Wizard" })).toBeHidden();

    await page.locator('[data-tour-id="nav-assistant"]').click();
    await expect(page.getByRole("heading", { name: "Create an agent first" })).toBeVisible();
    await expect(
      page.getByText(/Assistant chat needs one configured agent before it can route a message/i)
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Go to Team" })).toBeVisible();
  });

  test("keeps onboarding token plaintext only during active entry and does not expose it after setup", async ({
    page,
  }) => {
    await expect(await moveWizardToConnectionStep(page)).toBe(true);
    const setupWizard = wizard(page);

    const tokenField = setupWizard.getByLabel("Gateway token").first();
    await expect(tokenField).toHaveAttribute("type", "text");
    await tokenField.fill(TEST_TOKEN);
    await expect(tokenField).toHaveValue(TEST_TOKEN);

    await completeQuickstartLocalOnboarding(page, { startFromConnectionStep: true });
    await expect(page.locator("body")).not.toContainText(TEST_TOKEN);

    await page.locator('[data-tour-id="nav-config"]').click();
    await page.getByRole("button", { name: "Setup Wizard" }).click();
    const reopenedWizard = wizard(page);
    await reopenedWizard.getByRole("button", { name: "Continue" }).click();
    await reopenedWizard.getByRole("button", { name: "Continue" }).click();
    await expect(reopenedWizard.getByText("Step 3 of 6")).toBeVisible();
    await expect(reopenedWizard.getByLabel("Gateway token").first()).not.toHaveValue(TEST_TOKEN);
  });

  test("continues from connect step without manual Save + Connect click", async ({ page }) => {
    await expect(await moveWizardToConnectionStep(page)).toBe(true);
    const setupWizard = wizard(page);

    await setupWizard.getByLabel("Gateway URL").fill(GATEWAY_URL);
    await setupWizard.getByLabel("Gateway token").fill(TEST_TOKEN);
    await setupWizard.getByRole("button", { name: "Save connection + Continue" }).click();
    await expect(setupWizard.getByText("Step 4 of 6")).toBeVisible();

    await setupWizard.getByRole("button", { name: "Start new agent" }).click();
    await setupWizard.getByLabel("Agent ID").fill("assistant-continue");
    await setupWizard.getByLabel("Agent name").fill("Assistant Continue");
    await setupWizard.getByRole("radio", { name: "Local connector" }).check();
    await setupWizard
      .getByRole("combobox", { name: "Assistant model" })
      .selectOption(ASSISTANT_MODEL_ID);
    await setupWizard.getByRole("button", { name: "Apply setup + Continue" }).click();

    await expect(setupWizard.getByText("Step 5 of 6")).toBeVisible();
    await expect(
      page
        .locator(".mc-onboarding-checklist li")
        .filter({ hasText: "Connection verified" })
        .first()
    ).toHaveClass(/done/);
  });

  test("continues after manual Save + Connect even when token input is cleared", async ({ page }) => {
    await expect(await moveWizardToConnectionStep(page)).toBe(true);
    const setupWizard = wizard(page);

    await setupWizard.getByLabel("Gateway URL").fill(GATEWAY_URL);
    await setupWizard.getByLabel("Gateway token").fill(TEST_TOKEN);
    await setupWizard.getByRole("button", { name: /Save \+ Connect/ }).click();
    await expect(setupWizard.getByText(/Connection status:\s*Connected/)).toBeVisible();
    await expect(setupWizard.getByLabel("Gateway token")).toHaveValue("");

    await setupWizard.getByRole("button", { name: "Save connection + Continue" }).click();
    await expect(setupWizard.getByText("Step 4 of 6")).toBeVisible();
  });

  test("auto-verifies a Claude setup token and loads live model choices without extra hidden steps", async ({
    page,
  }) => {
    await expect(await moveWizardToConnectionStep(page)).toBe(true);
    const setupWizard = wizard(page);

    await setupWizard.getByLabel("Gateway URL").fill(GATEWAY_URL);
    await setupWizard.getByLabel("Gateway token").fill(TEST_TOKEN);
    await setupWizard.getByRole("button", { name: "Save connection + Continue" }).click();
    await expect(setupWizard.getByText("Step 4 of 6")).toBeVisible();

    await setupWizard.getByRole("button", { name: "Start new agent" }).click();
    await setupWizard.getByLabel("Agent ID").fill("assistant-claude");
    await setupWizard.getByLabel("Agent name").fill("Claude Assistant");
    await setupWizard.getByRole("radio", { name: "Anthropic (Claude)" }).check();
    await setupWizard.getByLabel("Profile name").fill("claude-primary");
    await setupWizard.getByRole("textbox", { name: "Setup token" }).fill(CLAUDE_SETUP_TOKEN);

    await expect(
      setupWizard.getByRole("combobox", { name: "Assistant model" })
    ).toHaveValue(CLAUDE_MODEL_ID);
    await expect(
      setupWizard.getByText(/carsinOS already picked .*claude-sonnet-4-5.* for you/i)
    ).toBeVisible();

    await setupWizard.getByRole("button", { name: "Apply setup + Continue" }).click();
    await expect(setupWizard.getByText("Step 5 of 6")).toBeVisible();
    await setupWizard.getByRole("button", { name: "Finish setup" }).click();

    await expect(setupWizard.getByText("Step 6 of 6")).toBeVisible();
  });

  test("connects via deterministic stub gateway and loads baseline", async ({ page }) => {
    await completeQuickstartLocalOnboarding(page);

    await expect(page.getByText("Investigate gateway health")).toBeVisible();

    await page.locator('[data-tour-id="nav-config"]').click();
    await expect(page.getByText(/Gateway:/)).toBeVisible();
    await expect(page.getByText(/Live link:/)).toBeVisible();
  });

  test("uses provider-backed model selectors in Assistant Chat instead of raw model text boxes @core", async ({
    page,
  }) => {
    await completeQuickstartLocalOnboarding(page);

    await page.locator('[data-tour-id="nav-assistant"]').click();
    await expect(page.getByRole("heading", { name: "Chat", exact: true })).toBeVisible();
    await expect(page.getByLabel("Assistant provider")).toHaveValue("ollama");
    await expect(page.getByLabel("Assistant model", { exact: true })).toHaveValue(ASSISTANT_MODEL_ID);
    await expect(page.getByRole("img", { name: /carsinOS pulls the live model list/i })).toBeVisible();
    await expect(page.getByRole("textbox", { name: "Model provider" })).toHaveCount(0);
    await expect(page.getByRole("textbox", { name: "Model ID" })).toHaveCount(0);
  });

  test("enables Strategy and follows linked work back from runtime surfaces @core", async ({
    page,
  }) => {
    await completeQuickstartLocalOnboarding(page);

    await clickAdvancedNav(page, "strategy");
    await expect(page.getByText("Strategy hub is disabled")).toBeVisible();

    await page.locator('[data-tour-id="nav-config"]').click();
    await page.getByText("2. Choose what pages show").click();
    await page.getByRole("checkbox", { name: "Strategy page" }).check();
    await page.keyboard.press("Escape");

    await clickAdvancedNav(page, "strategy");
    await expect(page.getByTestId("strategy-page")).toBeVisible();
    await expect(page.getByRole("heading", { name: "Tasks" })).toBeVisible();
    await expect(
      page
        .locator(".mc-strategy-task-list")
        .getByRole("button", { name: /Investigate gateway health/i })
        .first()
    ).toBeVisible();

    await page.locator('[data-tour-id="nav-boards"]').click();
    await page.locator(".mc-card", { hasText: "Investigate gateway health" }).first().click();
    await expect(page.locator(".mc-board-strategy-panel")).toBeVisible();
    await page.getByRole("button", { name: "Open in Strategy" }).click();
    await expect(page.getByTestId("strategy-page")).toBeVisible();
    await expect(page.locator(".mc-strategy-task-row.is-active").first()).toContainText(
      "Investigate gateway health"
    );

    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.getByRole("tab", { name: /Schedule/ }).click();
    await expect(page.getByRole("button", { name: "Open task" }).first()).toBeVisible();
    await page.getByRole("button", { name: "Open task" }).first().click();
    await expect(page.getByTestId("strategy-page")).toBeVisible();
    await expect(page.locator(".mc-strategy-task-row.is-active").first()).toContainText(
      "Investigate gateway health"
    );

    await page.locator('[data-tour-id="nav-focus"]').click();
    await expect(page.getByRole("button", { name: "Open task" }).first()).toBeVisible();
    await page.getByRole("button", { name: "Open task" }).first().click();
    await expect(page.getByTestId("strategy-page")).toBeVisible();
    await expect(page.locator(".mc-strategy-task-row.is-active").first()).toContainText(
      "Investigate gateway health"
    );
  });

  test("reset tab state preserves global connection settings", async ({ page }) => {
    await completeQuickstartLocalOnboarding(page);

    await page.getByTestId("e2e-crash-active-tab").click();
    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.locator('[data-tour-id="nav-boards"]').click();

    await expect(page.getByRole("heading", { name: "This tab crashed." })).toBeVisible();
    await page.getByRole("button", { name: "Reset tab state" }).click();
    await expect(page.getByText("Crash Recovery")).toBeHidden();

    await page.locator('[data-tour-id="nav-config"]').click();
    await expect(page.getByText(/Gateway:/)).toBeVisible();
    await expect(page.getByText(/Live link:/)).toBeVisible();
  });

  test("recovers from deterministic tab crash through tab boundary retry", async ({ page }) => {
    await completeQuickstartLocalOnboarding(page);

    await page.getByTestId("e2e-crash-active-tab").click();
    await page.locator('[data-tour-id="nav-calendar"]').click();
    await page.locator('[data-tour-id="nav-boards"]').click();

    const crashAlert = page.getByRole("alert");
    await expect(crashAlert.getByText("Crash Recovery")).toBeVisible();
    await expect(crashAlert.getByRole("heading", { name: "This tab crashed." })).toBeVisible();

    await page.getByRole("button", { name: "Retry" }).click();
    await expect(crashAlert).toBeHidden();
    await expect(page.getByText("Investigate gateway health")).toBeVisible();
  });

  test("guided tour shows explicit progress and covers events and config", async ({ page }) => {
    await completeQuickstartLocalOnboarding(page);
    const tourBubble = page.locator(".mc-tour-bubble");

    await page.locator('[data-tour-id="topbar-tour"]').click();
    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("1/13");
    await expect(page.getByRole("heading", { name: "Boards = task execution" })).toBeVisible();

    await tourBubble.getByRole("button", { name: "Next", exact: true }).click();
    await tourBubble.getByRole("button", { name: "Next", exact: true }).click();
    await tourBubble.getByRole("button", { name: "Next", exact: true }).click();

    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("4/13");
    await expect(page.getByRole("heading", { name: "Events = runtime activity" })).toBeVisible();

    for (let index = 0; index < 6; index += 1) {
      await tourBubble.getByRole("button", { name: "Next", exact: true }).click();
    }

    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("10/13");
    await expect(page.getByRole("heading", { name: "Strategy = management layer" })).toBeVisible();

    await tourBubble.getByRole("button", { name: "Next", exact: true }).click();
    await tourBubble.getByRole("button", { name: "Next", exact: true }).click();

    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("12/13");
    await expect(
      page.getByRole("heading", { name: "Config = connection + recovery controls" })
    ).toBeVisible();

    await tourBubble.getByRole("button", { name: "Next", exact: true }).click();
    await expect(page.locator(".mc-tour-progress-chip")).toHaveText("13/13");
    await expect(page.getByRole("heading", { name: "Command palette" })).toBeVisible();
  });
});
