// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { GlassWindowPage } from "./GlassWindowPage";
import type { GlassWindowController } from "./useGlassWindowController";

let root: Root | null = null;
let container: HTMLDivElement;

afterEach(async () => {
  await act(async () => root?.unmount());
  container.remove();
  root = null;
});

describe("GlassWindowPage", () => {
  it("renders only the safe projection DTO and preserves unknown presence", async () => {
    const controller = {
      presence: {
        generated_at_ms: 10,
        refresh_after_ms: 5_000,
        items: [
          {
            agent_id: "vale",
            display_name: "Vale",
            activity: "unknown",
            activity_label: "No recent observation",
            mood: "unknown",
            observed_at_ms: null,
          source: "local_storage",
            target: null,
          },
        ],
      },
      chatter: {
        rooms: [
          {
            thread_id: "room-1",
            workstream_id: "delegation-1",
            label: "Workstream 1",
            unread_count: null,
            last_activity_at_ms: 10,
          },
        ],
        messages: [
          {
            message_id: "message-1",
            thread_id: "room-1",
            author: { kind: "execass", display_name: "ExecAss" },
            text: "A workstream moved into planning.",
            created_at_ms: 10,
            source: {
              kind: "execass_event",
              event_name: "execass.v1.delegation.transitioned",
              workstream_id: "delegation-1",
              revision: 1,
            },
          },
        ],
      },
      error: null,
      loading: false,
      sending: false,
      refresh: vi.fn().mockResolvedValue(true),
      sendMessage: vi.fn().mockResolvedValue(true),
    } as GlassWindowController;

    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(<GlassWindowPage controller={controller} />);
    });

    expect(container.textContent).toContain("Vale");
    expect(container.textContent).toContain("No recent observation");
    expect(container.textContent).not.toContain("offline");
    expect(container.textContent).toContain(
      "A workstream moved into planning.",
    );
  });

  it("shows an honest unavailable state without inventing activity", async () => {
    const controller = {
      presence: null,
      chatter: null,
      error: "The Window is not wired yet.",
      loading: false,
      sending: false,
      refresh: vi.fn().mockResolvedValue(false),
      sendMessage: vi.fn().mockResolvedValue(false),
    } as unknown as GlassWindowController;

    container = document.createElement("div");
    document.body.appendChild(container);
    await act(async () => {
      root = createRoot(container);
      root.render(<GlassWindowPage controller={controller} />);
    });

    expect(container.textContent).toContain("Window unavailable");
    expect(container.textContent).toContain("not wired yet");
    expect(container.textContent).not.toContain("offline");
  });
});
