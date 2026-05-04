import {
  expect,
  test,
  type APIRequestContext,
  type Page,
} from "./testHarness";
import {
  completeQuickstartLocalOnboarding,
  GATEWAY_URL,
  TEST_TOKEN,
} from "./onboardingFlow";

interface BoardCard {
  card_id: string;
  board_id: string;
  column_id: string;
  title: string;
  latest_run_id: string | null;
}

interface BoardDetailResponse {
  cards: BoardCard[];
}

interface Approval {
  approval_id: string;
  status: string;
}

interface ApprovalListResponse {
  items: Approval[];
}

async function waitForWsConnected(page: Page): Promise<void> {
  const wsDot = page.locator(".mc-connection-dot").first();
  await expect(wsDot).toBeVisible({ timeout: 20_000 });
  await expect
    .poll(async () => wsDot.getAttribute("title"), {
      timeout: 20_000,
      message: "Expected websocket status indicator to reach connected state.",
    })
    .toBe("ws: connected");
}

async function fetchBoardDetail(
  request: APIRequestContext
): Promise<BoardDetailResponse> {
  const response = await request.get(`${GATEWAY_URL}/api/v1/boards/ops-board`, {
    headers: {
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
  });
  expect(response.ok()).toBeTruthy();
  return (await response.json()) as BoardDetailResponse;
}

async function fetchRequestedApprovals(
  request: APIRequestContext
): Promise<ApprovalListResponse> {
  const response = await request.get(
    `${GATEWAY_URL}/api/v1/approvals?status=requested&limit=100`,
    {
      headers: {
        Authorization: `Bearer ${TEST_TOKEN}`,
      },
    }
  );
  expect(response.ok()).toBeTruthy();
  return (await response.json()) as ApprovalListResponse;
}

test.describe("mission-control phase 3 operator workflows @p3", () => {
  test("boards workflow supports create/move/run with persistence across refresh", async ({
    page,
    request,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });
    await waitForWsConnected(page);

    const cardTitle = `P3 workflow card ${Date.now()}`;
    let createdCardId = "";
    await page.getByRole("button", { name: "+ New Card" }).click();
    await expect(page.getByRole("heading", { name: "New Card" })).toBeVisible();
    await page.getByLabel("Title").fill(cardTitle);
    await page.getByLabel("Column").selectOption({ label: "Doing" });
    await page.getByRole("button", { name: "Create Card" }).click();
    await expect(page.locator(".mc-card-title").filter({ hasText: cardTitle })).toBeVisible();

    await expect
      .poll(async () => {
        const detail = await fetchBoardDetail(request);
        const created = detail.cards.find((item) => item.title === cardTitle);
        createdCardId = created?.card_id ?? "";
        return createdCardId.length > 0;
      })
      .toBe(true);

    const cardTile = page.locator(".mc-card").filter({ hasText: cardTitle }).first();
    const doneLane = page.locator(".mc-lane").filter({
      has: page.getByRole("heading", { name: "Done" }),
    });
    const dataTransfer = await page.evaluateHandle(() => new DataTransfer());
    await cardTile.dispatchEvent("dragstart", { dataTransfer });
    await doneLane.locator(".mc-lane-body").dispatchEvent("drop", { dataTransfer });
    await cardTile.dispatchEvent("dragend", { dataTransfer });

    await expect
      .poll(async () => {
        const detail = await fetchBoardDetail(request);
        const moved = detail.cards.find((item) => item.card_id === createdCardId);
        return moved?.column_id ?? "";
      })
      .toBe("ops-done");

    await page.locator(".mc-card").filter({ hasText: cardTitle }).first().click();
    await expect(page.getByRole("button", { name: "Run Card" })).toBeVisible();
    await page.getByRole("button", { name: "Run Card" }).click();

    let runId = "";
    await expect
      .poll(async () => {
        const detail = await fetchBoardDetail(request);
        const updated = detail.cards.find((item) => item.card_id === createdCardId);
        runId = updated?.latest_run_id ?? "";
        return runId;
      })
      .toMatch(/^run-\d{4}$/);
    await expect(page.locator(".mc-modal-subtitle").filter({ hasText: runId })).toBeVisible();

    await page.reload();
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });
    await waitForWsConnected(page);
    await expect(page.locator(".mc-card-title").filter({ hasText: cardTitle })).toBeVisible();

    await expect
      .poll(async () => {
        const detail = await fetchBoardDetail(request);
        const refreshed = detail.cards.find((item) => item.card_id === createdCardId);
        return {
          column_id: refreshed?.column_id ?? "",
          latest_run_id: refreshed?.latest_run_id ?? "",
        };
      })
      .toEqual({
        column_id: "ops-done",
        latest_run_id: runId,
      });
  });

  test("focus approvals workflow resolves approve+deny and updates pending count", async ({
    page,
    request,
  }) => {
    await completeQuickstartLocalOnboarding(page, {
      beforeGoto: async (nextPage) => {
        await nextPage.addInitScript(() => {
          window.localStorage.setItem("mc-guided-tour-completed-v1", "true");
        });
      },
    });
    await waitForWsConnected(page);

    await page.locator('[data-tour-id="nav-focus"]').click();
    await expect(page.getByText("Operator Focus Queue")).toBeVisible();
    await expect(page.getByText(/Approvals:\s*2/)).toBeVisible();

    const approvalsBefore = await fetchRequestedApprovals(request);
    expect(approvalsBefore.items).toHaveLength(2);

    const approveItem = page
      .locator(".mc-focus-item")
      .filter({ hasText: "Allow shell command: ls -la" });
    await approveItem.getByRole("button", { name: "Approve" }).click();

    await expect
      .poll(async () => (await fetchRequestedApprovals(request)).items.length)
      .toBe(1);

    const denyItem = page
      .locator(".mc-focus-item")
      .filter({ hasText: "Allow file edit: docs/release.md" });
    await denyItem.getByRole("button", { name: "Deny" }).click();

    await expect
      .poll(async () => (await fetchRequestedApprovals(request)).items.length)
      .toBe(0);

    await expect(page.getByText(/Approvals:\s*0/)).toBeVisible();
    await expect(
      page.getByText("Approval requested: Allow shell command: ls -la")
    ).toHaveCount(0);
    await expect(
      page.getByText("Approval requested: Allow file edit: docs/release.md")
    ).toHaveCount(0);
  });
});
