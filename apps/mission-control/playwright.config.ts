import { defineConfig } from "@playwright/test";

const appPort = 1420;
const gatewayPort = 19_789;
const localBaseUrl = `http://127.0.0.1:${appPort}`;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  timeout: 45_000,
  expect: {
    timeout: 10_000,
  },
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: "list",
  use: {
    baseURL: process.env.MC_E2E_BASE_URL || localBaseUrl,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: [
    {
      command: `node ./e2e/mockGateway.mjs --port ${gatewayPort}`,
      port: gatewayPort,
      timeout: 120_000,
      reuseExistingServer: false,
    },
    {
      command: `npm run dev -- --host 127.0.0.1 --port ${appPort}`,
      port: appPort,
      timeout: 120_000,
      reuseExistingServer: !process.env.CI,
    },
  ],
});
