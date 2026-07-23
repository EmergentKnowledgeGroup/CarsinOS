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
});
