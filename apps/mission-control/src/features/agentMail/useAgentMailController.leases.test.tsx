// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../lib/api", () => ({
  ackAgentMailMessage: vi.fn(),
  createAgentMailFileLease: vi.fn(),
  createAgentMailThread: vi.fn(),
  createMemoryNote: vi.fn(),
  fetchAgentMailAttachmentBlob: vi.fn(),
  getAgentMailThread: vi.fn(),
  listAgentMailFileLeases: vi.fn(),
  listAgentMailMessages: vi.fn(),
  listAgentMailThreads: vi.fn(),
  releaseAgentMailFileLease: vi.fn(),
  sendAgentMailMessage: vi.fn(),
  uploadAgentMailAttachment: vi.fn(),
}));

import {
  createAgentMailFileLease,
  getAgentMailThread,
  listAgentMailFileLeases,
  listAgentMailMessages,
  listAgentMailThreads,
  releaseAgentMailFileLease,
} from "../../lib/api";
import type {
  AgentMailFileLeaseResponse,
  AgentMailThreadSummaryResponse,
  RuntimeConnectionSettings,
} from "../../types";
import type { Notice } from "../../app/useAppController";
import { useAgentMailController } from "./useAgentMailController";

const listLeasesMock = vi.mocked(listAgentMailFileLeases);
const listThreadsMock = vi.mocked(listAgentMailThreads);
const createLeaseMock = vi.mocked(createAgentMailFileLease);
const releaseLeaseMock = vi.mocked(releaseAgentMailFileLease);

const SETTINGS: RuntimeConnectionSettings = {
  gateway_url: "http://127.0.0.1:18789",
};

function leaseFixture(index: number): AgentMailFileLeaseResponse {
  return {
    lease_id: `lease-${index}`,
    holder_principal: "agent-root",
    glob_pattern: `src/slice-${index}/**`,
    exclusive: index % 2 === 0,
    ttl_ms: 900_000,
    note: null,
    created_at: 1_753_000_000_000,
    expires_at: 1_753_000_900_000,
    released_at: null,
  };
}

function threadFixture(index: number): AgentMailThreadSummaryResponse {
  return {
    thread_id: `thread-${index}`,
    kind: "direct",
    subject: `Subject ${index}`,
    created_by_principal: "agent-root",
    participant_count: 2,
    message_count: 0,
    latest_message_at: 1_753_000_000_000 + index,
    latest_message_preview: null,
    latest_sender_principal: null,
    unread_count: 0,
    created_at: 1_753_000_000_000,
    updated_at: 1_753_000_000_000 + index,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

type Controller = ReturnType<typeof useAgentMailController>;

let controller: Controller;
let notices: Notice[];

function pushNotice(notice: Notice | null) {
  if (notice) notices.push(notice);
}

interface HarnessProps {
  settings: RuntimeConnectionSettings;
  tokenConfigured: boolean;
}

function Harness(props: HarnessProps) {
  const current = useAgentMailController({
    settings: props.settings,
    tokenConfigured: props.tokenConfigured,
    setNotice: pushNotice,
  });
  useEffect(() => {
    controller = current;
  });
  return null;
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  notices = [];
  listThreadsMock.mockResolvedValue({ items: [] });
  listLeasesMock.mockResolvedValue([]);
  vi.mocked(getAgentMailThread).mockResolvedValue({
    thread: threadFixture(1),
    participants: [],
  });
  vi.mocked(listAgentMailMessages).mockResolvedValue({ items: [] });
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
  vi.useRealTimers();
});

async function mount(props: HarnessProps) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(<Harness {...props} />);
  });
}

/** Flush the controller's debounced Agent Mail refresh timer. */
async function flushRefreshTimer() {
  await act(async () => {
    vi.advanceTimersByTime(300);
  });
}

