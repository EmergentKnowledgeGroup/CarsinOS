// @vitest-environment jsdom

import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  FloorPresenceItem,
  OfficeChatterResponse,
} from "../../glass/window/types";
import { GlassWindowPage } from "./GlassWindowPage";
import type { GlassWindowController } from "./useGlassWindowController";

let root: Root | null = null;
let container: HTMLDivElement;

function execass(): FloorPresenceItem {
  return {
    agent_id: "agent-execass",
    display_name: "ExecAss",
    activity: "busy",
    activity_label: "Working",
    mood: "focused",
    observed_at_ms: Date.now() - 2_000,
    source: "local_storage",
    target: { kind: "run", id: "run-9" },
  };
}

function vale(): FloorPresenceItem {
  return {
    agent_id: "agent-vale",
    display_name: "Vale",
    activity: "unknown",
    activity_label: "No recent observation",
    mood: "unknown",
    observed_at_ms: null,
    source: "local_storage",
    target: null,
  };
}

function chatter(): OfficeChatterResponse {
  return {
    rooms: [
      {
        thread_id: "room-old",
        workstream_id: "w-old",
        label: "archive sweep",
        unread_count: 3,
        last_activity_at_ms: Date.now() - 60 * 60_000,
      },
      {
        thread_id: "room-new",
        workstream_id: "w-new",
        label: "launch",
        unread_count: null,
        last_activity_at_ms: Date.now() - 1_000,
      },
    ],
    messages: [
      {
        message_id: "m-1",
        thread_id: "room-new",
        author: { kind: "execass", display_name: "ExecAss" },
        text: "The launch brief moved into active work.",
        created_at_ms: Date.now() - 120_000,
        source: {
          kind: "execass_event",
          event_name: "execass.v1.delegation.transitioned",
          workstream_id: "w-new",
          revision: 1,
        },
      },
      {
        message_id: "m-2",
        thread_id: "room-new",
        author: { kind: "execass", display_name: "ExecAss" },
        text: "Venue shortlist drafted.",
        created_at_ms: Date.now() - 60_000,
        source: {
          kind: "execass_event",
          event_name: "execass.v1.continuation.claimed_or_result_recorded",
          workstream_id: "w-new",
          revision: 2,
        },
      },
      {
        message_id: "m-3",
        thread_id: "room-new",
        author: { kind: "owner", display_name: "You" },
        text: "Keep the caterer on hold.",
        created_at_ms: Date.now() - 30_000,
        source: {
          kind: "owner_message",
          event_name: null,
          workstream_id: "w-new",
          revision: null,
        },
      },
    ],
  };
}

function makeController(
  overrides: Partial<GlassWindowController> = {},
): GlassWindowController {
  return {
    presence: {
      generated_at_ms: Date.now(),
      refresh_after_ms: 5_000,
      items: [vale(), execass()],
    },
    chatter: chatter(),
    error: null,
    loading: false,
    sending: false,
    refresh: vi.fn().mockResolvedValue(true),
    sendMessage: vi.fn().mockResolvedValue(true),
    ...overrides,
  } as unknown as GlassWindowController;
}

async function mount(
  controller: GlassWindowController,
  onOpenTarget = vi.fn().mockReturnValue(true),
) {
  container = document.createElement("div");
  document.body.appendChild(container);
  await act(async () => {
    root = createRoot(container);
    root.render(
      <StrictMode>
        <GlassWindowPage controller={controller} onOpenTarget={onOpenTarget} />
      </StrictMode>,
    );
  });
  return onOpenTarget;
}

function crab(name: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find(
    (b) => b.getAttribute("aria-label") === `${name}'s report card`,
  );
  expect(button, `crab not found: ${name}`).toBeDefined();
  return button as HTMLButtonElement;
}

afterEach(async () => {
  await act(async () => root?.unmount());
  container.remove();
  root = null;
});

