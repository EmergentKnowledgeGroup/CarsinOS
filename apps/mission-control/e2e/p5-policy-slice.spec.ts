import { fileURLToPath } from "node:url";

import { expect, test, type Locator, type Page } from "./testHarness";
import {
  GATEWAY_URL,
  TEST_TOKEN,
  completeQuickstartLocalOnboarding,
} from "./onboardingFlow";

const GLASS_CONFIG_KEY = "mc-glass-config-v1";

function qaArtifact(name: string) {
  return fileURLToPath(
    new URL(`../../../runtime/qa/p5-policy-slice/${name}`, import.meta.url),
  );
}

function policyRoom(page: Page) {
  return page.getByTestId("policy-room-page");
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
  // The crash sentinel exists only in E2E mode; it is not product chrome.
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

async function assertCriticalGeometry(locators: readonly Locator[]) {
  for (const locator of locators) {
    const count = await locator.count();
    expect(count).toBeGreaterThan(0);
    for (let index = 0; index < count; index += 1) {
      const item = locator.nth(index);
      await item.scrollIntoViewIfNeeded();
      await expect(item).toBeVisible();
      const geometry = await item.evaluate((node) => {
          const rect = node.getBoundingClientRect();
          const room = node.closest('[data-testid="policy-room-page"]');
          const roomRect = room?.getBoundingClientRect();
          const centerHit = document.elementFromPoint(
            rect.left + rect.width / 2,
            rect.top + rect.height / 2,
          );
          return {
            nonzero: rect.width > 0 && rect.height > 0,
            viewportX: rect.left >= 0 && rect.right <= window.innerWidth,
            viewportY:
              rect.top >= -1 && rect.bottom <= window.innerHeight + 1,
            roomX:
              !roomRect ||
              (rect.left >= roomRect.left - 1 &&
                rect.right <= roomRect.right + 1),
            centerHit: Boolean(
              centerHit &&
                (centerHit === node ||
                  node.contains(centerHit) ||
                  centerHit.contains(node)),
            ),
            rect: {
              left: rect.left,
              right: rect.right,
              top: rect.top,
              bottom: rect.bottom,
              width: rect.width,
              height: rect.height,
            },
            viewport: {
              width: window.innerWidth,
              height: window.innerHeight,
            },
          };
        });
      expect(geometry, JSON.stringify(geometry)).toMatchObject({
        nonzero: true,
        viewportX: true,
        viewportY: true,
        roomX: true,
        centerHit: true,
      });
    }
  }
}

/**
 * Only call this while no Setup surface is on screen so the Settings toggle
 * is the single "Connectors page" checkbox in the document. The Policy room
 * itself has no feature checkboxes, so it is a safe caller.
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
  await expect(settingsModal(page)).toHaveCount(0);
}

interface RecordedPolicyRequest {
  method: string;
  path: string;
  idempotency_header: string;
  request_id: string;
  owner_proof_header: string | null;
  body: {
    idempotency_key: string;
    expected_policy_revision: number;
    change_summary: string;
    proposed_profile: string;
    proposed_rules: Array<Record<string, unknown>>;
  };
}

function decodeProofHeader(headerValue: string): {
  binding: Record<string, unknown>;
  proof: Record<string, unknown>;
} {
  const normalized = headerValue.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized + "=".repeat((4 - (normalized.length % 4)) % 4);
  return JSON.parse(Buffer.from(padded, "base64").toString("utf8"));
}

/**
 * P5 Basement · Policy room slice: the ninth design-required room presents
 * plain-language ExecAss autonomy truth (exact owner directions are never
 * policed; dangerous work gets exactly one consequence confirmation; policy
 * governs derived/unattended work; quotas are technical limits, never
 * money). The stable policy room shares the connectors route but is
 * independently reachable, renders before every connector gate, and is
 * never redirected to Setup when Connectors is disabled. Editing keeps the
 * complete ruleset; one review then one confirmation applies through the
 * exact owner-proof PUT (body digest, safe-snapshot digest, header-bound
 * correlation, idempotency, revision) against a mock that enforces the same
 * binding identity as the production gateway. Deliberate 503 and 409
 * injections prove retained drafts and authoritative conflict refetch; the
 * policy.changed websocket event through the one Office stream refetches
 * live. The hidden policy shortcut proves fourteenth-shortcut capacity
 * (thirteen earlier placements, one explicitly hidden, twelve visible small
 * shortcuts at 24/24 cells, byte-identical refusal, admit-after-free),
 * config-only pinning, the exact restored door, and reload persistence.
 * Verified at desktop and 390px with console-error, requestfailed, P-mark,
 * and nonzero inner-geometry assertions.
 */

test("@core @p5-policy policy room truth, edit/review/confirm proof, failure and conflict recovery, ws refetch, pin capacity, and the office door hold at desktop and 390px", async ({
  page,
  request,
}) => {
  test.setTimeout(240_000);
  const browserErrors: string[] = [];
  let expectInjectedPolicyUnavailable = false;
  let injectedUnavailableRequests = 0;
  let injectedUnavailableConsole = 0;
  let injectedUnavailableResponses = 0;
  let expectInjectedPolicyConflict = false;
  let injectedConflictRequests = 0;
  let injectedConflictConsole = 0;
  let injectedConflictResponses = 0;
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      const sourceUrl = message.location().url;
      const haystack = `${sourceUrl} ${message.text()}`;
      if (
        expectInjectedPolicyUnavailable &&
        message.text().includes("503") &&
        /\/api\/v1\/execass\/policy/.test(haystack)
      ) {
        injectedUnavailableConsole += 1;
        return;
      }
      if (
        expectInjectedPolicyConflict &&
        message.text().includes("409") &&
        /\/api\/v1\/execass\/policy/.test(haystack)
      ) {
        injectedConflictConsole += 1;
        return;
      }
      browserErrors.push(message.text());
    }
  });
  page.on("requestfailed", (req) => {
    browserErrors.push(
      `request failed: ${req.method()} ${req.url()} ${req.failure()?.errorText ?? "unknown"}`,
    );
  });
  page.on("response", (response) => {
    if (response.status() >= 400) {
      const isPolicyPut =
        response.request().method() === "PUT" &&
        /\/api\/v1\/execass\/policy(\?|$)/.test(response.url());
      if (
        expectInjectedPolicyUnavailable &&
        response.status() === 503 &&
        isPolicyPut
      ) {
        injectedUnavailableResponses += 1;
        return;
      }
      if (
        expectInjectedPolicyConflict &&
        response.status() === 409 &&
        isPolicyPut
      ) {
        injectedConflictResponses += 1;
        return;
      }
      browserErrors.push(`${response.status()} ${response.url()}`);
    }
  });
  // Every non-GET against operational authorities is recorded so pinning
  // can be proven config-only rather than merely count-equal.
  const sensitiveMutations: Array<{
    method: string;
    url: string;
    body: unknown;
  }> = [];
  page.on("request", (req) => {
    const isPolicyPut =
      req.method() === "PUT" &&
      /\/api\/v1\/execass\/policy(\?|$)/.test(req.url());
    if (expectInjectedPolicyUnavailable && isPolicyPut) {
      injectedUnavailableRequests += 1;
    }
    if (expectInjectedPolicyConflict && isPolicyPut) {
      injectedConflictRequests += 1;
    }
    if (
      req.method() !== "GET" &&
      /\/api\/v1\/(config\/runtime|agent-mail|memory|agents|channels|routing|connectors|onboarding|execass\/policy)/.test(
        req.url(),
      )
    ) {
      let body: unknown = null;
      try {
        body = req.postDataJSON();
      } catch {
        body = req.postData();
      }
      sensitiveMutations.push({ method: req.method(), url: req.url(), body });
    }
  });

  await completeQuickstartLocalOnboarding(page, {
    beforeGoto: async (nextPage) => {
      await nextPage.addInitScript(() => {
        window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
      });
    },
  });

  // Policy is independently reachable while the optional Connectors page is
  // off, exactly like Setup.
  const roomButton = page.locator('button[title="BF · Policy"]');
  await expect(roomButton).toHaveCount(1);
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(0);
  await setConnectorsPageEnabled(page, true);
  await expect(roomButton).toHaveCount(1);

  // The room exists once, lights exactly one lamp by stable id, and lands on
  // the plain-language policy surface with the three commitments.
  const activeRooms = page.locator(".mc-nav-item-active");
  await roomButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Policy");
  await expect(policyRoom(page)).toBeVisible();
  await expect(policyRoom(page)).toContainText("The deal");
  await expect(policyRoom(page)).toContainText("Autonomy profile");
  await expect(policyRoom(page)).toContainText("never policed");
  await expect(policyRoom(page)).toContainText("exactly one confirmation");
  await expect(policyRoom(page)).toContainText(
    "without a fresh instruction from you",
  );
  await expect(page.locator(".mc-connectors-tab-bar")).toHaveCount(0);

  // Authoritative initial truth: configured Balanced policy at revision 7.
  const statusRow = page.getByTestId("policy-room-status");
  await expect(statusRow).toContainText("Profile: Balanced");
  await expect(statusRow).toContainText("Revision 7");
  await expect(page.getByTestId("policy-effective-summary")).toContainText(
    "Balanced autonomy - ordinary work proceeds, dangerous actions get one confirmation.",
  );

  // Quotas are labeled technical execution limits, never money.
  const ruleDetails = page.getByTestId("policy-rule-advanced-rule-1");
  await ruleDetails.locator("summary").click();
  await expect(ruleDetails).toContainText("Technical execution limits");
  await expect(ruleDetails).toContainText("not money");
  await expect(ruleDetails).toContainText("118,000 used of 500,000 limit");
  await assertCriticalGeometry([
    page.getByTestId("policy-room-status"),
    page.getByTestId("policy-profile-locked_down"),
    page.getByTestId("policy-profile-balanced"),
    page.getByTestId("policy-profile-full_send"),
    page.getByTestId("policy-profile-custom"),
    page.getByRole("button", { name: "Pin Policy to Office" }),
  ]);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-room-desktop.png");

  // ── Edit → one review → injected failure keeps the draft honest ──
  await page.getByTestId("policy-profile-full_send").click();
  await expect(statusRow).toContainText("Editing");
  const parallelism = page.getByTestId("policy-rule-parallelism-rule-1");
  await expect(parallelism).toHaveValue("4");
  await parallelism.fill("6");
  await page.getByTestId("policy-review-open").click();
  const review = page.getByTestId("policy-review");
  await expect(review).toBeVisible();
  await expect(review).toContainText("From revision 7");
  await expect(review).toContainText("Full send");
  const confirmButton = page.getByTestId("policy-confirm");
  await expect(confirmButton).toBeDisabled();
  await page
    .getByTestId("policy-change-summary")
    .fill("Let routine work run at full speed overnight.");
  await expect(confirmButton).toBeEnabled();
  await assertCriticalGeometry([
    page.getByTestId("policy-change-summary"),
    page.getByTestId("policy-confirm"),
    page.getByTestId("policy-review-cancel"),
  ]);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-review-desktop.png");

  const failureArm = await request.post(
    `${GATEWAY_URL}/api/v1/e2e/execass-policy-failure`,
    {
      headers: { Authorization: `Bearer ${TEST_TOKEN}` },
      data: { mode: "unavailable" },
    },
  );
  expect(failureArm.ok()).toBeTruthy();
  expectInjectedPolicyUnavailable = true;
  await confirmButton.click();
  await expect(
    page
      .locator(".mc-toast")
      .filter({ hasText: "The policy service is briefly unavailable." }),
  ).toBeVisible();
  // No success claim: the draft, the review, and the revision all hold.
  await expect(review).toBeVisible();
  await expect(statusRow).toContainText("Revision 7");
  await expect(statusRow).toContainText("Editing");
  await expect(page.getByTestId("policy-change-summary")).toHaveValue(
    "Let routine work run at full speed overnight.",
  );
  await expect
    .poll(() => ({
      requests: injectedUnavailableRequests,
      console: injectedUnavailableConsole,
      responses: injectedUnavailableResponses,
    }))
    .toEqual({ requests: 1, console: 1, responses: 1 });
  expectInjectedPolicyUnavailable = false;
  await page
    .locator(".mc-toast")
    .filter({ hasText: "The policy service is briefly unavailable." })
    .scrollIntoViewIfNeeded();
  await captureQaArtifact(page, "policy-failure-desktop.png");
  await dismissVisibleToasts(page);

  // ── The one confirmation now applies once, authoritatively ──
  await confirmButton.click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Policy updated" }),
  ).toBeVisible();
  await expect(statusRow).toContainText("Profile: Full send");
  await expect(statusRow).toContainText("Revision 8");
  await expect(statusRow).not.toContainText("Editing");
  await expect(page.getByTestId("policy-review")).toHaveCount(0);
  await expect(page.getByTestId("policy-effective-summary")).toContainText(
    "Let routine work run at full speed overnight.",
  );
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-updated-desktop.png");

  // The recorded PUTs carry the exact proof identity the gateway demands.
  const recordedAfterUpdate = (await (
    await request.get(`${GATEWAY_URL}/api/v1/e2e/execass-policy-requests`, {
      headers: { Authorization: `Bearer ${TEST_TOKEN}` },
    })
  ).json()) as { items: RecordedPolicyRequest[] };
  expect(recordedAfterUpdate.items).toHaveLength(2);
  const successPut = recordedAfterUpdate.items[1]!;
  expect(successPut.method).toBe("PUT");
  expect(successPut.idempotency_header).toBe(successPut.body.idempotency_key);
  expect(successPut.body.expected_policy_revision).toBe(7);
  expect(successPut.body.proposed_profile).toBe("full_send");
  expect(successPut.body.change_summary).toBe(
    "Let routine work run at full speed overnight.",
  );
  // The complete ruleset travels: the edited field changed and every
  // untouched field survived exactly.
  expect(successPut.body.proposed_rules).toEqual([
    {
      rule_id: "rule-1",
      task_or_delegation_scope: null,
      workspace_scope: null,
      routine_scope: null,
      connector_or_tool_identity_and_version_scope: null,
      target_scope: null,
      audience_scope: null,
      technical_resource_quotas: [
        { kind: "tokens", limit: 500000, reserved: 0, consumed: 118000 },
      ],
      expires_at_ms: null,
      recovery_limit: 3,
      parallelism_limit: 6,
      clarification_sensitivity: "normal",
      recurring_work_scope: null,
    },
  ]);
  const successProof = decodeProofHeader(successPut.owner_proof_header!);
  expect(successProof.binding.operation).toBe("policy_update");
  expect(successProof.binding.method).toBe("PUT");
  expect(successProof.binding.path).toBe("/api/v1/execass/policy");
  expect(successProof.binding.idempotency_key).toBe(
    successPut.body.idempotency_key,
  );
  expect(successProof.binding.expected_revision).toBe(7);
  expect(successProof.binding.request_correlation_id).toBe(
    successPut.request_id,
  );
  expect(successProof.binding.canonical_body_digest).toMatch(/^[0-9a-f]{64}$/);
  expect(successProof.binding.safe_snapshot_digest).toMatch(/^[0-9a-f]{64}$/);
  expect(successProof.proof.request_correlation_id).toBe(
    successPut.request_id,
  );
  // Both idempotency keys and correlations differ between the two attempts.
  expect(recordedAfterUpdate.items[0]!.body.idempotency_key).not.toBe(
    successPut.body.idempotency_key,
  );
  expect(recordedAfterUpdate.items[0]!.request_id).not.toBe(
    successPut.request_id,
  );

  // ── policy.changed through the one Office stream refetches live ──
  const externalChange = await request.post(
    `${GATEWAY_URL}/api/v1/e2e/execass-policy-changed`,
    {
      headers: { Authorization: `Bearer ${TEST_TOKEN}` },
      data: { summary: "Maintenance tightened overnight caps." },
    },
  );
  expect(externalChange.ok()).toBeTruthy();
  await expect(statusRow).toContainText("Revision 9");
  await expect(page.getByTestId("policy-effective-summary")).toContainText(
    "Maintenance tightened overnight caps.",
  );
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-ws-refetch-desktop.png");

  // ── Revision conflict: refetch authoritative truth, keep the draft ──
  await page.getByTestId("policy-profile-balanced").click();
  await page.getByTestId("policy-review-open").click();
  await page
    .getByTestId("policy-change-summary")
    .fill("Back to balanced after the review.");
  const conflictArm = await request.post(
    `${GATEWAY_URL}/api/v1/e2e/execass-policy-failure`,
    {
      headers: { Authorization: `Bearer ${TEST_TOKEN}` },
      data: { mode: "conflict" },
    },
  );
  expect(conflictArm.ok()).toBeTruthy();
  expectInjectedPolicyConflict = true;
  await page.getByTestId("policy-confirm").click();
  const conflictBanner = page.getByTestId("policy-conflict");
  await expect(conflictBanner).toBeVisible();
  await expect(conflictBanner).toContainText("changed while you were editing");
  await expect(conflictBanner).toContainText("revision 10");
  await expect(statusRow).toContainText("Editing");
  await expect(page.getByTestId("policy-change-summary")).toHaveValue(
    "Back to balanced after the review.",
  );
  await expect
    .poll(() => ({
      requests: injectedConflictRequests,
      console: injectedConflictConsole,
      responses: injectedConflictResponses,
    }))
    .toEqual({ requests: 1, console: 1, responses: 1 });
  expectInjectedPolicyConflict = false;
  await dismissVisibleToasts(page);
  await conflictBanner.scrollIntoViewIfNeeded();
  await captureQaArtifact(page, "policy-conflict-desktop.png");

  // A stale complete ruleset cannot be resubmitted at revision 10. The owner
  // explicitly reconciles bounded edits onto the latest rules and performs a
  // fresh review before the one confirmation becomes live again.
  await expect(page.getByTestId("policy-confirm")).toBeDisabled();
  await page.getByTestId("policy-reconcile").click();
  await expect(page.getByTestId("policy-conflict")).toHaveCount(0);
  await expect(page.getByTestId("policy-review")).toHaveCount(0);
  await page.getByTestId("policy-review-open").click();
  await expect(page.getByTestId("policy-change-summary")).toHaveValue(
    "Back to balanced after the review.",
  );
  await page.getByTestId("policy-confirm").click();
  await expect(
    page.locator(".mc-toast").filter({ hasText: "Policy updated" }),
  ).toBeVisible();
  await expect(statusRow).toContainText("Profile: Balanced");
  await expect(statusRow).toContainText("Revision 11");
  await expect(page.getByTestId("policy-conflict")).toHaveCount(0);
  const recordedAfterConflict = (await (
    await request.get(`${GATEWAY_URL}/api/v1/e2e/execass-policy-requests`, {
      headers: { Authorization: `Bearer ${TEST_TOKEN}` },
    })
  ).json()) as { items: RecordedPolicyRequest[] };
  const reconciledPut = recordedAfterConflict.items.at(-1)!;
  expect(reconciledPut.body.expected_policy_revision).toBe(10);
  expect(reconciledPut.body.proposed_rules[0]).toMatchObject({
    parallelism_limit: 6,
    clarification_sensitivity: "strict",
  });
  await dismissVisibleToasts(page);

  // ── Pinning is config-only against the fourteenth-shortcut capacity ──
  const pin = page.getByRole("button", { name: "Pin Policy to Office" });
  await expect(pin).toBeVisible();
  const mutationsBeforePinFlow = JSON.parse(
    JSON.stringify(sensitiveMutations),
  ) as typeof sensitiveMutations;
  const fullCanvas = await page.evaluate((key) => {
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    const visibleShortcuts = [
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
      "setup",
    ];
    // All thirteen earlier placements through Setup exist; boards is the one
    // explicitly hidden placement, so twelve visible 2x1 shortcuts fill
    // 24/24 cells beside hidden core blocks.
    config.layout = [
      { id: "needs-you", size: "l", visible: false },
      { id: "in-motion", size: "m", visible: false },
      { id: "done", size: "m", visible: false },
      { id: "next", size: "s", visible: false },
      { id: "boards", size: "s", visible: false },
      ...visibleShortcuts.map((id) => ({ id, size: "s", visible: true })),
    ];
    const serialized = JSON.stringify(config);
    localStorage.setItem(key, serialized);
    window.dispatchEvent(new Event("mc-glass-config-changed"));
    return { serialized, visibleShortcuts };
  }, GLASS_CONFIG_KEY);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  for (const id of fullCanvas.visibleShortcuts) {
    const renderedBlock = page.getByTestId(`office-block-${id}`);
    await expect(renderedBlock).toBeVisible();
    await expect(renderedBlock).toHaveClass(/mc-block-s/);
  }
  await expect(page.getByTestId("office-block-boards")).toHaveCount(0);
  await expect(
    page.locator("[data-testid^='office-block-']:visible"),
  ).toHaveCount(12);
  await dismissVisibleToasts(page);
  await prepareQaEvidence(page);
  await page
    .locator(".mc-execass-buckets")
    .screenshot({ path: qaArtifact("policy-full-canvas-office.png") });
  await roomButton.click();
  await expect(policyRoom(page)).toBeVisible();
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note.is-error")).toHaveText(
    "Pinning this would exceed the six-column, four-row Office canvas.",
  );
  expect(
    await page.evaluate((key) => localStorage.getItem(key), GLASS_CONFIG_KEY),
  ).toBe(fullCanvas.serialized);
  expect(
    await page.evaluate((key) => {
      const config = JSON.parse(localStorage.getItem(key) ?? "{}");
      return config.layout?.some(
        (placement: { id?: string }) => placement.id === "policy",
      );
    }, GLASS_CONFIG_KEY),
  ).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-full-canvas-refusal.png");

  // Freeing one visible shortcut admits the fourteenth; repeat stays honest.
  await page.evaluate((key) => {
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "calendar"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  }, GLASS_CONFIG_KEY);
  await expect(page.locator(".mc-pin-to-office-note")).toHaveCount(0);
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "On the Office canvas.",
  );
  const pinnedConfig = await page.evaluate(
    (key) => localStorage.getItem(key),
    GLASS_CONFIG_KEY,
  );
  await pin.click();
  await expect(page.locator(".mc-pin-to-office-note")).toHaveText(
    "Already on the Office canvas.",
  );
  expect(
    await page.evaluate((key) => localStorage.getItem(key), GLASS_CONFIG_KEY),
  ).toBe(pinnedConfig);
  // Zero policy, connection, routing, memory, Mail, or ExecAss mutations
  // fired anywhere in the pin flow: pinning is Glass config only.
  expect(sensitiveMutations).toEqual(mutationsBeforePinFlow);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-pinned-desktop.png");

  // The mounted Office consumes config events live; the door opens the
  // exact stable room.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-policy");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await shortcut.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-office-shortcut.png");
  await shortcut.getByRole("button", { name: "Open Policy" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Policy");
  await expect(policyRoom(page)).toBeVisible();

  // Disabling Connectors never redirects Policy to Setup and keeps both the
  // lamp and the persisted door live.
  await setConnectorsPageEnabled(page, false);
  await expect(activeRooms).toHaveAttribute("title", "BF · Policy");
  await expect(policyRoom(page)).toBeVisible();
  await expect(policyRoom(page)).toContainText("Autonomy profile");
  await expect(roomButton).toHaveCount(1);
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(0);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-disabled-room-desktop.png");
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await page
    .getByTestId("office-block-policy")
    .getByRole("button", { name: "Open Policy" })
    .click();
  await expect(activeRooms).toHaveAttribute("title", "BF · Policy");
  await expect(policyRoom(page)).toBeVisible();

  // Restoring Connectors changes only that room; Policy stays put.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await setConnectorsPageEnabled(page, true);
  await expect(roomButton).toHaveCount(1);
  await expect(page.locator('button[title="BF · Connectors"]')).toHaveCount(1);
  await page
    .getByTestId("office-block-policy")
    .getByRole("button", { name: "Open Policy" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Policy");
  await expect(policyRoom(page)).toBeVisible();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-restored-door.png");

  // The pin is config: it survives a full reload and still opens the room.
  await page.evaluate(() => {
    localStorage.removeItem("mc-onboarding-dismissed-at-ms");
  });
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await expect(roomButton).toHaveCount(1);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-policy");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut.getByRole("button", { name: "Open Policy" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Policy");
  await expect(policyRoom(page)).toBeVisible();

  // ── Narrow width: readable P mark, contained controls, working review ──
  await page.setViewportSize({ width: 390, height: 844 });
  await roomButton.scrollIntoViewIfNeeded();
  await expect(roomButton).toHaveClass(/mc-nav-item-active/);
  const mark = roomButton.locator(".mc-nav-room-mark");
  await expect(mark).toHaveText("P");
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
    const room = document.querySelector('[data-testid="policy-room-page"]');
    if (!room) return { ok: false as const, reason: "no policy room" };
    const outer = room.getBoundingClientRect();
    const visible = (rect: DOMRect) => rect.width > 0 && rect.height > 0;
    const containedX = (rect: DOMRect, slack = 1) =>
      visible(rect) &&
      rect.left >= outer.left - slack &&
      rect.right <= outer.right + slack;
    const selectorCounts = Object.fromEntries(
      [
        ".mc-policy-room-panel",
        ".mc-policy-room-status",
        ".mc-policy-profile",
        ".mc-policy-commitments li",
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
      ".mc-policy-room-panel": { visible: 3, allContained: true },
      ".mc-policy-room-status": { visible: 1, allContained: true },
      ".mc-policy-profile": { visible: 4, allContained: true },
      ".mc-policy-commitments li": { visible: 3, allContained: true },
    });
  }
  await assertCriticalGeometry([
    page.getByTestId("policy-room-status"),
    page.getByTestId("policy-profile-locked_down"),
    page.getByTestId("policy-profile-balanced"),
    page.getByTestId("policy-profile-full_send"),
    page.getByTestId("policy-profile-custom"),
    page.getByRole("button", { name: "Pin Policy to Office" }),
  ]);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-room-390.png");

  // The full edit/review surface stays contained and cancellable at 390px.
  const mobileProfileCard = page.getByTestId("policy-profile-locked_down");
  await mobileProfileCard.scrollIntoViewIfNeeded();
  await mobileProfileCard.click();
  await expect(statusRow).toContainText("Editing");
  const mobileReviewOpen = page.getByTestId("policy-review-open");
  await mobileReviewOpen.scrollIntoViewIfNeeded();
  await mobileReviewOpen.click();
  const mobileReview = page.getByTestId("policy-review");
  await mobileReview.scrollIntoViewIfNeeded();
  await expect(mobileReview).toBeVisible();
  expect(
    await mobileReview.evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        rect.left >= 0 &&
        rect.right <= window.innerWidth
      );
    }),
  ).toBe(true);
  await assertCriticalGeometry([
    page.getByTestId("policy-change-summary"),
    page.getByTestId("policy-confirm"),
    page.getByTestId("policy-review-cancel"),
  ]);
  await page.locator(".mc-content-area").evaluate((node) => {
    node.scrollBy({ top: 24, behavior: "instant" });
  });
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-review-390.png");
  await page.getByTestId("policy-review-cancel").click();
  await expect(page.getByTestId("policy-review")).toHaveCount(0);
  const mobileDiscard = page.getByTestId("policy-discard");
  await mobileDiscard.scrollIntoViewIfNeeded();
  await mobileDiscard.click();
  await expect(statusRow).not.toContainText("Editing");
  await expect(statusRow).toContainText("Profile: Balanced");

  // The Office door stays reachable, contained, and exact at 390px.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const mobileShortcut = page.getByTestId("office-block-policy");
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
  await assertCriticalGeometry([
    mobileShortcut,
    mobileShortcut.getByRole("button", { name: "Open Policy" }),
  ]);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-office-door-390.png");
  await mobileShortcut.getByRole("button", { name: "Open Policy" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Policy");
  await expect(policyRoom(page)).toBeVisible();

  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "policy-door-390.png");

  // The deliberate failure windows swallowed exactly one injection each.
  expect(injectedUnavailableRequests).toBe(1);
  expect(injectedUnavailableConsole).toBe(1);
  expect(injectedUnavailableResponses).toBe(1);
  expect(injectedConflictRequests).toBe(1);
  expect(injectedConflictConsole).toBe(1);
  expect(injectedConflictResponses).toBe(1);
  expect(browserErrors).toEqual([]);
});
