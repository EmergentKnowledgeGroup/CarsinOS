import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { getGatewayToken } from "../../lib/runtime";
import type { RuntimeConnectionSettings } from "../../types";
import {
  getFloorPresence,
  OFFICE_REQUEST_TIMEOUT_MS,
} from "./api";

vi.mock("../../lib/runtime", () => ({
  getGatewayToken: vi.fn(),
}));

const settings: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

describe("Window API", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(getGatewayToken).mockResolvedValue("token-123");
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  test("aborts a stalled request so the Window refresh can recover", async () => {
    const fetchMock = vi.fn(
      (_url: string, init: RequestInit) =>
        new Promise<Response>((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => {
            reject(new DOMException("Timed out", "AbortError"));
          });
        }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const request = getFloorPresence(settings);
    await Promise.resolve();
    const rejected = expect(request).rejects.toMatchObject({ name: "AbortError" });
    await vi.advanceTimersByTimeAsync(OFFICE_REQUEST_TIMEOUT_MS);

    await rejected;
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(init.signal?.aborted).toBe(true);
  });
});