describe("Reef report cards", () => {
  it("leads with the distinct ExecAss crab", async () => {
    await mount(makeController());
    const agents = Array.from(
      container.querySelectorAll("[data-testid='reef-crab']"),
    );
    expect(agents[0]?.textContent).toContain("ExecAss");
    expect(agents[0]?.className).toContain("is-execass");
  });

  it("opens a report card with honest freshness and mood", async () => {
    await mount(makeController());
    await act(async () => crab("Vale").click());
    const card = container.querySelector("[data-testid='reef-report-card']");
    expect(card).not.toBeNull();
    expect(crab("Vale").getAttribute("aria-controls")).toBe(card?.id);
    expect(card?.textContent).toContain("Vale");
    expect(card?.textContent).toContain("No recent observation");
    expect(card?.textContent).toContain("unknown");
    expect(card?.textContent).toContain("Nothing to open for this crab.");
  });

  it("deep links to the authoritative target from the card", async () => {
    const onOpenTarget = await mount(makeController());
    await act(async () => crab("ExecAss").click());
    const link = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent === "Open the run history",
    );
    expect(link).toBeDefined();
    await act(async () => link!.click());
    expect(onOpenTarget).toHaveBeenCalledWith({ kind: "run", id: "run-9" });
  });

  it("says so honestly when the deep-link room is switched off", async () => {
    const onOpenTarget = vi.fn().mockReturnValue(false);
    await mount(makeController(), onOpenTarget);
    await act(async () => crab("ExecAss").click());
    const link = Array.from(container.querySelectorAll("button")).find(
      (b) => b.textContent === "Open the run history",
    );
    await act(async () => link!.click());
    const card = container.querySelector("[data-testid='reef-report-card']");
    expect(card?.textContent).toContain("switched off in Config");
  });

  it("closes the card on Escape and returns focus to the crab", async () => {
    await mount(makeController());
    const owner = crab("ExecAss");
    await act(async () => owner.click());
    const card = container.querySelector("[data-testid='reef-report-card']")!;
    expect(document.activeElement).toBe(card);
    await act(async () => {
      document.activeElement?.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(
      container.querySelector("[data-testid='reef-report-card']"),
    ).toBeNull();
    expect(document.activeElement).toBe(owner);
  });
});

describe("Chatter ergonomics", () => {
  it("sorts rooms by activity and keeps unreads quiet", async () => {
    await mount(makeController());
    const roomButtons = Array.from(
      container.querySelectorAll(".mc-chatter-rooms button"),
    );
    expect(roomButtons[0]?.textContent).toContain("launch");
    expect(roomButtons[1]?.textContent).toContain("archive sweep");
    const dot = roomButtons[1]?.querySelector("[data-testid='chatter-unread']");
    expect(dot).not.toBeNull();
    expect(dot?.getAttribute("aria-label")).toBe("unread notes");
    expect(roomButtons[1]?.textContent).not.toContain("3");
  });

  it("groups consecutive messages under one author header", async () => {
    await mount(makeController());
    const authors = Array.from(
      container.querySelectorAll(".mc-chatter-messages strong"),
    ).map((el) => el.textContent);
    expect(authors).toEqual(["ExecAss", "You"]);
    expect(container.textContent).toContain("Venue shortlist drafted.");
  });

  it("keeps newer owner text when an earlier send finishes", async () => {
    let finish!: (sent: boolean) => void;
    const pending = new Promise<boolean>((resolve) => {
      finish = resolve;
    });
    const sendMessage = vi.fn().mockReturnValue(pending);
    await mount(makeController({ sendMessage }));
    const input = container.querySelector(
      "input[aria-label='Add a safe owner note']",
    ) as HTMLInputElement;
    const form = input.closest("form")!;

    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )?.set;
      setter?.call(input, "first note");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    });
    expect(sendMessage).toHaveBeenCalledWith("room-new", "first note");

    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )?.set;
      setter?.call(input, "newer unsent note");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      finish(true);
      await pending;
    });
    expect(input.value).toBe("newer unsent note");
  });
});
