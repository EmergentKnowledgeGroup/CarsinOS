// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { connectGatewayEvents } from "../lib/ws";
import type { RuntimeConnectionSettings } from "../types";
import { useGatewayEvents } from "./useGatewayEvents";

vi.mock("../lib/ws", () => ({
  connectGatewayEvents: vi.fn(),
}));

const settings: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

function Harness(props: { authIdentityGeneration: number }) {
  useGatewayEvents({
    settings,
    tokenConfigured: true,
    authIdentityGeneration: props.authIdentityGeneration,
    onState: vi.fn(),
    onEvent: vi.fn(),
  });
  return null;
}

let root: Root | null = null;
let container: HTMLDivElement | null = null;

afterEach(async () => {
  await act(async () => {
    root?.unmount();
  });
  container?.remove();
  root = null;
  container = null;
  vi.clearAllMocks();
});

describe("useGatewayEvents", () => {
  it("closes and reconnects the one durable stream when auth changes at the same URL", async () => {
    const closeFirst = vi.fn();
    const closeSecond = vi.fn();
    vi.mocked(connectGatewayEvents)
      .mockReturnValueOnce({ close: closeFirst })
      .mockReturnValueOnce({ close: closeSecond });
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);

    await act(async () => {
      root!.render(<Harness authIdentityGeneration={0} />);
    });
    expect(connectGatewayEvents).toHaveBeenCalledTimes(1);

    await act(async () => {
      root!.render(<Harness authIdentityGeneration={1} />);
    });
    expect(closeFirst).toHaveBeenCalledTimes(1);
    expect(connectGatewayEvents).toHaveBeenCalledTimes(2);
    expect(closeSecond).not.toHaveBeenCalled();
  });
});