describe("useAgentMailController mounted lease read model", () => {
  it("does not fetch leases and reports not-loaded before authentication is configured", async () => {
    await mount({ settings: { gateway_url: "" }, tokenConfigured: false });
    await flushRefreshTimer();
    expect(listLeasesMock).not.toHaveBeenCalled();
    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);
    expect(controller.leasesLoading).toBe(false);
    expect(controller.leasesError).toBeNull();

    // A configured URL alone is not configured authentication.
    await mount({ settings: SETTINGS, tokenConfigured: false });
    await flushRefreshTimer();
    expect(listLeasesMock).not.toHaveBeenCalled();
  });

  it("loads leases on first authenticated refresh with the exact default filter identity", async () => {
    listLeasesMock.mockResolvedValue([leaseFixture(1), leaseFixture(2)]);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();

    expect(listLeasesMock).toHaveBeenCalledTimes(1);
    expect(listLeasesMock).toHaveBeenCalledWith(SETTINGS, {
      holderPrincipal: undefined,
      includeReleased: false,
    });
    expect(controller.leases.map((lease) => lease.lease_id)).toEqual([
      "lease-1",
      "lease-2",
    ]);
    expect(controller.leasesLoaded).toBe(true);
    expect(controller.leasesLoading).toBe(false);
    expect(controller.leasesError).toBeNull();
  });

  it("reports loading while the lease request is in flight", async () => {
    const gate = deferred<AgentMailFileLeaseResponse[]>();
    listLeasesMock.mockReturnValue(gate.promise);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();

    expect(controller.leasesLoading).toBe(true);
    expect(controller.leasesLoaded).toBe(false);

    await act(async () => {
      gate.resolve([leaseFixture(1)]);
      await gate.promise;
    });
    expect(controller.leasesLoading).toBe(false);
    expect(controller.leasesLoaded).toBe(true);
  });

  it("distinguishes loaded-empty from never-loaded", async () => {
    listLeasesMock.mockResolvedValue([]);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    expect(controller.leasesLoaded).toBe(false);
    await flushRefreshTimer();

    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(true);
    expect(controller.leasesError).toBeNull();
  });

  it("keeps Direct Mail and Agent-room reads live when only the lease read fails", async () => {
    listThreadsMock.mockResolvedValue({ items: [threadFixture(1)] });
    listLeasesMock.mockRejectedValue(new Error("lease backend down"));
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();

    expect(controller.mailThreads.map((thread) => thread.thread_id)).toEqual([
      "thread-1",
    ]);
    expect(controller.roomThreads.map((thread) => thread.thread_id)).toEqual([
      "thread-1",
    ]);
    expect(controller.leasesError).toContain("lease backend down");
    expect(controller.leasesLoaded).toBe(false);
    expect(controller.leases).toEqual([]);
    // The combined refresh must not report the whole Agent Mail read as
    // failed when the thread reads committed successfully.
    expect(
      notices.filter((notice) => notice.tone === "error"),
    ).toHaveLength(0);
  });

  it("keeps committed lease facts when only the thread read fails", async () => {
    listThreadsMock.mockRejectedValue(new Error("threads down"));
    listLeasesMock.mockResolvedValue([leaseFixture(3)]);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();

    expect(controller.leases.map((lease) => lease.lease_id)).toEqual([
      "lease-3",
    ]);
    expect(controller.leasesLoaded).toBe(true);
    expect(controller.leasesError).toBeNull();
    // The thread failure still surfaces through the existing notice path.
    expect(
      notices.some(
        (notice) =>
          notice.tone === "error" && notice.message.includes("threads down"),
      ),
    ).toBe(true);
  });

  it("recovers with refreshLeases after a lease failure without waiting for the debounce", async () => {
    listLeasesMock.mockRejectedValueOnce(new Error("first attempt failed"));
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(controller.leasesError).toContain("first attempt failed");

    listLeasesMock.mockResolvedValueOnce([leaseFixture(4)]);
    await act(async () => {
      await controller.refreshLeases();
    });
    expect(controller.leasesError).toBeNull();
    expect(controller.leasesLoaded).toBe(true);
    expect(controller.leases.map((lease) => lease.lease_id)).toEqual([
      "lease-4",
    ]);
  });

  it("does not render last-known lease rows as live after a same-identity refresh fails", async () => {
    listLeasesMock.mockResolvedValueOnce([leaseFixture(4)]);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(controller.leases).toHaveLength(1);
    expect(controller.leasesLoaded).toBe(true);

    listLeasesMock.mockRejectedValueOnce(new Error("refresh failed"));
    await act(async () => {
      await controller.refreshLeases();
    });

    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);
    expect(controller.leasesError).toContain("refresh failed");
  });

  it("passes the trimmed acting-as principal as the lease holder filter", async () => {
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(listLeasesMock).toHaveBeenLastCalledWith(SETTINGS, {
      holderPrincipal: undefined,
      includeReleased: false,
    });

    await act(async () => {
      controller.setMailPrincipalOverride("  agent-root  ");
    });
    await flushRefreshTimer();
    expect(listLeasesMock).toHaveBeenLastCalledWith(SETTINGS, {
      holderPrincipal: "agent-root",
      includeReleased: false,
    });
  });

  it("hides committed lease facts immediately when the acting-as principal changes", async () => {
    listLeasesMock.mockResolvedValueOnce([leaseFixture(1)]);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(controller.leases).toHaveLength(1);
    expect(controller.leasesLoaded).toBe(true);

    await act(async () => {
      controller.setMailPrincipalOverride("agent-root");
    });

    // The replacement read is still inside the debounce window. Old-principal
    // facts must already be unavailable rather than presented as current.
    expect(listLeasesMock).toHaveBeenCalledTimes(1);
    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);
    expect(controller.leasesLoading).toBe(false);
    expect(controller.leasesError).toBeNull();
  });

  it("invalidates an old-principal response before the replacement debounce starts", async () => {
    const oldIdentity = deferred<AgentMailFileLeaseResponse[]>();
    listLeasesMock
      .mockReturnValueOnce(oldIdentity.promise)
      .mockRejectedValueOnce(new Error("new principal unavailable"));
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();

    await act(async () => {
      controller.setMailPrincipalOverride("agent-root");
    });
    expect(listLeasesMock).toHaveBeenCalledTimes(1);

    // Resolve the old request inside the 280 ms identity-transition window.
    await act(async () => {
      oldIdentity.resolve([leaseFixture(1)]);
      await oldIdentity.promise;
    });
    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);

    await flushRefreshTimer();
    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);
    expect(controller.leasesError).toContain("new principal unavailable");
  });

  it("hides committed lease facts immediately when the gateway changes", async () => {
    listLeasesMock.mockResolvedValueOnce([leaseFixture(1)]);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(controller.leases).toHaveLength(1);

    await mount({
      settings: { gateway_url: "http://10.0.0.9:18789" },
      tokenConfigured: true,
    });

    expect(listLeasesMock).toHaveBeenCalledTimes(1);
    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);
    expect(controller.leasesError).toBeNull();
  });

  it("refuses a stale lease response after the acting-as principal changed", async () => {
    const first = deferred<AgentMailFileLeaseResponse[]>();
    const second = deferred<AgentMailFileLeaseResponse[]>();
    listLeasesMock
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();

    await act(async () => {
      controller.setMailPrincipalOverride("agent-root");
    });
    await flushRefreshTimer();
    expect(listLeasesMock).toHaveBeenCalledTimes(2);

    await act(async () => {
      second.resolve([leaseFixture(2)]);
      await second.promise;
    });
    expect(controller.leases.map((lease) => lease.lease_id)).toEqual([
      "lease-2",
    ]);

    // The pre-change response arrives late; the newest identity must win.
    await act(async () => {
      first.resolve([leaseFixture(1)]);
      await first.promise;
    });
    expect(controller.leases.map((lease) => lease.lease_id)).toEqual([
      "lease-2",
    ]);
    expect(controller.leasesError).toBeNull();
    expect(controller.leasesLoading).toBe(false);
  });

  it("refuses a stale lease response after the gateway settings changed", async () => {
    const first = deferred<AgentMailFileLeaseResponse[]>();
    const second = deferred<AgentMailFileLeaseResponse[]>();
    listLeasesMock
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();

    const nextSettings: RuntimeConnectionSettings = {
      gateway_url: "http://10.0.0.9:18789",
    };
    await mount({ settings: nextSettings, tokenConfigured: true });
    await flushRefreshTimer();
    expect(listLeasesMock).toHaveBeenLastCalledWith(nextSettings, {
      holderPrincipal: undefined,
      includeReleased: false,
    });

    await act(async () => {
      second.resolve([leaseFixture(9)]);
      await second.promise;
    });
    await act(async () => {
      first.resolve([leaseFixture(1)]);
      await first.promise;
    });
    expect(controller.leases.map((lease) => lease.lease_id)).toEqual([
      "lease-9",
    ]);
  });

  it("clears stale lease facts immediately on auth loss and ignores the in-flight response", async () => {
    const gate = deferred<AgentMailFileLeaseResponse[]>();
    listLeasesMock.mockResolvedValueOnce([leaseFixture(1)]);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(controller.leases).toHaveLength(1);
    expect(controller.leasesLoaded).toBe(true);

    // Queue a second in-flight read, then drop authentication.
    listLeasesMock.mockReturnValueOnce(gate.promise);
    await act(async () => {
      controller.queueAgentMailRefresh(SETTINGS);
    });
    await flushRefreshTimer();
    await mount({ settings: SETTINGS, tokenConfigured: false });

    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);
    expect(controller.leasesError).toBeNull();

    await act(async () => {
      gate.resolve([leaseFixture(7)]);
      await gate.promise;
    });
    expect(controller.leases).toEqual([]);
    expect(controller.leasesLoaded).toBe(false);
  });

  it("drives a real new lease read on queueAgentMailRefresh", async () => {
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(listLeasesMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      controller.queueAgentMailRefresh(SETTINGS);
    });
    await flushRefreshTimer();
    expect(listLeasesMock).toHaveBeenCalledTimes(2);
  });
});

