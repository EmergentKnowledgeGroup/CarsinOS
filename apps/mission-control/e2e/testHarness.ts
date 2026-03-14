import {
  expect,
  test as base,
  type APIRequestContext,
  type Page,
} from "@playwright/test";

const GATEWAY_URL = "http://127.0.0.1:19789";
const TEST_TOKEN = "stub-token-001";

async function resetGatewayState(request: APIRequestContext): Promise<void> {
  const response = await request.post(`${GATEWAY_URL}/api/v1/e2e/reset`, {
    headers: {
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
  });
  expect(response.ok()).toBeTruthy();
}

export const test = base.extend<{
  page: Page;
}>({
  page: async ({ page, request }, runTest) => {
    await resetGatewayState(request);
    await runTest(page);
  },
});

export { expect };
export type { APIRequestContext, Page };
