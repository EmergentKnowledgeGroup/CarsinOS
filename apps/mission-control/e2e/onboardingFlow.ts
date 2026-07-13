import { expect, type Page } from "./testHarness";

export const E2E_APP_URL = "/?e2e=1";
export const GATEWAY_URL = "http://127.0.0.1:19789";
export const TEST_TOKEN = "stub-token-001";
export const ASSISTANT_MODEL_ID = "qwen3.5-9b-instruct";
const GATEWAY_SETTINGS_KEY = "mc-gateway-settings";
const GATEWAY_TOKEN_KEY = "mc-gateway-token";
const GUIDED_TOUR_KEY = "mc-guided-tour-completed-v1";

export function wizard(page: Page) {
  return page.getByRole("dialog", { name: "Setup Wizard" });
}

export async function clickAdvancedNav(page: Page, tab: string): Promise<void> {
  const target = page.locator(`[data-tour-id="nav-${tab}"]`);
  if (!(await target.isVisible())) {
    await page.locator('[data-tour-id="nav-advanced"]').click();
    await expect(target).toBeVisible();
  }
  await target.click();
}

export async function openWizard(
  page: Page,
  options?: {
    beforeGoto?: (page: Page) => Promise<void>;
    seedConnection?: boolean;
  }
): Promise<boolean> {
  await page.addInitScript(
    ({ gatewayUrl, token, gatewaySettingsKey, gatewayTokenKey, guidedTourKey, seedConnection }) => {
      if (seedConnection) {
        window.localStorage.setItem(
          gatewaySettingsKey,
          JSON.stringify({ gateway_url: gatewayUrl })
        );
        window.localStorage.removeItem(gatewayTokenKey);
        window.sessionStorage.setItem(gatewayTokenKey, token);
      } else {
        window.localStorage.removeItem(gatewaySettingsKey);
        window.localStorage.removeItem(gatewayTokenKey);
        window.sessionStorage.removeItem(gatewayTokenKey);
      }
      window.localStorage.setItem(guidedTourKey, "true");
    },
    {
      gatewayUrl: `${GATEWAY_URL}/`,
      token: TEST_TOKEN,
      gatewaySettingsKey: GATEWAY_SETTINGS_KEY,
      gatewayTokenKey: GATEWAY_TOKEN_KEY,
      guidedTourKey: GUIDED_TOUR_KEY,
      seedConnection: options?.seedConnection ?? false,
    }
  );
  if (options?.beforeGoto) {
    await options.beforeGoto(page);
  }
  await page.goto(E2E_APP_URL);
  try {
    await expect(wizard(page)).toBeVisible({ timeout: 5000 });
    return true;
  } catch {
    return false;
  }
}

export async function moveWizardToConnectionStep(
  page: Page,
  options?: {
    beforeGoto?: (page: Page) => Promise<void>;
    seedConnection?: boolean;
  }
): Promise<boolean> {
  const visible = await openWizard(page, options);
  if (!visible) {
    return false;
  }
  const setupWizard = wizard(page);
  await setupWizard.getByRole("button", { name: "Continue" }).click();
  await expect(setupWizard.getByText("Step 2 of 6")).toBeVisible();
  await setupWizard.getByRole("button", { name: "Continue" }).click();
  await expect(setupWizard.getByText("Step 3 of 6")).toBeVisible();
  return true;
}

export async function completeQuickstartLocalOnboarding(
  page: Page,
  options?: {
    beforeGoto?: (page: Page) => Promise<void>;
    agentId?: string;
    agentName?: string;
    startFromConnectionStep?: boolean;
    seedConnection?: boolean;
  }
): Promise<void> {
  if (!options?.startFromConnectionStep) {
    const visible = await moveWizardToConnectionStep(page, options);
    if (!visible) {
      throw new Error("Setup wizard did not appear before quickstart onboarding could run.");
    }
  }
  const setupWizard = wizard(page);

  await setupWizard.getByLabel("Gateway URL").fill(GATEWAY_URL);
  await setupWizard.getByLabel("Gateway token").fill(TEST_TOKEN);
  await setupWizard.getByRole("button", { name: /Save \+ Connect/ }).click();
  await expect(setupWizard.getByText(/Connection status:\s*Connected/)).toBeVisible();
  await setupWizard.getByRole("button", { name: "Save connection + Continue" }).click();

  await expect(setupWizard.getByText("Step 4 of 6")).toBeVisible();
  await setupWizard.getByRole("button", { name: "Start new agent" }).click();
  const advancedAgentSettings = setupWizard.getByText("Advanced agent settings", { exact: true });
  if (await advancedAgentSettings.isVisible()) {
    await advancedAgentSettings.click();
  }
  await setupWizard.getByLabel("Agent ID").fill(options?.agentId ?? "assistant-main");
  await setupWizard.getByLabel("Assistant name").fill(options?.agentName ?? "Assistant");
  await setupWizard.getByRole("radio", { name: "Local connector" }).check();
  await setupWizard
    .getByRole("combobox", { name: "Assistant model" })
    .selectOption(ASSISTANT_MODEL_ID);
  await setupWizard.getByRole("button", { name: "Apply setup + Continue" }).click();

  await expect(setupWizard.getByText("Step 5 of 6")).toBeVisible();
  await setupWizard.getByRole("button", { name: "Finish setup" }).click();
  await expect(setupWizard.getByText("Step 6 of 6")).toBeVisible();
  await setupWizard.getByRole("button", { name: "Go to Boards" }).click();
  await expect(setupWizard).toBeHidden();
}
