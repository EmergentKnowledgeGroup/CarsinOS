import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WS_RECONNECT_INITIAL_MS } from "../constants";
import type { WsEventFrame } from "../types";
import { connectGatewayEvents } from "./ws";
import { getGatewayToken } from "./runtime";
import { createWebSocketTicket, websocketUrlFromGateway } from "./api";

vi.mock("./runtime", () => ({
  getGatewayToken: vi.fn(),
}));

vi.mock("./api", () => ({
  createWebSocketTicket: vi.fn(),
  websocketUrlFromGateway: vi.fn(() => "ws://127.0.0.1:18789/api/v1/ws?ticket=ticket"),
}));

interface MockMessageEvent {
  data: string;
}

class MockWebSocket {
  static instances: MockWebSocket[] = [];

  public readonly url: string;
  public onopen: (() => void) | null = null;
  public onmessage: ((event: MockMessageEvent) => void) | null = null;
  public onclose: (() => void) | null = null;
  public onerror: (() => void) | null = null;
  public closed = false;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  close() {
    this.closed = true;
    this.onclose?.();
  }
}

describe("connectGatewayEvents", () => {
  const originalWebSocket = globalThis.WebSocket;

  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.useFakeTimers();
    vi.clearAllMocks();
    vi.mocked(createWebSocketTicket).mockResolvedValue({
      ticket: "ticket",
      expires_at: 1234,
    });
    (globalThis as { WebSocket: typeof WebSocket }).WebSocket = MockWebSocket as unknown as typeof WebSocket;
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    (globalThis as { WebSocket: typeof WebSocket }).WebSocket = originalWebSocket;
  });

  it("stays idle when no token is configured", async () => {
    vi.mocked(getGatewayToken).mockResolvedValue(null);
    const states: string[] = [];

    connectGatewayEvents({
      settings: { gateway_url: "http://127.0.0.1:18789" },
      onEvent: () => {},
      onState: (state) => states.push(state),
    });

    await vi.runAllTimersAsync();
    expect(states).toContain("idle");
    expect(MockWebSocket.instances).toHaveLength(0);
  });

  it("connects and ignores malformed ws payloads", async () => {
    vi.mocked(getGatewayToken).mockResolvedValue("token");
    const states: string[] = [];
    const events: WsEventFrame[] = [];

    connectGatewayEvents({
      settings: { gateway_url: "http://127.0.0.1:18789" },
      onEvent: (frame) => events.push(frame),
      onState: (state) => states.push(state),
    });

    await vi.runAllTimersAsync();
    expect(MockWebSocket.instances).toHaveLength(1);
    expect(vi.mocked(createWebSocketTicket)).toHaveBeenCalledWith({
      gateway_url: "http://127.0.0.1:18789",
    });
    expect(vi.mocked(websocketUrlFromGateway)).toHaveBeenCalledWith(
      { gateway_url: "http://127.0.0.1:18789" },
      "ticket"
    );

    const socket = MockWebSocket.instances[0];
    socket.onopen?.();
    socket.onmessage?.({ data: "this-is-not-json" });
    socket.onmessage?.({
      data: JSON.stringify({
        schema_version: "v1",
        event_id: "evt_1",
        event_type: "job.updated",
        ts_unix_ms: 1000,
        request_id: null,
        entity: "jobs",
        payload: { job_id: "job_1" },
      }),
    });

    expect(states).toContain("connecting");
    expect(states).toContain("connected");
    expect(events).toHaveLength(1);
    expect(events[0]?.event_id).toBe("evt_1");
  });

  it("schedules reconnect and re-attempts websocket connection", async () => {
    vi.mocked(getGatewayToken).mockResolvedValue("token");
    const states: string[] = [];

    connectGatewayEvents({
      settings: { gateway_url: "http://127.0.0.1:18789" },
      onEvent: () => {},
      onState: (state) => states.push(state),
    });

    await vi.runAllTimersAsync();
    const firstSocket = MockWebSocket.instances[0];
    firstSocket.onopen?.();
    firstSocket.onclose?.();

    expect(states).toContain("reconnecting");

    await vi.advanceTimersByTimeAsync(WS_RECONNECT_INITIAL_MS);
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2);
  });

  it("respects max reconnect attempts cap", async () => {
    vi.mocked(getGatewayToken).mockResolvedValue("token");
    const states: string[] = [];

    connectGatewayEvents({
      settings: { gateway_url: "http://127.0.0.1:18789" },
      onEvent: () => {},
      onState: (state) => states.push(state),
      maxReconnectAttempts: 0,
    });

    await vi.runAllTimersAsync();
    const firstSocket = MockWebSocket.instances[0];
    firstSocket.onclose?.();

    expect(states.at(-1)).toBe("error");
  });
});
