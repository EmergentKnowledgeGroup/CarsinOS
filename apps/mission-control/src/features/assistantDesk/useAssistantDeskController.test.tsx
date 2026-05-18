// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getAssistantDesk, getAssistantDeskTranscript } from "../../lib/api";
import type {
  AssistantDeskResponse,
  AssistantDeskTranscriptResponse,
  RuntimeConnectionSettings,
} from "../../types";
import { useAssistantDeskController } from "./useAssistantDeskController";

vi.mock("../../lib/api", () => ({
  getAssistantDesk: vi.fn(),
  getAssistantDeskTranscript: vi.fn(),
}));

const settings: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

function item(id: string, status = "working") {
  return {
    id,
    kind: "execass" as const,
    title: id,
    task_label: "Checking work",
    owner_label: "ExecAss",
    status,
    current_action: "Working",
    last_event_at: "2026-05-17T00:00:00Z",
    transcript_id: `transcript:${id}`,
    artifact_count: 0,
    changed_file_count: 0,
    source_refs: [],
    details: {},
    can_open_transcript: true,
    transcript_unavailable_reason: null,
  };
}

function deskResponse(ids: string[]): AssistantDeskResponse {
  const working = ids.map((id) => item(id));
  return {
    generated_at: "2026-05-17T00:00:00Z",
    stale: false,
    buckets: {
      needs_you: [],
      working,
      done_recently: [],
    },
    summary: {
      needs_you_count: 0,
      working_count: working.length,
      done_recently_count: 0,
      stale_count: 0,
    },
  };
}

function transcriptResponse(workItemId: string): AssistantDeskTranscriptResponse {
  return {
    transcript_id: `transcript:${workItemId}`,
    work_item_id: workItemId,
    events: [
      {
        id: "evt-1",
        at: "2026-05-17T00:00:00Z",
        role: "system",
        source: "run",
        text: "Started.",
      },
    ],
    complete: true,
    next_cursor: null,
  };
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((innerResolve, innerReject) => {
    resolve = innerResolve;
    reject = innerReject;
  });
  return { promise, resolve, reject };
}

type Controller = ReturnType<typeof useAssistantDeskController>;

function Harness(props: {
  onReady: (controller: Controller) => void;
  enabled?: boolean;
  statusStripEnabled?: boolean;
  deskOpen?: boolean;
}) {
  const controller = useAssistantDeskController({
    settings,
    tokenConfigured: true,
    assistantDeskEnabled: props.enabled ?? true,
    assistantDeskStatusStripEnabled: props.statusStripEnabled ?? true,
    deskOpen: props.deskOpen ?? false,
  });
  useEffect(() => {
    props.onReady(controller);
  }, [controller, props]);
  return null;
}

