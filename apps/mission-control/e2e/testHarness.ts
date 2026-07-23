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
    await page.addInitScript(() => {
      const proofBase = {
        authenticated_client_id: "e2e-browser",
        proof_hex: "00".repeat(32),
      };
      (
        window as unknown as {
          __CARSINOS_E2E_TEST_SIGNER__: (
            kind: string,
            payload: Record<string, unknown>,
          ) => Record<string, unknown>;
        }
      ).__CARSINOS_E2E_TEST_SIGNER__ = (kind, payload) => {
        if (kind === "intake") {
          return {
            ...proofBase,
            request_correlation_id: payload.source_correlation_id,
            request_id: payload.request_id,
            idempotency_key: payload.idempotency_key,
            attach_to_delegation_id:
              payload.attach_to_delegation_id ?? null,
            normalized_intent_digest: "sha256:e2e",
            instruction_digest: "sha256:e2e",
          };
        }
        if (kind === "decision") {
          return {
            ...proofBase,
            request_correlation_id: payload.request_correlation_id,
          };
        }
        return {
          ...proofBase,
          request_correlation_id: payload.request_correlation_id,
        };
      };
    });
    await runTest(page);
  },
});

export { expect };
export type { APIRequestContext, Page };
