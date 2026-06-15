// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AssistantDeskResponse,
  AssistantDeskTranscriptResponse,
  AssistantDeskWorkItem,
} from "../../types";
import {
  AssistantDeskPanel,
  AssistantDeskStatusStrip,
} from "./AssistantDeskPanel";
import type { useAssistantDeskController } from "./useAssistantDeskController";

function makeItem(
  id: string,
  status: AssistantDeskWorkItem["status"],
  title: string
): AssistantDeskWorkItem {
  return {
    id,
    kind: status === "needs_you" ? "approval" : "execass",
    title,
    task_label: "Checking work",
    owner_label: "ExecAss",
    status,
    current_action: status === "needs_you" ? "Needs your review" : "Working",
    last_event_at: "2026-05-17T00:00:00Z",
    transcript_id: `transcript:${id}`,
    artifact_count: 0,
    changed_file_count: 0,
    source_refs: [],
    details: {
      provider_label: "LM Studio",
      model_label: "local",
      workspace_label: "carsinos",
      source_health: "fresh",
      last_error: null,
    },
    can_open_transcript: true,
    transcript_unavailable_reason: null,
  };
}

function makeDesk(): AssistantDeskResponse {
  return {
    generated_at: "2026-05-17T00:00:00Z",
    stale: false,
    buckets: {
      needs_you: [makeItem("approval:1", "needs_you", "Review command")],
      working: [
        makeItem("run:1", "working", "Edit files"),
        makeItem("run:2", "waiting", "Waiting on model"),
        makeItem("run:3", "working", "Run tests"),
        makeItem("run:4", "working", "Collect notes"),
      ],
      done_recently: [makeItem("run:5", "done", "Finished cleanup")],
    },
    summary: {
      needs_you_count: 1,
      working_count: 4,
      done_recently_count: 1,
      stale_count: 0,
    },
  };
}

function makeTranscript(workItemId: string): AssistantDeskTranscriptResponse {
  return {
    work_item_id: workItemId,
    transcript_id: `transcript:${workItemId}`,
    title: "Review command",
    complete: true,
    next_cursor: null,
    events: [
      {
        id: "event-1",
        at: "2026-05-17T00:00:00Z",
        role: "assistant",
        source: "assistant",
        title: "Note",
        text: "Ready",
        body_markdown: "Ready for **review**.",
        artifact_refs: [],
      },
    ],
    artifacts: [],
  };
}

type Controller = ReturnType<typeof useAssistantDeskController>;

function makeController(overrides: Partial<Controller> = {}): Controller {
  const desk = overrides.desk ?? makeDesk();
  const allItems = [
    ...(desk?.buckets.needs_you ?? []),
    ...(desk?.buckets.working ?? []),
    ...(desk?.buckets.done_recently ?? []),
  ];
  const visibleStatusItems = allItems.slice(0, 4);
  return {
    desk,
    loading: false,
    error: null,
    stale: false,
    allItems,
    visibleStatusItems,
    overflowStatusCount: Math.max(0, allItems.length - visibleStatusItems.length),
    selectedWorkItemId: null,
    selectedWorkItem: null,
    transcript: null,
    transcriptLoading: false,
    transcriptError: null,
    pollIntervalMs: 12_000,
    refresh: vi.fn(),
    selectWorkItem: vi.fn(),
    openTranscript: vi.fn(),
    closeTranscript: vi.fn(),
    ...overrides,
  } as Controller;
}

