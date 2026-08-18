import { fileURLToPath } from "node:url";

import { expect, test, type Page } from "./testHarness";
import {
  GATEWAY_URL,
  TEST_TOKEN,
  completeQuickstartLocalOnboarding,
} from "./onboardingFlow";

const ROTATED_TOKEN = "rotated-token-002";
const GATEWAY_SETTINGS_KEY = "mc-gateway-settings";
const GATEWAY_TOKEN_KEY = "mc-gateway-token";

function qaArtifact(name: string) {
  return fileURLToPath(
    new URL(`../../../runtime/qa/p5-setup-slice/${name}`, import.meta.url),
  );
}

function setupRoom(page: Page) {
  return page.getByTestId("setup-room-page");
}

function settingsModal(page: Page) {
  return page.locator(".mc-settings-modal");
}

async function dismissVisibleToasts(page: Page) {
  await page.locator(".mc-toast-dismiss").evaluateAll((buttons) => {
    for (const button of buttons) {
      (button as HTMLButtonElement).click();
    }
  });
}

async function prepareQaEvidence(page: Page) {
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

async function captureQaArtifact(page: Page, name: string) {
  await prepareQaEvidence(page);
  await expect(page.locator(".mc-modal-overlay")).toHaveCount(0);
  await expect(page.locator(".mc-tour-overlay")).toHaveCount(0);
  await page.screenshot({ path: qaArtifact(name), fullPage: true });
}

/**
 * Both Basement rooms on the connectors route ride the optional Connectors
 * page. Only call this while neither Setup surface is on screen so the
 * Settings toggle is the single "Connectors page" checkbox in the document.
 */
async function setConnectorsPageEnabled(
  page: Page,
  enabled: boolean,
): Promise<void> {
  await page.locator('[data-tour-id="nav-config"]').click();
  await expect(settingsModal(page)).toBeVisible();
  const featureSection = settingsModal(page)
    .locator("details")
    .filter({ hasText: "2. Choose what pages show" });
  const checkbox = settingsModal(page).getByRole("checkbox", {
    name: "Connectors page",
  });
  if (!(await featureSection.evaluate((details) => details.open))) {
    await featureSection.locator("summary").click();
  }
  await expect(featureSection).toHaveAttribute("open", "");
  await expect(checkbox).toBeVisible();
  if (enabled) {
    await checkbox.check();
  } else {
    await checkbox.uncheck();
  }
  await page.keyboard.press("Escape");
}

async function assertNoTokenLeakage(page: Page, token: string) {
  // The token may exist only inside the explicit session-only E2E harness
  // slot — never in localStorage, the DOM, or visible text.
  const leakage = await page.evaluate((secret) => {
    const localEntries = JSON.stringify(Object.entries(localStorage));
    return {
      inLocalStorage: localEntries.includes(secret),
      inDom: document.documentElement.outerHTML.includes(secret),
      inText: document.body.innerText.includes(secret),
    };
  }, token);
  expect(leakage).toEqual({
    inLocalStorage: false,
    inDom: false,
    inText: false,
  });
}

/**
 * P5 Basement · Setup room slice: the stable setup room shares the
 * connectors route but lands on the product's distinct gateway/token/
 * feature-toggle/onboarding surface, rendered from the exact same shared
 * authority as the Settings modal. Connection failure and success stay
 * honest without leaking the typed token anywhere outside the explicit
 * session-only E2E harness; Forget token takes exactly one confirmation
 * with its concrete consequence; feature switches drive a real availability
 * change that live-syncs with Settings in both directions; the one
 * onboarding wizard launches exactly once; and the hidden setup shortcut
 * pins config-only with thirteenth-shortcut full-canvas byte immutability.
 * The Office door executes the exact stable-room landing through
 * disable/restore and reload. Verified at desktop and 390px with
 * console-error, requestfailed, S-mark, and inner-rect containment
 * assertions.
 */

test("@core @p5-setup setup room identity, connection/token truth, feature-switch sync, pin-to-office, and the office door hold at desktop and 390px", async ({
  page,
}) => {
  test.setTimeout(180_000);
  const browserErrors: string[] = [];
  // One deliberately injected gateway-health failure proves the honest
  // connection-failure surface; only that exact GET 500 against the health
  // route is excluded from the error budget, and the exclusion count is
  // itself asserted at request, response, and console boundaries.
  let expectInjectedHealthFailure = false;
  let injectedHealthFailureConsoleErrors = 0;
  let injectedHealthFailureRequests = 0;
  let injectedHealthFailureResponses = 0;
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      const sourceUrl = message.location().url;
      if (
        expectInjectedHealthFailure &&
        message.text().includes("500") &&
        /\/api\/v1\/health/.test(`${sourceUrl} ${message.text()}`)
      ) {
        injectedHealthFailureConsoleErrors += 1;
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
        expectInjectedHealthFailure &&
        response.status() === 500 &&
        response.request().method() === "GET" &&
        /\/api\/v1\/health(\?|$)/.test(response.url())
      ) {
        injectedHealthFailureResponses += 1;
        return;
      }
      browserErrors.push(`${response.status()} ${response.url()}`);
    }
  });
  // Every write against the operational authorities is recorded so pinning
  // can be proven config-only rather than merely count-equal.
  const sensitiveMutations: Array<{
    method: string;
    url: string;
    body: unknown;
  }> = [];
  page.on("request", (request) => {
    if (
      expectInjectedHealthFailure &&
      request.method() === "GET" &&
      /\/api\/v1\/health(\?|$)/.test(request.url())
    ) {
      injectedHealthFailureRequests += 1;
    }
    if (
      request.method() !== "GET" &&
      /\/api\/v1\/(config\/runtime|agent-mail|memory|agents|channels|routing|connectors|onboarding)/.test(
        request.url(),
      )
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

  // Setup is the recovery authority for the optional Connectors page, so it
  // remains independently available while Connectors itself is off.
  const roomButton = page.locator('button[title="BF · Setup"]');
  await expect(roomButton).toHaveCount(1);
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(0);
  await setConnectorsPageEnabled(page, true);
  await expect(roomButton).toHaveCount(1);

  // The Setup room exists once, lights exactly one lamp by stable id, and
  // lands on the distinct gateway/token/features/onboarding surface.
  const activeRooms = page.locator(".mc-nav-item-active");
  await roomButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Setup");
  await expect(setupRoom(page)).toBeVisible();
  await expect(setupRoom(page)).toContainText("Gateway connection");
  await expect(setupRoom(page)).toContainText("Feature switches");
  await expect(setupRoom(page)).toContainText("Open setup wizard");
  await expect(setupRoom(page)).toContainText("Start guided tour");
  await expect(page.locator(".mc-connectors-tab-bar")).toHaveCount(0);
  await expect(page.getByTestId("setup-room-page")).not.toContainText(
    "Quick Setup",
  );

  // Shared-authority status chips carry the live connection truth.
  const statusRow = page.getByTestId("setup-room-status");
  await expect(statusRow).toContainText("Gateway: Healthy");
  await expect(statusRow).toContainText("Live link: Connected");
  await expect(statusRow).toContainText("Token: Configured");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-room-desktop.png");

  // Connector quick setup keeps its parity home inside the Connectors room.
  await page.locator('button[title="BF · Connectors"]').click();
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await page
    .locator(".mc-connectors-tab-bar")
    .getByRole("button", { name: "Setup", exact: true })
    .click();
  await expect(page.getByTestId("connectors-page")).toContainText(
    "Quick Setup",
  );
  await expect(activeRooms).toHaveAttribute("title", "BF · Connectors");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-quick-setup-parity-desktop.png");
  await roomButton.click();
  await expect(setupRoom(page)).toBeVisible();

  // The gateway field shows the authoritative saved URL — the wizard save
  // landed in the one shared draft, not a stale blank.
  const urlInputEarly = setupRoom(page).locator(".mc-modal-field input").first();
  await expect(urlInputEarly).toHaveValue(GATEWAY_URL);

  // Token truth: a configured token is a boolean, never an echoed value.
  const tokenInput = setupRoom(page).locator(
    '.mc-modal-field input[type="password"]',
  );
  await expect(tokenInput).toHaveValue("");
  await expect(tokenInput).toHaveAttribute("placeholder", "token configured");
  await assertNoTokenLeakage(page, TEST_TOKEN);

  // A failed save reports honestly without leaking the typed token beyond
  // the owner-controlled password field: the
  // secure upsert lands in the session-only harness slot, the baseline
  // failure surfaces as one critical notice, and the health chip drops.
  await tokenInput.fill(ROTATED_TOKEN);
  expectInjectedHealthFailure = true;
  await page.route(
    "**/api/v1/health",
    async (route) => {
      await route.fulfill({
        status: 500,
        contentType: "application/json",
        body: JSON.stringify({ error: "health backend down" }),
      });
    },
    { times: 1 },
  );
  await setupRoom(page)
    .getByRole("button", { name: "Save and connect" })
    .click();
  const failureToast = page
    .locator(".mc-toast")
    .filter({ hasText: "Connection save failed" });
  await expect(failureToast).toBeVisible();
  await expect(failureToast).toContainText("Gateway health unavailable");
  expect(await failureToast.innerText()).not.toContain(ROTATED_TOKEN);
  await expect(statusRow).toContainText("Gateway: Needs attention");
  await expect(tokenInput).toHaveValue(ROTATED_TOKEN);
  expect(
    await page.evaluate(
      (key) => sessionStorage.getItem(key),
      GATEWAY_TOKEN_KEY,
    ),
  ).toBe(ROTATED_TOKEN);
  await captureQaArtifact(page, "setup-connection-failed-desktop.png");
  await page.unroute("**/api/v1/health");
  await expect
    .poll(() => ({
      console: injectedHealthFailureConsoleErrors,
      requests: injectedHealthFailureRequests,
      responses: injectedHealthFailureResponses,
    }))
    .toEqual({ console: 1, requests: 1, responses: 1 });
  expectInjectedHealthFailure = false;
  await dismissVisibleToasts(page);

  // Reconnect restores the live truth through the same shared action.
  await setupRoom(page)
    .getByRole("button", { name: "Try reconnect" })
    .click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Connection refreshed." }),
  ).toBeVisible();
  await expect(statusRow).toContainText("Gateway: Healthy");
  await dismissVisibleToasts(page);

  // A save persists the trimmed, normalized gateway URL.
  const urlInput = setupRoom(page).locator(".mc-modal-field input").first();
  await urlInput.fill(`   ${GATEWAY_URL}   `);
  await setupRoom(page)
    .getByRole("button", { name: "Save and connect" })
    .click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Connection settings saved." }),
  ).toBeVisible();
  expect(
    JSON.parse(
      (await page.evaluate(
        (key) => localStorage.getItem(key),
        GATEWAY_SETTINGS_KEY,
      )) ?? "{}",
    ),
  ).toEqual({ gateway_url: `${GATEWAY_URL}/` });
  await dismissVisibleToasts(page);

  // Forget token is exactly one confirmation stating the concrete
  // consequence. Cancel changes nothing.
  await setupRoom(page).getByRole("button", { name: "Forget token" }).click();
  const clearDialog = page.getByRole("dialog", { name: "Clear Token?" });
  await expect(clearDialog).toContainText(
    "disconnect the WebSocket connection",
  );
  await clearDialog.getByRole("button", { name: "Cancel" }).click();
  await expect(clearDialog).toBeHidden();
  await expect(statusRow).toContainText("Token: Configured");
  expect(
    await page.evaluate(
      (key) => sessionStorage.getItem(key),
      GATEWAY_TOKEN_KEY,
    ),
  ).toBe(ROTATED_TOKEN);

  // One confirmation executes once: the token leaves secure storage, the
  // live link drops to Waiting, and the configured truth turns Missing.
  await setupRoom(page).getByRole("button", { name: "Forget token" }).click();
  await clearDialog.getByRole("button", { name: "Clear Token" }).click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Gateway token cleared." }),
  ).toBeVisible();
  await expect(statusRow).toContainText("Token: Missing");
  await expect(statusRow).toContainText("Live link: Waiting");
  expect(
    await page.evaluate(
      (key) => sessionStorage.getItem(key),
      GATEWAY_TOKEN_KEY,
    ),
  ).toBeNull();
  // Losing configured auth re-offers the one onboarding wizard (existing
  // product behavior). Dismiss it so the cleared-state evidence shows the
  // Setup room itself.
  const autoReopenedWizard = page.getByRole("dialog", { name: "Setup Wizard" });
  await expect(autoReopenedWizard).toBeVisible();
  await autoReopenedWizard
    .getByRole("button", { name: "Dismiss (24h)" })
    .click();
  await expect(autoReopenedWizard).toBeHidden();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-token-cleared-desktop.png");

  // Reconnection requires a token, exactly as the confirmation warned.
  await tokenInput.fill(TEST_TOKEN);
  await setupRoom(page)
    .getByRole("button", { name: "Save and connect" })
    .click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Connection settings saved." }),
  ).toBeVisible();
  await expect(statusRow).toContainText("Token: Configured");
  await expect(statusRow).toContainText("Live link: Connected");
  await expect(tokenInput).toHaveValue("");
  await assertNoTokenLeakage(page, TEST_TOKEN);
  await dismissVisibleToasts(page);

  // Feature switches drive a real availability change through the one
  // patch authority: the Memory plant lamp appears only when the Memory
  // page is on.
  const memoryLamp = page.locator('button[title="BF · Memory plant"]');
  await expect(memoryLamp).toHaveCount(0);
  const roomMemoryToggle = setupRoom(page).getByRole("checkbox", {
    name: "Memory page",
  });
  await roomMemoryToggle.check();
  await expect(memoryLamp).toHaveCount(1);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-features-desktop.png");

  // Settings shows the same truth live, and its change syncs straight back.
  await page.locator('[data-tour-id="nav-config"]').click();
  await expect(settingsModal(page)).toBeVisible();
  const settingsFeatureSection = settingsModal(page)
    .locator("details")
    .filter({ hasText: "2. Choose what pages show" });
  const settingsMemoryToggle = settingsModal(page).getByRole("checkbox", {
    name: "Memory page",
  });
  if (!(await settingsFeatureSection.evaluate((details) => details.open))) {
    await settingsFeatureSection.locator("summary").click();
  }
  await expect(settingsFeatureSection).toHaveAttribute("open", "");
  await expect(settingsMemoryToggle).toBeVisible();
  await expect(settingsMemoryToggle).toBeChecked();
  await settingsMemoryToggle.uncheck();
  await page.keyboard.press("Escape");
  await expect(settingsModal(page)).toHaveCount(0);
  await expect(roomMemoryToggle).not.toBeChecked();
  await expect(memoryLamp).toHaveCount(0);

  // The Setup room launches the one existing onboarding wizard — exactly
  // one instance — and the guided tour entry works.
  await setupRoom(page)
    .getByRole("button", { name: "Open setup wizard" })
    .click();
  const wizardDialog = page.getByRole("dialog", { name: "Setup Wizard" });
  await expect(wizardDialog).toHaveCount(1);
  await wizardDialog.getByRole("button", { name: "Dismiss (24h)" }).click();
  await expect(wizardDialog).toBeHidden();
  await setupRoom(page)
    .getByRole("button", { name: "Start guided tour" })
    .click();
  const tourOverlay = page.locator(".mc-tour-overlay");
  await expect(tourOverlay).toHaveCount(1);
  await tourOverlay.getByRole("button", { name: "End Tour" }).click();
  await expect(tourOverlay).toHaveCount(0);
  // The tour walks the elevator (its first step navigates to Boards);
  // return to the Setup room for the pin flow.
  await roomButton.click();
  await expect(setupRoom(page)).toBeVisible();

  // Pinning is config-only. The full-canvas fixture genuinely contains all
  // twelve earlier registered shortcuts through File locks (24/24 cells),
  // proves its capacity precondition, refuses byte-for-byte, and no
  // operational mutation fires anywhere in the pin flow.
  const pin = page.getByRole("button", { name: "Pin Setup to Office" });
  await expect(pin).toBeVisible();
  const mutationsBeforePinFlow = JSON.parse(
    JSON.stringify(sensitiveMutations),
  ) as typeof sensitiveMutations;
  const connectionBeforePinFlow = await page.evaluate(
    ({ settingsKey, tokenKey }) => ({
      settings: localStorage.getItem(settingsKey),
      token: sessionStorage.getItem(tokenKey),
    }),
    { settingsKey: GATEWAY_SETTINGS_KEY, tokenKey: GATEWAY_TOKEN_KEY },
  );
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
      "locks",
    ];
    // Twelve 2x1 shortcuts = 24 of 24 cells with every core block hidden.
    config.layout = [
      { id: "needs-you", size: "l", visible: false },
      { id: "in-motion", size: "m", visible: false },
      { id: "done", size: "m", visible: false },
      { id: "next", size: "s", visible: false },
      ...earlierShortcuts.map((id) => ({ id, size: "s", visible: true })),
    ];
    const serialized = JSON.stringify(config);
    localStorage.setItem(key, serialized);
    window.dispatchEvent(new Event("mc-glass-config-changed"));
    return { serialized, earlierShortcuts };
  });
  await page.locator('[data-tour-id="nav-assistant"]').click();
  for (const id of fullCanvas.earlierShortcuts) {
    const renderedBlock = page.getByTestId(`office-block-${id}`);
    await expect(renderedBlock).toBeVisible();
    await expect(renderedBlock).toHaveClass(/mc-block-s/);
  }
  await expect(
    page.locator("[data-testid^='office-block-']:visible"),
  ).toHaveCount(12);
  await roomButton.click();
  await expect(setupRoom(page)).toBeVisible();
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
        (placement: { id?: string }) => placement.id === "setup",
      );
    }),
  ).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-full-canvas-refusal.png");

  // Freeing one small shortcut admits the thirteenth; a repeat pin stays
  // byte-identical and honest.
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "boards"
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
  expect(
    await page.evaluate(
      ({ settingsKey, tokenKey }) => ({
        settings: localStorage.getItem(settingsKey),
        token: sessionStorage.getItem(tokenKey),
      }),
      { settingsKey: GATEWAY_SETTINGS_KEY, tokenKey: GATEWAY_TOKEN_KEY },
    ),
  ).toEqual(connectionBeforePinFlow);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-pinned-desktop.png");

  // The pinned shortcut appears on the Office canvas, names its floor, and
  // consumes config events live without a route remount.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-setup");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "setup"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-setup")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "setup"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(shortcut).toBeVisible();
  await shortcut.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-office-shortcut.png");

  // Opening the door lands the exact stable room on the Setup surface.
  await shortcut.getByRole("button", { name: "Open Setup" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Setup");
  await expect(setupRoom(page)).toBeVisible();
  await expect(setupRoom(page)).toContainText("Gateway connection");

  // The pin is config: it survives a full reload and still opens the exact
  // room afterwards. Clear the wizard's dismissal snooze so onboarding can
  // run again after the reload, exactly like a fresh sitting.
  await page.evaluate(() => {
    localStorage.removeItem("mc-onboarding-dismissed-at-ms");
  });
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await expect(roomButton).toHaveCount(1);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-setup");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut.getByRole("button", { name: "Open Setup" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Setup");
  await expect(setupRoom(page)).toBeVisible();

  // Disabling the optional Connectors page removes only that room. Setup and
  // its persisted recovery door remain live so the user can turn it back on.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await setConnectorsPageEnabled(page, false);
  await expect(roomButton).toHaveCount(1);
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(0);
  await expect(
    page.getByTestId("office-block-setup").getByRole("status"),
  ).toHaveCount(0);
  await page
    .getByTestId("office-block-setup")
    .getByRole("button", { name: "Open Setup" })
    .click();
  await expect(activeRooms).toHaveAttribute("title", "BF · Setup");
  await expect(setupRoom(page)).toBeVisible();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-disabled-door.png");

  // Restoring Connectors changes only that room; Setup stays put.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await setConnectorsPageEnabled(page, true);
  await expect(roomButton).toHaveCount(1);
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(1);
  await page
    .getByTestId("office-block-setup")
    .getByRole("button", { name: "Open Setup" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Setup");
  await expect(setupRoom(page)).toBeVisible();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-restored-door.png");

  // Narrow width: a readable nonzero S mark that nothing paints over, plus
  // contained panels, fields, toggles, and action buttons.
  await page.setViewportSize({ width: 390, height: 844 });
  await roomButton.scrollIntoViewIfNeeded();
  await expect(roomButton).toHaveClass(/mc-nav-item-active/);
  const mark = roomButton.locator(".mc-nav-room-mark");
  await expect(mark).toHaveText("S");
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
      // Nothing may paint over the mark's center: the hit target must be
      // the mark itself or its own room button.
      const centerHit = document.elementFromPoint(
        rect.left + rect.width / 2,
        rect.top + rect.height / 2,
      );
      return Boolean(
        centerHit &&
          (centerHit === node ||
            node.contains(centerHit) ||
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

  const roomGeometry = await page.evaluate(() => {
    const room = document.querySelector('[data-testid="setup-room-page"]');
    if (!room) return { ok: false as const, reason: "no setup room" };
    const outer = room.getBoundingClientRect();
    const visible = (rect: DOMRect) => rect.width > 0 && rect.height > 0;
    const containedX = (rect: DOMRect, slack = 1) =>
      visible(rect) &&
      rect.left >= outer.left - slack &&
      rect.right <= outer.right + slack;
    const selectorCounts = Object.fromEntries(
      [
        ".mc-setup-room-panel",
        ".mc-setup-room-status",
        ".mc-modal-field input",
        ".mc-modal-actions button",
        ".mc-settings-toggle-row",
      ].map((selector) => {
        const rects = Array.from(room.querySelectorAll(selector))
          .map((node) => node.getBoundingClientRect())
          .filter((rect) => visible(rect));
        return [
          selector,
          {
            visible: rects.length,
            allContained: rects.every((rect) => containedX(rect)),
          },
        ];
      }),
    );
    return { ok: true as const, outerVisible: visible(outer), selectorCounts };
  });
  expect(roomGeometry.ok).toBe(true);
  if (roomGeometry.ok) {
    expect(roomGeometry.outerVisible).toBe(true);
    expect(roomGeometry.selectorCounts).toEqual({
      ".mc-setup-room-panel": { visible: 2, allContained: true },
      ".mc-setup-room-status": { visible: 1, allContained: true },
      ".mc-modal-field input": { visible: 2, allContained: true },
      ".mc-modal-actions button": { visible: 5, allContained: true },
      ".mc-settings-toggle-row": { visible: 8, allContained: true },
    });
  }
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-room-390.png");
  const mobileFeaturePanel = setupRoom(page)
    .locator(".mc-setup-room-panel")
    .filter({ hasText: "Feature switches" });
  await mobileFeaturePanel.scrollIntoViewIfNeeded();
  await expect(mobileFeaturePanel).toBeInViewport();
  await captureQaArtifact(page, "setup-features-390.png");

  // The one-confirmation dialog stays contained at 390px; cancelling keeps
  // the configured token untouched.
  const mobileForget = setupRoom(page).getByRole("button", {
    name: "Forget token",
  });
  await mobileForget.scrollIntoViewIfNeeded();
  await mobileForget.click();
  const mobileClearDialog = page.getByRole("dialog", { name: "Clear Token?" });
  await expect(mobileClearDialog).toBeVisible();
  expect(
    await mobileClearDialog.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth
      );
    }),
  ).toBe(true);
  await mobileClearDialog.getByRole("button", { name: "Cancel" }).click();
  await expect(mobileClearDialog).toBeHidden();
  await expect(statusRow).toContainText("Token: Configured");

  // The Office door stays reachable and contained at 390px and still opens
  // the exact room.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const mobileShortcut = page.getByTestId("office-block-setup");
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
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-office-door-390.png");
  await mobileShortcut.getByRole("button", { name: "Open Setup" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Setup");
  await expect(setupRoom(page)).toBeVisible();

  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "setup-door-390.png");

  // The deliberate failure window swallowed exactly the one injected read.
  expect(injectedHealthFailureConsoleErrors).toBe(1);
  expect(injectedHealthFailureRequests).toBe(1);
  expect(injectedHealthFailureResponses).toBe(1);
  expect(browserErrors).toEqual([]);
});