describe("useAgentMailController mounted lease mutations", () => {
  it("rejects a non-integer, zero, or negative TTL without posting", async () => {
    await mount({ settings: SETTINGS, tokenConfigured: true });
    for (const ttl of ["abc", "0", "-5", "1.5", ""]) {
      notices = [];
      await act(async () => {
        controller.setLeaseTtlMs(ttl);
      });
      let result: boolean | undefined;
      await act(async () => {
        result = await controller.createFileLease();
      });
      expect(result, `ttl ${JSON.stringify(ttl)} must be refused`).toBe(false);
      expect(
        notices.some(
          (notice) =>
            notice.tone === "error" && notice.message.includes("TTL"),
        ),
      ).toBe(true);
    }
    expect(createLeaseMock).not.toHaveBeenCalled();
  });

  it("rejects an empty glob pattern without posting", async () => {
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await act(async () => {
      controller.setLeaseGlobPattern("   ");
    });
    let result: boolean | undefined;
    await act(async () => {
      result = await controller.createFileLease();
    });
    expect(result).toBe(false);
    expect(createLeaseMock).not.toHaveBeenCalled();
    expect(
      notices.some(
        (notice) =>
          notice.tone === "error" && notice.message.includes("glob"),
      ),
    ).toBe(true);
  });

  it("posts the exact trimmed reserve body and clears only the note on success", async () => {
    createLeaseMock.mockResolvedValue({ lease: leaseFixture(11) });
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await act(async () => {
      controller.setLeaseHolderPrincipal("  agent-root  ");
      controller.setLeaseGlobPattern("  docs/**/*  ");
      controller.setLeaseTtlMs("3600000");
      controller.setLeaseExclusive(true);
      controller.setLeaseNote("  hands off  ");
    });

    let result: boolean | undefined;
    await act(async () => {
      result = await controller.createFileLease();
    });
    expect(result).toBe(true);
    expect(createLeaseMock).toHaveBeenCalledTimes(1);
    expect(createLeaseMock).toHaveBeenCalledWith(SETTINGS, {
      holder_principal: "agent-root",
      glob_pattern: "docs/**/*",
      exclusive: true,
      ttl_ms: 3_600_000,
      note: "hands off",
    });
    expect(controller.leaseNote).toBe("");
    expect(controller.leaseGlobPattern).toBe("  docs/**/*  ");
  });

  it("omits blank holder and note from the reserve body", async () => {
    createLeaseMock.mockResolvedValue({ lease: leaseFixture(12) });
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await act(async () => {
      controller.setLeaseHolderPrincipal("   ");
      controller.setLeaseNote("");
    });
    await act(async () => {
      await controller.createFileLease();
    });
    expect(createLeaseMock).toHaveBeenCalledWith(SETTINGS, {
      holder_principal: undefined,
      glob_pattern: "**/*",
      exclusive: false,
      ttl_ms: 900_000,
      note: undefined,
    });
  });

  it("reports a failed reserve truthfully and preserves the owner's note", async () => {
    createLeaseMock.mockRejectedValue(new Error("boom"));
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await act(async () => {
      controller.setLeaseNote("keep me");
    });
    let result: boolean | undefined;
    await act(async () => {
      result = await controller.createFileLease();
    });
    expect(result).toBe(false);
    expect(controller.leaseNote).toBe("keep me");
    expect(
      notices.some(
        (notice) =>
          notice.tone === "error" &&
          notice.message.includes("Lease create failed"),
      ),
    ).toBe(true);
  });

  it("locks same-tick duplicate reserves inside the controller, not the UI", async () => {
    const gate = deferred<{ lease: AgentMailFileLeaseResponse }>();
    createLeaseMock.mockReturnValue(gate.promise);
    await mount({ settings: SETTINGS, tokenConfigured: true });

    let first: Promise<boolean>;
    let second: Promise<boolean>;
    await act(async () => {
      first = controller.createFileLease();
      second = controller.createFileLease();
      gate.resolve({ lease: leaseFixture(13) });
      await Promise.all([first, second]);
    });
    expect(createLeaseMock).toHaveBeenCalledTimes(1);
    await act(async () => {
      expect(await first!).toBe(true);
      expect(await second!).toBe(false);
    });

    // The lock releases after settlement so the next deliberate reserve works.
    createLeaseMock.mockResolvedValue({ lease: leaseFixture(14) });
    await act(async () => {
      expect(await controller.createFileLease()).toBe(true);
    });
    expect(createLeaseMock).toHaveBeenCalledTimes(2);
  });

  it("releases the exact lease with the trimmed acting holder", async () => {
    releaseLeaseMock.mockResolvedValue({
      lease: { ...leaseFixture(2), released_at: 1_753_000_950_000 },
    });
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await act(async () => {
      controller.setLeaseHolderPrincipal("  agent-root  ");
    });
    let result: boolean | undefined;
    await act(async () => {
      result = await controller.releaseFileLease("lease-2");
    });
    expect(result).toBe(true);
    expect(releaseLeaseMock).toHaveBeenCalledTimes(1);
    expect(releaseLeaseMock).toHaveBeenCalledWith(
      SETTINGS,
      "lease-2",
      "agent-root",
    );
  });

  it("reports a failed release truthfully", async () => {
    releaseLeaseMock.mockRejectedValue(new Error("lease not found"));
    await mount({ settings: SETTINGS, tokenConfigured: true });
    let result: boolean | undefined;
    await act(async () => {
      result = await controller.releaseFileLease("lease-9");
    });
    expect(result).toBe(false);
    expect(
      notices.some(
        (notice) =>
          notice.tone === "error" &&
          notice.message.includes("Lease release failed"),
      ),
    ).toBe(true);
  });

  it("locks same-tick duplicate releases per lease while allowing distinct leases", async () => {
    const gateA = deferred<{ lease: AgentMailFileLeaseResponse }>();
    const gateB = deferred<{ lease: AgentMailFileLeaseResponse }>();
    releaseLeaseMock.mockImplementation((_settings, leaseId) =>
      leaseId === "lease-a" ? gateA.promise : gateB.promise,
    );
    await mount({ settings: SETTINGS, tokenConfigured: true });

    let duplicate: Promise<boolean>;
    await act(async () => {
      const firstA = controller.releaseFileLease("lease-a");
      duplicate = controller.releaseFileLease("lease-a");
      const firstB = controller.releaseFileLease("lease-b");
      gateA.resolve({
        lease: { ...leaseFixture(1), lease_id: "lease-a" },
      });
      gateB.resolve({
        lease: { ...leaseFixture(2), lease_id: "lease-b" },
      });
      await Promise.all([firstA, firstB]);
    });
    // Same lease: one POST. Distinct lease: its own POST.
    expect(releaseLeaseMock).toHaveBeenCalledTimes(2);
    expect(releaseLeaseMock).toHaveBeenCalledWith(
      SETTINGS,
      "lease-a",
      undefined,
    );
    expect(releaseLeaseMock).toHaveBeenCalledWith(
      SETTINGS,
      "lease-b",
      undefined,
    );
    await act(async () => {
      expect(await duplicate!).toBe(false);
    });

    // After settlement the same lease can be targeted again deliberately.
    releaseLeaseMock.mockResolvedValue({
      lease: { ...leaseFixture(1), lease_id: "lease-a" },
    });
    await act(async () => {
      expect(await controller.releaseFileLease("lease-a")).toBe(true);
    });
    expect(releaseLeaseMock).toHaveBeenCalledTimes(3);
  });

  it("locks same-tick duplicate room-workspace reserves inside the controller", async () => {
    listThreadsMock.mockResolvedValue({
      items: [{ ...threadFixture(1), kind: "room" }],
    });
    const gate = deferred<{ lease: AgentMailFileLeaseResponse }>();
    createLeaseMock.mockReturnValue(gate.promise);
    await mount({ settings: SETTINGS, tokenConfigured: true });
    await flushRefreshTimer();
    expect(controller.selectedRoomThreadId).toBe("thread-1");

    await act(async () => {
      const first = controller.reserveSelectedRoomWorkspace();
      const second = controller.reserveSelectedRoomWorkspace();
      gate.resolve({ lease: leaseFixture(21) });
      await Promise.all([first, second]);
    });
    expect(createLeaseMock).toHaveBeenCalledTimes(1);
    expect(createLeaseMock).toHaveBeenCalledWith(SETTINGS, {
      holder_principal: undefined,
      glob_pattern: "chatrooms/thread-1/**",
      exclusive: true,
      ttl_ms: 900_000,
      note: "room moderation reserve",
    });
  });
});