describe("AssistantDeskPanel", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    // @ts-expect-error test-only global
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(async () => {
    await act(async () => {
      root.unmount();
    });
    document.body.innerHTML = "";
  });

  it("renders max four status items with Needs you first and overflow count", async () => {
    await act(async () => {
      root.render(
        <AssistantDeskStatusStrip controller={makeController()} onOpenDesk={vi.fn()} />
      );
    });

    const chips = container.querySelectorAll(".mc-assistant-desk-chip");
    expect(chips.length).toBe(5);
    expect(chips[0]?.textContent).toContain("Needs you");
    expect(chips[0]?.textContent).toContain("Review command");
    expect(container.textContent).toContain("+2 more");
  });

  it("renders a nonblank three-bucket empty shell", async () => {
    const emptyDesk: AssistantDeskResponse = {
      generated_at: "2026-05-17T00:00:00Z",
      stale: false,
      buckets: {
        needs_you: [],
        working: [],
        done_recently: [],
      },
      summary: {
        needs_you_count: 0,
        working_count: 0,
        done_recently_count: 0,
        stale_count: 0,
      },
    };

    await act(async () => {
      root.render(
        <AssistantDeskPanel
          open
          controller={makeController({ desk: emptyDesk, allItems: [], visibleStatusItems: [] })}
          onClose={vi.fn()}
        />
      );
    });

    expect(container.textContent).toContain("Nothing needs your attention right now.");
    expect(container.textContent).toContain("Needs you");
    expect(container.textContent).toContain("Working");
    expect(container.textContent).toContain("Done recently");
    expect(container.querySelectorAll(".mc-assistant-desk-bucket").length).toBe(3);
  });

  it("keeps normal cards free of advanced internal terms", async () => {
    await act(async () => {
      root.render(
        <AssistantDeskPanel open controller={makeController()} onClose={vi.fn()} />
      );
    });

    const cardText = Array.from(container.querySelectorAll(".mc-assistant-desk-card"))
      .map((node) => node.textContent ?? "")
      .join(" ");
    for (const forbidden of [
      "runbook",
      "bridge",
      "PID",
      "canonical session",
      "source_refs",
      "routing config",
    ]) {
      expect(cardText).not.toContain(forbidden);
    }
  });

  it("does not render transcript events from a stale selected work item", async () => {
    const desk = makeDesk();
    const firstItem = desk.buckets.needs_you[0]!;
    const secondItem = desk.buckets.working[0]!;

    await act(async () => {
      root.render(
        <AssistantDeskPanel
          open
          controller={makeController({
            desk,
            selectedWorkItemId: secondItem.id,
            selectedWorkItem: secondItem,
            transcript: makeTranscript(firstItem.id),
          })}
          onClose={vi.fn()}
        />
      );
    });

    const transcriptPane = container.querySelector(".mc-assistant-desk-transcript-events");
    expect(transcriptPane?.textContent).not.toContain("Ready for");
    expect(transcriptPane?.textContent).toContain("No transcript events yet.");
  });

  it("returns focus to the transcript opener when the drawer closes", async () => {
    const originalRaf = window.requestAnimationFrame;
    window.requestAnimationFrame = ((callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    }) as typeof window.requestAnimationFrame;
    const desk = makeDesk();
    const firstItem = desk.buckets.needs_you[0]!;
    const closeTranscript = vi.fn();

    try {
      await act(async () => {
        root.render(
          <AssistantDeskPanel
            open
            controller={makeController({ desk, closeTranscript })}
            onClose={vi.fn()}
          />
        );
      });

      const transcriptButton = container.querySelector<HTMLButtonElement>(
        ".mc-assistant-desk-card .mc-assistant-desk-card-actions button"
      );
      expect(transcriptButton).not.toBeNull();

      await act(async () => {
        transcriptButton?.click();
      });

      await act(async () => {
        root.render(
          <AssistantDeskPanel
            open
            controller={makeController({
              desk,
              closeTranscript,
              selectedWorkItemId: firstItem.id,
              selectedWorkItem: firstItem,
              transcript: makeTranscript(firstItem.id),
            })}
            onClose={vi.fn()}
          />
        );
      });

      const closeButton = container.querySelector<HTMLButtonElement>(
        ".mc-assistant-desk-transcript header button"
      );
      expect(closeButton).not.toBeNull();

      await act(async () => {
        closeButton?.click();
      });

      expect(closeTranscript).toHaveBeenCalledTimes(1);
      expect(document.activeElement).toBe(transcriptButton);
    } finally {
      window.requestAnimationFrame = originalRaf;
    }
  });
});