describe("useAssistantDeskController", () => {
  let container: HTMLDivElement;
  let root: Root;
  let latest: Controller | null;

  const flush = async () => {
    await act(async () => {
      await Promise.resolve();
    });
  };

  beforeEach(() => {
    vi.useFakeTimers();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    latest = null;
    // @ts-expect-error test-only global
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    vi.mocked(getAssistantDesk).mockResolvedValue(deskResponse(["run:1"]));
    vi.mocked(getAssistantDeskTranscript).mockImplementation(async (_settings, workItemId) =>
      transcriptResponse(workItemId)
    );
  });

  afterEach(async () => {
    await act(async () => {
      root.unmount();
    });
    vi.useRealTimers();
    vi.clearAllMocks();
    document.body.innerHTML = "";
  });

  it("does not poll when both Desk flags are disabled", async () => {
    await act(async () => {
      root.render(
        <Harness
          enabled={false}
          statusStripEnabled={false}
          onReady={(controller) => {
            latest = controller;
          }}
        />
      );
    });
    await flush();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });

    expect(latest?.desk).toBeNull();
    expect(getAssistantDesk).not.toHaveBeenCalled();
  });

  it("uses a slower status-strip cadence and a faster open-Desk cadence", async () => {
    await act(async () => {
      root.render(
        <Harness
          deskOpen={false}
          onReady={(controller) => {
            latest = controller;
          }}
        />
      );
    });
    await flush();
    expect(getAssistantDesk).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(11_900);
    });
    expect(getAssistantDesk).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(200);
    });
    expect(getAssistantDesk).toHaveBeenCalledTimes(2);

    vi.mocked(getAssistantDesk).mockClear();
    await act(async () => {
      root.render(
        <Harness
          deskOpen
          onReady={(controller) => {
            latest = controller;
          }}
        />
      );
    });
    await flush();
    expect(getAssistantDesk).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3_000);
    });
    expect(getAssistantDesk).toHaveBeenCalledTimes(2);
  });

  it("keeps the last Desk data visible when a later refresh fails", async () => {
    vi.mocked(getAssistantDesk)
      .mockResolvedValueOnce(deskResponse(["run:1"]))
      .mockRejectedValueOnce(new Error("gateway down"));

    await act(async () => {
      root.render(
        <Harness
          deskOpen
          onReady={(controller) => {
            latest = controller;
          }}
        />
      );
    });
    await flush();
    expect(latest?.desk?.buckets.working[0]?.id).toBe("run:1");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(3_000);
    });
    await flush();

    expect(latest?.stale).toBe(true);
    expect(latest?.error).toContain("gateway down");
    expect(latest?.desk?.buckets.working[0]?.id).toBe("run:1");
  });

  it("keeps selected work stable across refreshed data", async () => {
    vi.mocked(getAssistantDesk)
      .mockResolvedValueOnce(deskResponse(["run:1"]))
      .mockResolvedValueOnce(deskResponse(["run:1", "run:2"]));

    await act(async () => {
      root.render(
        <Harness
          deskOpen
          onReady={(controller) => {
            latest = controller;
          }}
        />
      );
    });
    await flush();

    act(() => {
      latest?.selectWorkItem("run:1");
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3_000);
    });
    await flush();

    expect(latest?.selectedWorkItemId).toBe("run:1");
    expect(latest?.desk?.buckets.working.map((workItem) => workItem.id)).toEqual([
      "run:1",
      "run:2",
    ]);
  });

  it("clears stale transcripts and ignores slower older transcript responses", async () => {
    const oldTranscript = deferred<AssistantDeskTranscriptResponse>();
    const slowTranscript = deferred<AssistantDeskTranscriptResponse>();
    const fastTranscript = deferred<AssistantDeskTranscriptResponse>();
    vi.mocked(getAssistantDeskTranscript)
      .mockReturnValueOnce(oldTranscript.promise)
      .mockReturnValueOnce(slowTranscript.promise)
      .mockReturnValueOnce(fastTranscript.promise);

    await act(async () => {
      root.render(
        <Harness
          deskOpen
          onReady={(controller) => {
            latest = controller;
          }}
        />
      );
    });
    await flush();

    let oldCall: Promise<void> | undefined;
    await act(async () => {
      oldCall = latest?.openTranscript("run:old");
      await Promise.resolve();
    });
    await act(async () => {
      oldTranscript.resolve(transcriptResponse("run:old"));
      await oldCall;
    });
    expect(latest?.transcript?.work_item_id).toBe("run:old");

    let slowCall: Promise<void> | undefined;
    await act(async () => {
      slowCall = latest?.openTranscript("run:slow");
      await Promise.resolve();
    });
    expect(latest?.selectedWorkItemId).toBe("run:slow");
    expect(latest?.transcript).toBeNull();
    expect(latest?.transcriptLoading).toBe(true);

    let fastCall: Promise<void> | undefined;
    await act(async () => {
      fastCall = latest?.openTranscript("run:fast");
      await Promise.resolve();
    });
    await act(async () => {
      fastTranscript.resolve(transcriptResponse("run:fast"));
      await fastCall;
    });
    await act(async () => {
      slowTranscript.resolve(transcriptResponse("run:slow"));
      await slowCall;
    });

    expect(latest?.selectedWorkItemId).toBe("run:fast");
    expect(latest?.transcript?.work_item_id).toBe("run:fast");
    expect(latest?.transcriptLoading).toBe(false);
  });
});
