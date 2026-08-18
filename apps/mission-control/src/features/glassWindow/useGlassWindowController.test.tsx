// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import {
  getFloorPresence,
  getOfficeChatter,
  postOfficeChatterMessage,
} from "../../glass/window/api";
import type { RuntimeConnectionSettings } from "../../types";
import {
  type GlassWindowController,
  useGlassWindowController,
} from "./useGlassWindowController";

vi.mock("../../glass/window/api", () => ({
  getFloorPresence: vi.fn(),
  getOfficeChatter: vi.fn(),
  postOfficeChatterMessage: vi.fn(),
}));

const settings: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

let container: HTMLDivElement;
let root: Root | null = null;
let controller: GlassWindowController | null = null;

function Harness(props: { tokenConfigured: boolean }) {
  const current = useGlassWindowController({
    active: false,
    settings,
    tokenConfigured: props.tokenConfigured,
  });
  useEffect(() => {
    controller = current;
  });
  return null;
}

async function render(tokenConfigured: boolean) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(<Harness tokenConfigured={tokenConfigured} />);
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  vi.clearAllMocks();
});

afterEach(async () => {
  await act(async () => root?.unmount());
  container.remove();
  root = null;
  controller = null;
});

describe("useGlassWindowController", () => {
  test("does not resurrect an in-flight Window result after disconnect", async () => {
    let resolvePresence!: (value: {
      generated_at_ms: number;
      refresh_after_ms: number;
      items: [];
    }) => void;
    let resolveChatter!: (value: { rooms: []; messages: [] }) => void;
    vi.mocked(getFloorPresence).mockReturnValue(
      new Promise((resolve) => {
        resolvePresence = resolve;
      }),
    );
    vi.mocked(getOfficeChatter).mockReturnValue(
      new Promise((resolve) => {
        resolveChatter = resolve;
      }),
    );

    await render(true);
    await act(async () => {
      void controller!.refresh();
      await Promise.resolve();
    });
    await render(false);
    await act(async () => {
      await controller!.refresh();
      resolvePresence({ generated_at_ms: 1, refresh_after_ms: 5_000, items: [] });
      resolveChatter({ rooms: [], messages: [] });
      await Promise.resolve();
    });

    expect(controller?.presence).toBeNull();
    expect(controller?.chatter).toBeNull();
    expect(controller?.error).toBe("Connect CarsinOS to observe the Window.");
  });

  test("keeps a posted message successful when only the follow-up refresh fails", async () => {
    vi.mocked(postOfficeChatterMessage).mockResolvedValue({
      message: {
        message_id: "message-1",
        thread_id: "thread-1",
        author: { kind: "owner", display_name: "Owner" },
        text: "hello",
        created_at_ms: 1,
        source: {
          kind: "owner_message",
          event_name: null,
          workstream_id: "work-1",
          revision: null,
        },
      },
    });
    vi.mocked(getOfficeChatter).mockRejectedValue(new Error("refresh failed"));
    await render(true);

    let sent = false;
    await act(async () => {
      sent = await controller!.sendMessage("thread-1", " hello ");
    });

    expect(sent).toBe(true);
    expect(controller?.error).toBe("Note sent, but the chatter feed could not refresh.");
  });

  test("admits only one owner-note send while the request is in flight", async () => {
    let finishPost!: () => void;
    vi.mocked(postOfficeChatterMessage).mockReturnValue(
      new Promise((resolve) => {
        finishPost = () =>
          resolve({
            message: {
              message_id: "message-1",
              thread_id: "thread-1",
              author: { kind: "owner", display_name: "Owner" },
              text: "hello",
              created_at_ms: 1,
              source: {
                kind: "owner_message",
                event_name: null,
                workstream_id: "work-1",
                revision: null,
              },
            },
          });
      }),
    );
    vi.mocked(getOfficeChatter).mockResolvedValue({ rooms: [], messages: [] });
    await render(true);

    let first!: Promise<boolean>;
    let second!: Promise<boolean>;
    await act(async () => {
      first = controller!.sendMessage("thread-1", "hello");
      second = controller!.sendMessage("thread-1", "hello again");
      await Promise.resolve();
    });
    expect(postOfficeChatterMessage).toHaveBeenCalledTimes(1);
    expect(controller?.sending).toBe(true);
    await expect(second).resolves.toBe(false);

    await act(async () => {
      finishPost();
      await first;
    });
    expect(controller?.sending).toBe(false);
  });

  test("does not let an older refresh overwrite chatter fetched after a send", async () => {
    let finishPresence!: () => void;
    let finishOldChatter!: () => void;
    let finishNewChatter!: () => void;
    vi.mocked(getFloorPresence).mockReturnValue(
      new Promise((resolve) => {
        finishPresence = () =>
          resolve({ generated_at_ms: 1, refresh_after_ms: 5_000, items: [] });
      }),
    );
    vi.mocked(getOfficeChatter)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          finishOldChatter = () =>
            resolve({
              rooms: [],
              messages: [
                {
                  message_id: "old",
                  thread_id: "thread-1",
                  author: { kind: "execass", display_name: "ExecAss" },
                  text: "old",
                  created_at_ms: 1,
                  source: {
                    kind: "execass_event",
                    event_name: null,
                    workstream_id: "work-1",
                    revision: null,
                  },
                },
              ],
            });
        }),
      )
      .mockReturnValueOnce(
        new Promise((resolve) => {
          finishNewChatter = () =>
            resolve({
              rooms: [],
              messages: [
                {
                  message_id: "new",
                  thread_id: "thread-1",
                  author: { kind: "owner", display_name: "Owner" },
                  text: "new",
                  created_at_ms: 2,
                  source: {
                    kind: "owner_message",
                    event_name: null,
                    workstream_id: "work-1",
                    revision: null,
                  },
                },
              ],
            });
        }),
      );
    vi.mocked(postOfficeChatterMessage).mockResolvedValue({
      message: {
        message_id: "new",
        thread_id: "thread-1",
        author: { kind: "owner", display_name: "Owner" },
        text: "new",
        created_at_ms: 2,
        source: {
          kind: "owner_message",
          event_name: null,
          workstream_id: "work-1",
          revision: null,
        },
      },
    });
    await render(true);

    let refresh!: Promise<boolean>;
    let send!: Promise<boolean>;
    await act(async () => {
      refresh = controller!.refresh();
      await Promise.resolve();
      send = controller!.sendMessage("thread-1", "new");
      await Promise.resolve();
      finishNewChatter();
      await send;
    });
    expect(controller?.chatter?.messages[0]?.message_id).toBe("new");

    await act(async () => {
      finishPresence();
      finishOldChatter();
      await refresh;
    });
    expect(controller?.chatter?.messages[0]?.message_id).toBe("new");
  });
});
