import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ExecassWsFrame } from "../glass/execass/types";
import type { WsEventFrame } from "../types";
import { createWebSocketTicket } from "./api";
import { getGatewayToken } from "./runtime";
import { connectGatewayEvents } from "./ws";

vi.mock("./runtime", () => ({
  getGatewayToken: vi.fn(),
}));

vi.mock("./api", () => ({
  createWebSocketTicket: vi.fn(),
  websocketUrlFromGateway: vi.fn(
    () => "ws://127.0.0.1:18789/api/v1/ws?ticket=ticket",
  ),
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
  public sent: string[] = [];
  public closed = false;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send(text: string) {
    this.sent.push(text);
  }

  close() {
    this.closed = true;
    this.onclose?.();
  }
}

function legacyFrame(): string {
  return JSON.stringify({
    schema_version: "v1",
    event_id: "evt-1",
    event_type: "board.card.created",
    ts_unix_ms: 1,
    entity: "board",
    payload: { summary: "x" },
  });
}

function execassEventFrame(): string {
  return JSON.stringify({
    type: "execass.v1.event",
    event: {
      event_name: "execass.v1.summary.changed",
      aggregate_id: "summary",
      revision: 1,
      correlation_id: "c",
      causation_id: "c",
      occurred_at_ms: 1,
      schema_version: "v1",
      safe_payload: { summary: "changed" },
      global_sequence: 7,
      duplicate_identity: "dup-7",
    },
  });
}

describe("connectGatewayEvents execass routing", () => {
  const originalWebSocket = globalThis.WebSocket;

  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.useFakeTimers();
    vi.clearAllMocks();
    vi.mocked(getGatewayToken).mockResolvedValue("token");
    vi.mocked(createWebSocketTicket).mockResolvedValue({
      ticket: "ticket",
      expires_at: 1234,
    });
    (globalThis as { WebSocket: typeof WebSocket }).WebSocket =
      MockWebSocket as unknown as typeof WebSocket;
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    (globalThis as { WebSocket: typeof WebSocket }).WebSocket = originalWebSocket;
  });

  async function open(options: {
    onEvent?: (frame: WsEventFrame) => void;
    onExecassFrame?: (frame: ExecassWsFrame) => void;
    onOpen?: (send: (text: string) => void) => void;
  }) {
    connectGatewayEvents({
      settings: { gateway_url: "http://127.0.0.1:18789" },
      onEvent: options.onEvent ?? (() => {}),
      onState: () => {},
      onExecassFrame: options.onExecassFrame,
      onOpen: options.onOpen,
    });
    await vi.runAllTimersAsync();
    const socket = MockWebSocket.instances[0]!;
    socket.onopen?.();
    return socket;
  }

  it("routes execass frames to onExecassFrame and not to onEvent", async () => {
    const events: WsEventFrame[] = [];
    const execass: ExecassWsFrame[] = [];
    const socket = await open({
      onEvent: (f) => events.push(f),
      onExecassFrame: (f) => execass.push(f),
    });
    socket.onmessage?.({ data: execassEventFrame() });
    socket.onmessage?.({ data: legacyFrame() });
    expect(execass).toHaveLength(1);
    expect(execass[0]?.type).toBe("execass.v1.event");
    expect(events).toHaveLength(1);
    expect(events[0]?.event_type).toBe("board.card.created");
  });

  it("routes summary_refetch_required frames to onExecassFrame", async () => {
    const execass: ExecassWsFrame[] = [];
    const socket = await open({ onExecassFrame: (f) => execass.push(f) });
    socket.onmessage?.({
      data: JSON.stringify({
        type: "execass.v1.summary_refetch_required",
        reason: "gap",
        consumer_cursor: 5,
        requested_cursor: 9,
        head_global_sequence: 12,
      }),
    });
    expect(execass).toHaveLength(1);
    expect(execass[0]?.type).toBe("execass.v1.summary_refetch_required");
  });

  it("drops execass frames silently when no handler is registered", async () => {
    const events: WsEventFrame[] = [];
    const socket = await open({ onEvent: (f) => events.push(f) });
    socket.onmessage?.({ data: execassEventFrame() });
    expect(events).toHaveLength(0);
  });

  it("exposes a send function once the socket opens", async () => {
    let send: ((text: string) => void) | null = null;
    const socket = await open({ onOpen: (s) => (send = s) });
    expect(send).not.toBeNull();
    send!('{"type":"execass.v1.resume","client_id":"c","cursor":0}');
    expect(socket.sent).toEqual([
      '{"type":"execass.v1.resume","client_id":"c","cursor":0}',
    ]);
  });
});
