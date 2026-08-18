import { fileURLToPath } from "node:url";

import { expect, test } from "./testHarness";
import { completeQuickstartLocalOnboarding } from "./onboardingFlow";

function qaArtifact(name: string) {
  return fileURLToPath(
    new URL(`../../../runtime/qa/p5-memory-slice/${name}`, import.meta.url),
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
  // Multiple mounted panes can contain hidden help banners, so a first-match
  // role locator is not a reliable way to dismiss only the visible copy.
  // Remove every guide from visual evidence without changing product state.
  await page.locator(".mc-tab-help-banner").evaluateAll((guides) => {
    for (const guide of guides) {
      (guide as HTMLElement).style.display = "none";
    }
  });
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

function memorySectionButton(
  page: import("./testHarness").Page,
  label: string,
) {
  return page
    .getByTestId("memory-page")
    .locator(".mc-page-section-tabs")
    .getByRole("button", { name: label, exact: true });
}

/**
 * P5 Basement · Memory plant room slice: the stable memory room owns the
 * memory route and lands on the global plant (runtime memory defaults plus
 * an honest per-assistant binding posture board), while Library through
 * Health stay the exact per-agent drill-in. Runtime-default save and source
 * sync drive the real config/sync seams; lane policy saves flow through the
 * one shared People & Routing authority. A Staff card opens the exact
 * requested agent. The hidden memory shortcut pins config-only with
 * full-canvas byte immutability, and the Office door executes the exact
 * stable-room landing through disable/restore and reload. Verified at
 * desktop and 390px with console-error, requestfailed, MP-mark, and
 * inner-rect containment assertions.
 */

test("@core @p5-memory memory room identity, global plant, drill-in parity, shared-authority lane save, pin-to-office, and the office door hold at desktop and 390px", async ({
  page,
}) => {
  test.setTimeout(180_000);
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

  await completeQuickstartLocalOnboarding(page, {
    beforeGoto: async (nextPage) => {
      await nextPage.addInitScript(() => {
        window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
      });
    },
  });

  // The Memory page is optional; turn it on through the real Config seam.
  await page.locator('[data-tour-id="nav-config"]').click();
  await page.getByText("2. Choose what pages show").click();
  await page.getByRole("checkbox", { name: "Memory page" }).check();
  await page.keyboard.press("Escape");

  // The Memory plant room exists once and lights exactly one lamp by stable id.
  const roomButton = page.locator('button[title="BF · Memory plant"]');
  await expect(roomButton).toHaveCount(1);
  const activeRooms = page.locator(".mc-nav-item-active");
  await roomButton.click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Memory plant");

  // Entering the memory room lands on the global plant: runtime defaults plus
  // the per-assistant binding posture board.
  const memoryPage = page.getByTestId("memory-page");
  await expect(memoryPage).toBeVisible();
  await expect(memoryPage).toContainText("Runtime Memory Defaults");
  await expect(memoryPage).toContainText("Binding Posture");
  await expect(memoryPage).toContainText("0 runtime local sources");
  const postureCards = memoryPage.locator(
    ".mc-memory-lane-card-grid .mc-memory-lane-card",
  );
  const assistantPosture = postureCards.filter({ hasText: "Local Assistant" });
  const rootPosture = postureCards.filter({ hasText: "Root" });
  await expect(assistantPosture).toHaveCount(1);
  await expect(rootPosture).toHaveCount(1);
  await expect(assistantPosture).toContainText(
    "memory bound · modelnumquamoblita",
  );
  await expect(rootPosture).toContainText("no memory binding");
  await expect(rootPosture).toContainText("not checked in this sitting");
  // Health is never invented for an assistant that has not been checked.
  await expect(rootPosture).not.toContainText("checked:");

  // The ready plant surface owns the memory pin.
  const pin = page.getByRole("button", { name: "Pin Memory plant to Office" });
  await expect(pin).toBeVisible();

  // Exact drill-in from the posture board: the requested agent opens, not a
  // remembered one.
  await assistantPosture
    .getByRole("button", { name: "Inspect memory" })
    .click();
  const agentSelect = page.getByTestId("memory-agent-select");
  await expect(agentSelect).toHaveValue("default");
  await expect(memoryPage).toContainText(
    "Local Assistant prefers a narrow, operator-readable incident summary.",
  );
  await expect(memoryPage).toContainText("lane: available");

  // Back on the plant, the inspected assistant now shows its server-backed
  // status while the other stays honestly unchecked.
  await memorySectionButton(page, "Plant").click();
  await expect(assistantPosture).toContainText("checked: available");
  await expect(rootPosture).toContainText("not checked in this sitting");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-plant-desktop.png");

  // The honest unconfigured panel for the unbound assistant.
  await memorySectionButton(page, "Library").click();
  await agentSelect.selectOption("agent-root");
  await expect(memoryPage).toContainText(
    "This agent doesn't have memory set up yet.",
  );
  await expect(memoryPage).not.toContainText(
    "Local Assistant prefers a narrow, operator-readable incident summary.",
  );
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-unconfigured-root-desktop.png");
  await agentSelect.selectOption("default");
  await expect(memoryPage).toContainText(
    "Local Assistant prefers a narrow, operator-readable incident summary.",
  );

  // Every drill-in section renders its own real content for the bound agent.
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-library-desktop.png");
  await memoryPage
    .locator(".mc-memory-list-item", {
      hasText:
        "Local Assistant resolved the gateway heartbeat drift during the last reliability pass.",
    })
    .first()
    .click();
  await memorySectionButton(page, "Episodes").click();
  await expect(memoryPage.locator(".mc-memory-list-item").first()).toBeVisible();
  await memorySectionButton(page, "Graph").click();
  await expect(memoryPage).toContainText("Knowledge Graph");
  await expect(memoryPage).toContainText("Related Concepts");
  await expect(memoryPage.locator(".mc-memory-node").first()).toBeVisible();
  await memorySectionButton(page, "Details").click();
  await expect(memoryPage).toContainText("Card dossier");
  await expect(memoryPage).toContainText("Atom detail");
  await memorySectionButton(page, "Reasoning").click();
  await expect(page.getByText("Turn 1", { exact: true })).toBeVisible();
  const citationPill = page.getByRole("button", {
    name: "runtime card excerpt",
    exact: true,
  });
  await expect(citationPill).toBeVisible();
  await citationPill.click();
  await expect(
    page.getByRole("heading", { name: "Citation drilldown" }),
  ).toBeVisible();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-reasoning-desktop.png");
  await memorySectionButton(page, "Health").click();
  await expect(memoryPage).toContainText("Health + Diagnostics");
  await expect(memoryPage).toContainText("Telemetry summary");
  await expect(memoryPage).toContainText("Decision reasons");

  // Runtime memory defaults drive the real config seam with an exact body,
  // and the file sync becomes available only once a saved source exists.
  await memorySectionButton(page, "Plant").click();
  const syncRuntimeButton = page.getByRole("button", {
    name: "Sync runtime files now",
  });
  await expect(syncRuntimeButton).toBeDisabled();
  await memoryPage
    .locator('label:has-text("Runtime local files") textarea')
    .fill("docs/e2e-memory.md");
  const mutationsBeforeDefaultsSave = sensitiveMutations.length;
  await page.getByRole("button", { name: "Save runtime defaults" }).click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Runtime memory defaults saved.",
  );
  await expect(memoryPage).toContainText("1 runtime local source");
  const defaultsMutations = sensitiveMutations.slice(
    mutationsBeforeDefaultsSave,
  );
  expect(defaultsMutations).toHaveLength(1);
  expect(defaultsMutations[0]).toMatchObject({
    method: "POST",
    body: {
      memory: {
        blend_mode: "local_augment",
        memory_md_sources: ["docs/e2e-memory.md"],
      },
    },
  });
  expect(defaultsMutations[0]?.url).toMatch(/\/api\/v1\/config\/runtime$/);
  await dismissVisibleToasts(page);

  await expect(syncRuntimeButton).toBeEnabled();
  const mutationsBeforeRuntimeSync = sensitiveMutations.length;
  await syncRuntimeButton.click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Runtime memory sync complete: 1 file synced.",
  );
  const runtimeSyncMutations = sensitiveMutations.slice(
    mutationsBeforeRuntimeSync,
  );
  expect(runtimeSyncMutations).toHaveLength(1);
  expect(runtimeSyncMutations[0]).toMatchObject({
    method: "POST",
    body: {},
  });
  expect(runtimeSyncMutations[0]?.url).toMatch(
    /\/api\/v1\/memory\/sync$/,
  );
  await dismissVisibleToasts(page);

  // Route a person to this assistant through the one Front Desk authority so
  // the lane surfaces have a real lane to show.
  await page.locator('button[title="BF · Directory / Front Desk"]').click();
  const mailPage = page.getByTestId("mail-page");
  await mailPage
    .locator(".mc-team-routing-card select")
    .first()
    .selectOption({ label: "Local Assistant" });
  await mailPage.getByRole("button", { name: "Save Routing" }).click();
  await expect(mailPage).toContainText("People and routing saved.");
  await dismissVisibleToasts(page);

  // Re-entering the memory room by the elevator relands on the plant, which
  // now shows the routed person and the server-backed lane readiness.
  await roomButton.click();
  await expect(memoryPage).toContainText("Binding Posture");
  await expect(assistantPosture).toContainText("1 routed person");
  await expect(assistantPosture).toContainText("1/1 lanes ready");
  await expect(rootPosture).toContainText("no one routed yet");

  // The lane map's policy editor saves through the shared People & Routing
  // authority: one full-routing write that keeps every other routing fact.
  await memorySectionButton(page, "Routing").click();
  await expect(memoryPage).toContainText("Lane Map");
  await expect(memoryPage).toContainText("routing on");
  const laneCard = memoryPage.locator(".mc-memory-lane-card", {
    hasText: "You",
  });
  await expect(laneCard).toContainText("Memory ready");
  await expect(laneCard).toContainText("lane runtime");
  await laneCard
    .locator('label:has-text("Memory mode") select')
    .selectOption({ label: "Memory + local" });
  await laneCard
    .locator('label:has-text("Lane local files") textarea')
    .fill("notes/e2e-lane.md");
  const mutationsBeforeLaneSave = sensitiveMutations.length;
  await laneCard.getByRole("button", { name: "Save lane settings" }).click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Lane memory settings saved.",
  );
  const laneSaveMutations = sensitiveMutations.slice(mutationsBeforeLaneSave);
  expect(laneSaveMutations).toHaveLength(1);
  expect(laneSaveMutations[0]).toMatchObject({
    method: "POST",
    body: {
      routing: {
        enabled: true,
        local_operator_human_identity_id: "local-operator",
        human_identities: [
          {
            human_identity_id: "local-operator",
            display_name: "You",
            enabled: true,
          },
        ],
        assistant_assignments: [
          {
            human_identity_id: "local-operator",
            assistant_agent_id: "default",
            enabled: true,
          },
        ],
        lane_memory_policies: [
          {
            human_identity_id: "local-operator",
            assistant_agent_id: "default",
            memory_mode: "mno_with_local_sources",
            lane_id: null,
            local_memory_sources: ["notes/e2e-lane.md"],
          },
        ],
      },
    },
  });
  expect(laneSaveMutations[0]?.url).toMatch(/\/api\/v1\/config\/runtime$/);
  await expect(laneCard).toContainText("lane policy set");
  await dismissVisibleToasts(page);

  const mutationsBeforeLaneSync = sensitiveMutations.length;
  await laneCard.getByRole("button", { name: "Sync lane files" }).click();
  await expect(page.locator(".mc-toast").first()).toContainText(
    "Lane memory sync complete: 1 file synced.",
  );
  const laneSyncMutations = sensitiveMutations.slice(mutationsBeforeLaneSync);
  expect(laneSyncMutations).toHaveLength(1);
  expect(laneSyncMutations[0]).toMatchObject({
    method: "POST",
    body: {
      human_identity_id: "local-operator",
      assistant_agent_id: "default",
    },
  });
  expect(laneSyncMutations[0]?.url).toMatch(
    /\/api\/v1\/memory\/sync$/,
  );
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-routing-lanes-desktop.png");

  // A Staff card opens the exact requested agent's drill-in, not whichever
  // agent was selected last. Select the other agent first to prove it.
  await memorySectionButton(page, "Library").click();
  await agentSelect.selectOption("agent-root");
  await expect(memoryPage).toContainText(
    "This agent doesn't have memory set up yet.",
  );
  await page.locator('button[title="2F · Staff Directory"]').click();
  const teamPage = page.getByTestId("team-page");
  await expect(teamPage).toBeVisible();
  const assistantStaffCard = teamPage.locator(".mc-team-card", {
    hasText: "Local Assistant",
  });
  await assistantStaffCard
    .getByRole("button", { name: "Memory", exact: true })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Memory plant");
  await expect(agentSelect).toHaveValue("default");
  await expect(memoryPage).toContainText(
    "Local Assistant prefers a narrow, operator-readable incident summary.",
  );

  // Leave and return by the elevator: the newest explicit room intent is the
  // plant landing, not the drill-in left behind.
  await page.locator('button[title="2F · Staff Directory"]').click();
  await expect(teamPage).toBeVisible();
  await roomButton.click();
  await expect(memoryPage).toContainText("Binding Posture");

  // Pinning is config-only. The full-canvas fixture genuinely contains every
  // earlier registered shortcut, proves its capacity precondition, refuses
  // byte-for-byte, and no runtime-config/memory/mail mutation fires.
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
    ];
    // in-motion (2x2 = 4 cells) + ten 2x1 shortcuts (20 cells) = 24 of 24.
    config.layout = [
      { id: "needs-you", size: "l", visible: false },
      { id: "in-motion", size: "m", visible: true },
      { id: "done", size: "m", visible: false },
      { id: "next", size: "s", visible: false },
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
  expect(fullCanvas.visibleEarlierShortcuts).toBe(10);
  await memorySectionButton(page, "Plant").click();
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
        (placement: { id?: string }) => placement.id === "memory",
      );
    }),
  ).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-full-canvas-refusal.png");

  // Freeing the medium block admits the eleventh shortcut; a repeat pin stays
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
  await captureQaArtifact(page, "memory-pinned-desktop.png");

  // The pinned shortcut appears on the Office canvas, names its floor, and
  // consumes config events live without a route remount.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const shortcut = page.getByTestId("office-block-memory");
  await expect(shortcut).toBeVisible();
  await expect(shortcut).toContainText("The Basement");
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "memory"
          ? { ...placement, visible: false }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(page.getByTestId("office-block-memory")).toHaveCount(0);
  await page.evaluate(() => {
    const key = "mc-glass-config-v1";
    const config = JSON.parse(localStorage.getItem(key) ?? "{}");
    config.layout = config.layout.map(
      (placement: { id: string; visible: boolean }) =>
        placement.id === "memory"
          ? { ...placement, visible: true }
          : placement,
    );
    localStorage.setItem(key, JSON.stringify(config));
    window.dispatchEvent(new Event("mc-glass-config-changed"));
  });
  await expect(shortcut).toBeVisible();
  await shortcut.scrollIntoViewIfNeeded();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-office-shortcut.png");

  // Opening the door lands the exact stable room on the global plant.
  await shortcut.getByRole("button", { name: "Open Memory plant" }).click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Memory plant");
  await expect(memoryPage).toBeVisible();
  await expect(memoryPage).toContainText("Binding Posture");

  // The pin is config: it survives a full reload and still opens the exact
  // room afterwards.
  await page.reload();
  await completeQuickstartLocalOnboarding(page);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const reloadedShortcut = page.getByTestId("office-block-memory");
  await expect(reloadedShortcut).toBeVisible();
  await reloadedShortcut
    .getByRole("button", { name: "Open Memory plant" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Memory plant");
  await expect(memoryPage).toContainText("Binding Posture");

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
    .getByTestId("office-block-memory")
    .getByRole("button", { name: "Open Memory plant" })
    .click();
  await expect(
    page.getByTestId("office-block-memory").getByRole("status"),
  ).toHaveText("Unavailable — turn on in Config");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-disabled-door.png");

  // A disabled Basement also removes the Staff-card entry point instead of
  // leaving a control that can only fail after the boss clicks it.
  await page.locator('button[title="2F · Staff Directory"]').click();
  await expect(teamPage).toBeVisible();
  await expect(
    teamPage.getByRole("button", { name: "Memory", exact: true }),
  ).toHaveCount(0);
  await page.locator('[data-tour-id="nav-assistant"]').click();
  await expect(page.getByTestId("office-block-memory")).toBeVisible();

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
    page.getByTestId("office-block-memory").getByRole("status"),
  ).toHaveCount(0);
  await page
    .getByTestId("office-block-memory")
    .getByRole("button", { name: "Open Memory plant" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Memory plant");
  await expect(memoryPage).toContainText("Binding Posture");
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-restored-door.png");

  // Narrow width: a readable nonzero MP mark that nothing paints over,
  // contained section tabs, and inner geometry for the plant board.
  await page.setViewportSize({ width: 390, height: 844 });
  await prepareQaEvidence(page);
  await roomButton.scrollIntoViewIfNeeded();
  await expect(roomButton).toHaveClass(/mc-nav-item-active/);
  const mark = roomButton.locator(".mc-nav-room-mark");
  await expect(mark).toHaveText("MP");
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

  const plantGeometry = await page.evaluate(() => {
    const containsX = (outer: DOMRect, inner: DOMRect, slack = 1): boolean =>
      inner.width > 0 &&
      inner.height > 0 &&
      inner.left >= outer.left - slack &&
      inner.right <= outer.right + slack;
    const memory = document.querySelector('[data-testid="memory-page"]');
    if (!memory) return { ok: false as const, reason: "no memory page" };
    const memoryRect = memory.getBoundingClientRect();
    const tabs = Array.from(
      memory.querySelectorAll(".mc-page-section-tabs .mc-page-section-btn"),
    ).map((tab) => containsX(memoryRect, tab.getBoundingClientRect()));
    const grid = memory.querySelector(".mc-memory-lane-card-grid");
    if (!grid) return { ok: false as const, reason: "no posture grid" };
    const gridRect = grid.getBoundingClientRect();
    const cards = Array.from(
      grid.querySelectorAll(".mc-memory-lane-card"),
    ).map((card) => containsX(gridRect, card.getBoundingClientRect()));
    return { ok: true as const, tabs, cards };
  });
  expect(plantGeometry.ok).toBe(true);
  if (plantGeometry.ok) {
    expect(plantGeometry.tabs.length).toBe(8);
    expect(plantGeometry.tabs).not.toContain(false);
    // The reload's onboarding pass may add its own assistant; the proof here
    // is containment for every posture card, with both seeded agents present.
    expect(plantGeometry.cards.length).toBeGreaterThanOrEqual(2);
    expect(plantGeometry.cards).not.toContain(false);
  }
  await expect(assistantPosture).toHaveCount(1);
  await expect(rootPosture).toHaveCount(1);
  await assistantPosture.scrollIntoViewIfNeeded();
  await expect(assistantPosture).toBeInViewport();
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-plant-390.png");

  // Every drill-in section stays reachable and contained at 390px. The walk
  // targets the exact bound agent rather than whichever assistant the reload
  // onboarding selected last.
  await memorySectionButton(page, "Library").click();
  await page.getByTestId("memory-agent-select").selectOption("default");
  await expect(memoryPage).toContainText(
    "Local Assistant prefers a narrow, operator-readable incident summary.",
  );
  const sectionProof: Array<[string, string]> = [
    ["Routing", "Lane Map"],
    ["Library", "Memory Cards"],
    ["Episodes", "Episodes"],
    ["Graph", "Knowledge Graph"],
    ["Details", "Card dossier"],
    ["Reasoning", "Citation drilldown"],
    ["Health", "Health + Diagnostics"],
  ];
  for (const [label, expected] of sectionProof) {
    const tab = memorySectionButton(page, label);
    await tab.scrollIntoViewIfNeeded();
    await tab.click();
    await expect(memoryPage).toContainText(expected);
    const contained = await page.evaluate(() => {
      const memory = document.querySelector('[data-testid="memory-page"]');
      if (!memory) return false;
      const memoryRect = memory.getBoundingClientRect();
      const panels = Array.from(
        memory.querySelectorAll(".mc-memory-panel, .mc-memory-state"),
      );
      if (panels.length === 0) return false;
      return panels.every((panel) => {
        const rect = panel.getBoundingClientRect();
        return (
          rect.width > 0 &&
          rect.left >= memoryRect.left - 1 &&
          rect.right <= memoryRect.right + 1
        );
      });
    });
    expect(contained, `section ${label} panels contained at 390px`).toBe(true);
  }
  await memorySectionButton(page, "Library").click();
  const mobileAgentSelect = page.getByTestId("memory-agent-select");
  await mobileAgentSelect.scrollIntoViewIfNeeded();
  await expect(mobileAgentSelect).toBeVisible();
  expect(
    await page.evaluate(() => {
      const select = document.querySelector(
        '[data-testid="memory-agent-select"]',
      );
      const panel = select?.closest(".mc-memory-panel");
      if (!select || !panel) return null;
      const panelRect = panel.getBoundingClientRect();
      const rect = select.getBoundingClientRect();
      return (
        rect.width > 0 &&
        rect.left >= panelRect.left - 1 &&
        rect.right <= panelRect.right + 1
      );
    }),
  ).toBe(true);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-library-390.png");

  // The Office door stays reachable and contained at 390px and still opens
  // the exact room.
  await page.locator('[data-tour-id="nav-assistant"]').click();
  const mobileShortcut = page.getByTestId("office-block-memory");
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
    .getByRole("button", { name: "Open Memory plant" })
    .click();
  await expect(activeRooms).toHaveCount(1);
  await expect(activeRooms).toHaveAttribute("title", "BF · Memory plant");
  await expect(memoryPage).toContainText("Binding Posture");

  const overflows = await page.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth,
  );
  expect(overflows).toBe(false);
  await dismissVisibleToasts(page);
  await captureQaArtifact(page, "memory-office-door-390.png");

  expect(browserErrors).toEqual([]);
});
