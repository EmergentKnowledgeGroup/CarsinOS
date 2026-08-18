// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  Agent,
  AgentMailFileLeaseResponse,
  AgentMailMessageResponse,
  AgentMailThreadDetailResponse,
  AgentMailThreadSummaryResponse,
  RuntimeRoutingConfigResponse,
} from "../../types";
import type { PeopleRoutingController } from "../peopleRouting/usePeopleRoutingController";
import { MailPage } from "./MailPage";

const AGENTS: Agent[] = [
  {
    agent_id: "agent-root",
    name: "Root",
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "standard",
    role_label: "Operations Director",
    reports_to_agent_id: null,
    memory_binding: null,
  },
  {
    agent_id: "default",
    name: "Local Assistant",
    model_provider: "ollama",
    model_id: "qwen3.5-9b-instruct",
    workspace_root: ".",
    tool_profile: "default",
    role_label: "Reliability Lead",
    reports_to_agent_id: "agent-root",
    memory_binding: null,
  },
];

function threadFixture(index: number): AgentMailThreadSummaryResponse {
  return {
    thread_id: `thread-${index}`,
    kind: "direct",
    subject: `Subject ${index}`,
    created_by_principal: "agent-root",
    participant_count: 2,
    message_count: 1,
    latest_message_at: 1_753_000_000_000 + index,
    latest_message_preview: `Preview ${index}`,
    latest_sender_principal: "agent-root",
    unread_count: 0,
    created_at: 1_753_000_000_000,
    updated_at: 1_753_000_000_000 + index,
  };
}

function threadDetailFixture(index: number): AgentMailThreadDetailResponse {
  return {
    thread: threadFixture(index),
    participants: [
      {
        principal_id: "agent-root",
        role: "owner",
        joined_at: 1_753_000_000_000,
        last_read_at: null,
        muted: false,
      },
    ],
  };
}

function messageFixture(index: number): AgentMailMessageResponse {
  return {
    message_id: `msg-${index}`,
    thread_id: "thread-1",
    sender_principal: "agent-root",
    sender_kind: "agent",
    body_text: `Body ${index}`,
    metadata_json: null,
    created_at: 1_753_000_000_000 + index,
    recipients: [
      {
        recipient_principal: "default",
        delivered_at: 1_753_000_000_000,
        acked_at: 1_753_000_000_500,
      },
      {
        recipient_principal: "agent-root",
        delivered_at: 1_753_000_000_000,
        acked_at: null,
      },
    ],
    attachments: [],
  };
}

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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function routingFixture(): RuntimeRoutingConfigResponse {
  return {
    enabled: true,
    use_channel_defaults_as_fallback: false,
    local_operator_human_identity_id: "local-operator",
    dm_unmapped_policy: "approval_required",
    shared_unmapped_policy: "block",
    human_identities: [
      { human_identity_id: "local-operator", display_name: "You", enabled: true },
    ],
    platform_identity_links: [],
    assistant_assignments: [
      {
        human_identity_id: "local-operator",
        assistant_agent_id: "default",
        enabled: true,
      },
    ],
    lane_memory_policies: [],
  };
}

function stubPeopleRouting(
  overrides?: Partial<PeopleRoutingController>,
): PeopleRoutingController {
  const draft = routingFixture();
  const controller: PeopleRoutingController = {
    gatewayConfigured: true,
    routingConfig: routingFixture(),
    routingDraft: draft,
    routingLoading: false,
    routingSaving: false,
    routingError: null,
    routingNotice: null,
    routingDirty: false,
    humanRoutingCards: [
      {
        index: 0,
        human: draft.human_identities[0]!,
        assignment: draft.assistant_assignments[0]!,
        memoryPolicy: null,
        links: [],
      },
    ],
    routingSummary: {
      humans: 1,
      linkedAccounts: 0,
      assignedHumans: 1,
      waitingForAssignment: 0,
      localOperator: "local-operator",
    },
    routedHumansByAgentId: new Map(),
    loadRoutingConfig: vi.fn().mockResolvedValue(draft),
    patchRoutingDraft: vi.fn(),
    addHumanIdentity: vi.fn(),
    updateHumanDisplayName: vi.fn(),
    updateHumanEnabled: vi.fn(),
    removeHumanIdentity: vi.fn(),
    setHumanAssignment: vi.fn(),
    addPlatformIdentityLink: vi.fn(),
    updatePlatformIdentityLink: vi.fn(),
    removePlatformIdentityLink: vi.fn(),
    resetRoutingDraft: vi.fn(),
    saveRoutingDraft: vi.fn().mockResolvedValue(undefined),
    restoreRoutingConfig: vi.fn().mockResolvedValue(draft),
    detachAgentRouting: vi.fn().mockResolvedValue(draft),
    ...overrides,
  };
  return controller;
}

type MailPageProps = Parameters<typeof MailPage>[0];

function defaultProps(): MailPageProps {
  return {
    onRefresh: vi.fn(),
    agents: AGENTS,
    activeRoomId: null,
    peopleRouting: stubPeopleRouting(),
    mailboxFilter: "all",
    onMailboxFilterChange: vi.fn(),
    mailPrincipalOverride: "",
    onMailPrincipalOverrideChange: vi.fn(),
    mailSearch: "",
    onMailSearchChange: vi.fn(),
    newMailThreadSubject: "",
    onNewMailThreadSubjectChange: vi.fn(),
    newMailThreadParticipants: "",
    onNewMailThreadParticipantsChange: vi.fn(),
    onCreateDirectThread: vi.fn().mockResolvedValue(true),
    mailThreads: [threadFixture(1)],
    selectedMailThreadId: "thread-1",
    onSelectMailThread: vi.fn(),
    mailThreadDetail: threadDetailFixture(1),
    mailMessages: [messageFixture(1)],
    onAcknowledgeMessage: vi.fn().mockResolvedValue(undefined),
    onDownloadAttachment: vi.fn().mockResolvedValue(undefined),
    mailComposeSender: "",
    onMailComposeSenderChange: vi.fn(),
    mailComposeRecipients: "",
    onMailComposeRecipientsChange: vi.fn(),
    mailComposeBody: "draft body kept for the owner",
    onMailComposeBodyChange: vi.fn(),
    mailAttachmentFiles: [],
    onMailAttachmentFilesChange: vi.fn(),
    onSendMessage: vi.fn().mockResolvedValue(undefined),
    onSummarizeToNote: vi.fn().mockResolvedValue(undefined),
    leaseHolderPrincipal: "",
    onLeaseHolderPrincipalChange: vi.fn(),
    leaseGlobPattern: "**/*",
    onLeaseGlobPatternChange: vi.fn(),
    leaseTtlMs: "900000",
    onLeaseTtlMsChange: vi.fn(),
    leaseNote: "",
    onLeaseNoteChange: vi.fn(),
    leaseExclusive: false,
    onLeaseExclusiveChange: vi.fn(),
    onCreateFileLease: vi.fn().mockResolvedValue(true),
    leases: [],
    leasesLoading: false,
    leasesLoaded: true,
    leasesError: null,
    onRetryLeases: vi.fn().mockResolvedValue(undefined),
    onReleaseFileLease: vi.fn().mockResolvedValue(true),
  };
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
  localStorage.clear();
});

async function render(props: MailPageProps) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(<MailPage {...props} />);
  });
}

function findButton(label: string, scope: ParentNode = container) {
  return Array.from(scope.querySelectorAll("button")).find((candidate) =>
    candidate.textContent?.includes(label),
  );
}

async function clickButton(label: string, scope: ParentNode = container) {
  const button = findButton(label, scope);
  expect(button, `missing button ${label}`).toBeTruthy();
  await act(async () => button!.click());
}

function subTabButton(label: string) {
  return Array.from(
    container.querySelectorAll<HTMLButtonElement>(".mc-sub-tabs button"),
  ).find((button) => button.textContent?.startsWith(label));
}

function paginationInfo(): string | null {
  return (
    container.querySelector(".mc-pagination-info")?.textContent?.trim() ?? null
  );
}

function paginationButton(label: "Prev" | "Next") {
  return Array.from(
    container.querySelectorAll<HTMLButtonElement>(".mc-pagination-btn"),
  ).find((button) => button.textContent?.trim() === label);
}

async function clickPagination(label: "Prev" | "Next") {
  const button = paginationButton(label);
  expect(button, `missing pagination button ${label}`).toBeTruthy();
  await act(async () => button!.click());
}

function labeledControl<T extends Element>(
  labelText: string,
  selector: string,
): T | undefined {
  const label = Array.from(container.querySelectorAll("label")).find(
    (candidate) => candidate.textContent?.trim().startsWith(labelText),
  );
  return label?.querySelector<T>(selector) ?? undefined;
}

async function setLabeledSelect(labelText: string, value: string) {
  const select = labeledControl<HTMLSelectElement>(labelText, "select");
  expect(select, `missing select ${labelText}`).toBeTruthy();
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(select),
    "value",
  )?.set;
  setter?.call(select, value);
  await act(async () => {
    select!.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

async function setLabeledInput(labelText: string, value: string) {
  const input = labeledControl<HTMLInputElement>(labelText, "input");
  expect(input, `missing input ${labelText}`).toBeTruthy();
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(input),
    "value",
  )?.set;
  setter?.call(input, value);
  await act(async () => {
    input!.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("MailPage direct-mail parity", () => {
  it("renders threads, messages, ack progress, and calls selection with the exact thread", async () => {
    const props = defaultProps();
    props.mailThreads = [threadFixture(1), threadFixture(2)];
    await render(props);

    expect(container.textContent).toContain("Subject 1");
    expect(container.textContent).toContain("Subject 2");
    expect(container.textContent).toContain("Body 1");
    expect(container.textContent).toContain("1/2 acknowledged");
    expect(container.textContent).toContain("1 message(s)");

    const grid = container.querySelector(".mc-mail-grid");
    expect(grid?.className).toContain("mc-mobile-list-open");

    const secondThread = Array.from(
      container.querySelectorAll<HTMLButtonElement>(".mc-mail-thread-item"),
    ).find((item) => item.textContent?.includes("Subject 2"));
    expect(secondThread).toBeTruthy();
    await act(async () => secondThread!.click());
    expect(props.onSelectMailThread).toHaveBeenCalledWith("thread-2");
    // Mobile: picking a thread moves from the list pane to the detail pane.
    expect(container.querySelector(".mc-mail-grid")?.className).toContain(
      "mc-mobile-detail-open",
    );
    await clickButton("Back to threads");
    expect(container.querySelector(".mc-mail-grid")?.className).toContain(
      "mc-mobile-list-open",
    );
  });

  it("keeps mailbox/acting-as/search filters wired and clears them together", async () => {
    const props = defaultProps();
    props.mailboxFilter = "inbox";
    props.mailSearch = "needle";
    props.mailThreads = [];
    await render(props);

    expect(container.textContent).toContain(
      "No direct threads match your current filters.",
    );
    await setLabeledSelect("Mailbox", "outbox");
    expect(props.onMailboxFilterChange).toHaveBeenCalledWith("outbox");
    await setLabeledInput("Search", "handoff");
    expect(props.onMailSearchChange).toHaveBeenCalledWith("handoff");
    await setLabeledSelect("Acting as", "agent-root");
    expect(props.onMailPrincipalOverrideChange).toHaveBeenCalledWith(
      "agent-root",
    );

    await clickButton("Clear filters");
    expect(props.onMailboxFilterChange).toHaveBeenCalledWith("all");
    expect(props.onMailPrincipalOverrideChange).toHaveBeenCalledWith("");
    expect(props.onMailSearchChange).toHaveBeenCalledWith("");
  });

  it("supports a custom acting-as principal through the escape hatch input", async () => {
    const props = defaultProps();
    await render(props);

    await setLabeledSelect("Acting as", "__custom__");
    const custom = labeledControl<HTMLInputElement>("Acting as", "input");
    expect(custom).toBeTruthy();
    const setter = Object.getOwnPropertyDescriptor(
      Object.getPrototypeOf(custom),
      "value",
    )?.set;
    setter?.call(custom, "outside-principal");
    await act(async () => {
      custom!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(props.onMailPrincipalOverrideChange).toHaveBeenCalledWith(
      "outside-principal",
    );
  });

  it("shows the honest unfiltered and in-thread empty states", async () => {
    const props = defaultProps();
    props.mailThreads = [];
    props.mailMessages = [];
    props.mailThreadDetail = null;
    props.selectedMailThreadId = null;
    await render(props);

    expect(container.textContent).toContain(
      "No direct threads yet. Start one with New Thread.",
    );
    expect(container.textContent).toContain("No messages in this thread yet.");
    expect(container.textContent).toContain("Select a thread");
  });

  it("reveals compose options and wires sender and recipients", async () => {
    const props = defaultProps();
    await render(props);

    expect(labeledControl("Sender", "select")).toBeUndefined();
    await clickButton("Options");
    await setLabeledSelect("Sender", "agent-root");
    expect(props.onMailComposeSenderChange).toHaveBeenCalledWith("agent-root");
    const recipientChip = Array.from(
      container.querySelectorAll<HTMLButtonElement>(".mc-agent-chip"),
    ).find((chip) => chip.textContent?.includes("Local Assistant"));
    expect(recipientChip).toBeTruthy();
    await act(async () => recipientChip!.click());
    expect(props.onMailComposeRecipientsChange).toHaveBeenCalledWith("default");
  });

  it("acknowledges with the active principal override and downloads the exact attachment", async () => {
    const props = defaultProps();
    props.mailPrincipalOverride = "agent-root";
    const message = messageFixture(1);
    message.attachments = [
      {
        attachment_id: "att-1",
        message_id: "msg-1",
        filename: "spec.pdf",
        mime: "application/pdf",
        sha256: "abc",
        bytes: 2048,
        created_at: 1_753_000_000_000,
      },
    ];
    props.mailMessages = [message];
    await render(props);

    await clickButton("Acknowledge");
    expect(props.onAcknowledgeMessage).toHaveBeenCalledWith(
      "msg-1",
      "agent-root",
    );

    await clickButton("spec.pdf");
    expect(props.onDownloadAttachment).toHaveBeenCalledWith(
      "msg-1",
      "att-1",
      "spec.pdf",
    );
  });

  it("locks acknowledge against same-tick duplicates and shows the busy label", async () => {
    const props = defaultProps();
    const gate = deferred<void>();
    props.onAcknowledgeMessage = vi.fn().mockReturnValue(gate.promise);
    await render(props);

    const ack = findButton("Acknowledge");
    expect(ack).toBeTruthy();
    await act(async () => {
      ack!.click();
      ack!.click();
    });
    expect(props.onAcknowledgeMessage).toHaveBeenCalledTimes(1);
    expect(container.textContent).toContain("Acknowledging");
    await act(async () => {
      gate.resolve();
      await gate.promise;
    });
    expect(container.textContent).not.toContain("Acknowledging");
  });

  it("summarizes once for a same-tick double click", async () => {
    const props = defaultProps();
    const gate = deferred<void>();
    props.onSummarizeToNote = vi.fn().mockReturnValue(gate.promise);
    await render(props);

    const summarize = findButton("Summarize");
    expect(summarize).toBeTruthy();
    await act(async () => {
      summarize!.click();
      summarize!.click();
    });
    expect(props.onSummarizeToNote).toHaveBeenCalledTimes(1);
    await act(async () => {
      gate.resolve();
      await gate.promise;
    });
  });

  it("sends exactly once for a same-tick duplicate and never clears owner input itself", async () => {
    const props = defaultProps();
    const gate = deferred<void>();
    props.onSendMessage = vi.fn().mockReturnValue(gate.promise);
    await render(props);

    const send = findButton("Send");
    expect(send).toBeTruthy();
    await act(async () => {
      send!.click();
      send!.click();
    });
    expect(props.onSendMessage).toHaveBeenCalledTimes(1);
    expect(container.textContent).toContain("Sending...");

    await act(async () => {
      gate.reject(new Error("gateway down"));
      await gate.promise.catch(() => {});
    });
    // The authoritative controller owns clearing; a failed send must leave
    // the owner's draft untouched and the button usable again.
    expect(props.onMailComposeBodyChange).not.toHaveBeenCalled();
    expect(props.onMailAttachmentFilesChange).not.toHaveBeenCalled();
    const sendAfter = findButton("Send");
    expect(sendAfter?.disabled).toBe(false);
    expect(container.textContent).not.toContain("Sending...");
  });

  it("disables send without a selected thread", async () => {
    const props = defaultProps();
    props.selectedMailThreadId = null;
    await render(props);
    expect(findButton("Send")?.disabled).toBe(true);
  });

  it("reports attachment count from the authoritative thread-scoped files", async () => {
    const props = defaultProps();
    props.mailAttachmentFiles = [
      new File(["a"], "a.txt", { type: "text/plain" }),
      new File(["b"], "b.txt", { type: "text/plain" }),
    ];
    await render(props);
    expect(container.textContent).toContain("Attach (2)");

    const fileInput = container.querySelector<HTMLInputElement>(
      '.upload-pill input[type="file"]',
    );
    expect(fileInput).toBeTruthy();
    const next = new File(["c"], "c.txt", { type: "text/plain" });
    Object.defineProperty(fileInput, "files", {
      value: [next],
      configurable: true,
    });
    await act(async () => {
      fileInput!.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(props.onMailAttachmentFilesChange).toHaveBeenCalledWith([next]);
  });

  it("creates a thread once for a same-tick duplicate and keeps the modal open on failure", async () => {
    const props = defaultProps();
    const first = deferred<boolean>();
    props.onCreateDirectThread = vi.fn().mockReturnValueOnce(first.promise);
    props.newMailThreadSubject = "Directory handoff";
    await render(props);

    await clickButton("+ New Thread");
    expect(container.textContent).toContain("New Direct Thread");
    const create = findButton("Create Thread");
    expect(create).toBeTruthy();
    await act(async () => {
      create!.click();
      create!.click();
    });
    expect(props.onCreateDirectThread).toHaveBeenCalledTimes(1);

    await act(async () => {
      first.resolve(false);
      await first.promise;
    });
    // A refused create keeps the modal (and the owner's draft props) intact.
    expect(container.textContent).toContain("New Direct Thread");
    expect(props.onNewMailThreadSubjectChange).not.toHaveBeenCalled();
    expect(props.onNewMailThreadParticipantsChange).not.toHaveBeenCalled();

    (props.onCreateDirectThread as ReturnType<typeof vi.fn>).mockResolvedValue(
      true,
    );
    await clickButton("Create Thread");
    expect(container.textContent).not.toContain("New Direct Thread");
  });

  it("paginates threads at the exact eight-item boundary", async () => {
    const props = defaultProps();
    props.mailThreads = Array.from({ length: 8 }, (_, i) => threadFixture(i + 1));
    await render(props);
    expect(container.querySelector(".mc-pagination")).toBeNull();
    expect(container.querySelectorAll(".mc-mail-thread-item")).toHaveLength(8);

    props.mailThreads = Array.from({ length: 9 }, (_, i) => threadFixture(i + 1));
    await render(props);
    expect(paginationInfo()).toBe("1 / 2");
    expect(container.querySelectorAll(".mc-mail-thread-item")).toHaveLength(8);
    await clickPagination("Next");
    expect(paginationInfo()).toBe("2 / 2");
    expect(container.querySelectorAll(".mc-mail-thread-item")).toHaveLength(1);
    expect(container.textContent).toContain("Subject 9");
    expect(paginationButton("Next")?.disabled).toBe(true);
    await clickPagination("Prev");
    expect(paginationInfo()).toBe("1 / 2");
    expect(paginationButton("Prev")?.disabled).toBe(true);
  });

  it("keeps the stored threads page clamped through shrink, shrink, and regrowth", async () => {
    const props = defaultProps();
    const seventeen = Array.from({ length: 17 }, (_, i) => threadFixture(i + 1));
    props.mailThreads = seventeen;
    await render(props);
    await clickPagination("Next");
    await clickPagination("Next");
    expect(paginationInfo()).toBe("3 / 3");
    expect(container.textContent).toContain("Subject 17");

    // Shrink to two pages: the stored page must clamp to 2, not merely render 2.
    props.mailThreads = seventeen.slice(0, 9);
    await render(props);
    expect(paginationInfo()).toBe("2 / 2");

    // Shrink to one page: pagination disappears and the stored page clamps to 1.
    props.mailThreads = seventeen.slice(0, 3);
    await render(props);
    expect(container.querySelector(".mc-pagination")).toBeNull();
    expect(container.textContent).toContain("Subject 1");

    // Regrowth must stay on the clamped page instead of resurrecting page 3.
    props.mailThreads = seventeen;
    await render(props);
    expect(paginationInfo()).toBe("1 / 3");
    expect(container.textContent).toContain("Subject 1");
    expect(container.textContent).not.toContain("Subject 17");
  });

  it("resets the thread page when the mailbox filter changes", async () => {
    const props = defaultProps();
    props.mailThreads = Array.from({ length: 9 }, (_, i) => threadFixture(i + 1));
    await render(props);
    await clickPagination("Next");
    expect(paginationInfo()).toBe("2 / 2");
    await setLabeledSelect("Mailbox", "inbox");
    expect(props.onMailboxFilterChange).toHaveBeenCalledWith("inbox");
    expect(paginationInfo()).toBe("1 / 2");
  });
});

describe("MailPage temporary File locks parity", () => {
  it("lists leases with holder, expiry, exclusivity, and the empty state", async () => {
    const props = defaultProps();
    props.leases = [leaseFixture(1), leaseFixture(2)];
    await render(props);

    await act(async () => subTabButton("File locks")!.click());
    expect(container.textContent).toContain("Advisory file locks");
    expect(container.textContent).toContain("2 active file lock(s)");
    expect(container.textContent).toContain("src/slice-1/**");
    expect(container.textContent).toContain("agent-root");
    expect(container.textContent).toContain("exclusive");

    props.leases = [];
    await render(props);
    expect(container.textContent).toContain("No active file locks.");
  });

  it("paginates leases at the exact six-item boundary and survives shrink and regrowth", async () => {
    const props = defaultProps();
    const thirteen = Array.from({ length: 13 }, (_, i) => leaseFixture(i + 1));
    props.leases = thirteen.slice(0, 6);
    await render(props);
    await act(async () => subTabButton("File locks")!.click());
    expect(container.querySelector(".mc-pagination")).toBeNull();
    expect(
      container.querySelectorAll(".mc-mail-lease-list > li"),
    ).toHaveLength(6);

    props.leases = thirteen;
    await render(props);
    expect(paginationInfo()).toBe("1 / 3");
    await clickPagination("Next");
    await clickPagination("Next");
    expect(paginationInfo()).toBe("3 / 3");
    expect(container.textContent).toContain("src/slice-13/**");

    props.leases = thirteen.slice(0, 7);
    await render(props);
    expect(paginationInfo()).toBe("2 / 2");

    props.leases = thirteen.slice(0, 5);
    await render(props);
    expect(container.querySelector(".mc-pagination")).toBeNull();

    props.leases = thirteen;
    await render(props);
    expect(paginationInfo()).toBe("1 / 3");
    expect(container.textContent).toContain("src/slice-1/**");
    expect(container.textContent).not.toContain("src/slice-13/**");
  });

  it("creates a lease once for a same-tick duplicate with the modal fields wired", async () => {
    const props = defaultProps();
    const first = deferred<boolean>();
    props.onCreateFileLease = vi.fn().mockReturnValueOnce(first.promise);
    await render(props);

    await act(async () => subTabButton("File locks")!.click());
    await clickButton("+ New file lock");
    expect(container.textContent).toContain("Reserve file lock");

    await setLabeledSelect("Acting as", "agent-root");
    expect(props.onLeaseHolderPrincipalChange).toHaveBeenCalledWith(
      "agent-root",
    );
    await setLabeledSelect("Glob Pattern", "docs/**/*");
    expect(props.onLeaseGlobPatternChange).toHaveBeenCalledWith("docs/**/*");
    await clickButton("1h");
    expect(props.onLeaseTtlMsChange).toHaveBeenCalledWith("3600000");
    await setLabeledInput("Note", "hands off");
    expect(props.onLeaseNoteChange).toHaveBeenCalledWith("hands off");
    const exclusive = labeledControl<HTMLInputElement>(
      "Exclusive",
      'input[type="checkbox"]',
    );
    expect(exclusive).toBeTruthy();
    await act(async () => exclusive!.click());
    expect(props.onLeaseExclusiveChange).toHaveBeenCalledWith(true);

    const reserve = Array.from(
      container.querySelectorAll<HTMLButtonElement>(".mc-modal-actions button"),
    ).find((button) => button.textContent?.includes("Reserve file lock"));
    expect(reserve).toBeTruthy();
    await act(async () => {
      reserve!.click();
      reserve!.click();
    });
    expect(props.onCreateFileLease).toHaveBeenCalledTimes(1);

    await act(async () => {
      first.resolve(false);
      await first.promise;
    });
    // A refused reserve keeps the modal open for the owner to correct.
    expect(container.textContent).toContain("Reserve file lock");

    (props.onCreateFileLease as ReturnType<typeof vi.fn>).mockResolvedValue(
      true,
    );
    await act(async () => reserve!.click());
    expect(container.textContent).not.toContain("Reserve file lock");
  });

  it("releases the exact lease once, keeps the confirm open on failure, and closes on success", async () => {
    const props = defaultProps();
    props.leases = [leaseFixture(1), leaseFixture(2)];
    const first = deferred<boolean>();
    props.onReleaseFileLease = vi.fn().mockReturnValueOnce(first.promise);
    await render(props);

    await act(async () => subTabButton("File locks")!.click());
    const secondRow = Array.from(
      container.querySelectorAll(".mc-mail-lease-list > li"),
    ).find((row) => row.textContent?.includes("src/slice-2/**"));
    expect(secondRow).toBeTruthy();
    await act(async () => {
      findButton("Release", secondRow!)!.click();
    });
    expect(container.textContent).toContain("Release file lock?");
    expect(
      container.querySelector(".mc-modal")?.textContent,
    ).toContain("src/slice-2/**");

    const confirm = Array.from(
      container.querySelectorAll<HTMLButtonElement>(".mc-modal-actions button"),
    ).find((button) => button.textContent?.trim() === "Release");
    expect(confirm).toBeTruthy();
    await act(async () => {
      confirm!.click();
      confirm!.click();
    });
    expect(props.onReleaseFileLease).toHaveBeenCalledTimes(1);
    expect(props.onReleaseFileLease).toHaveBeenCalledWith("lease-2");

    await act(async () => {
      first.resolve(false);
      await first.promise;
    });
    expect(container.textContent).toContain("Release file lock?");

    (props.onReleaseFileLease as ReturnType<typeof vi.fn>).mockResolvedValue(
      true,
    );
    await act(async () => confirm!.click());
    expect(container.textContent).not.toContain("Release file lock?");
  });
});

function activeSubTab(): string | undefined {
  return Array.from(
    container.querySelectorAll<HTMLButtonElement>(".mc-sub-tabs button"),
  )
    .find((button) => button.getAttribute("aria-selected") === "true")
    ?.textContent?.trim();
}

describe("MailPage Directory / Front Desk room seam", () => {
  it("lands on Front Desk for the directory room and keeps internal choices across rerenders", async () => {
    const props = defaultProps();
    props.activeRoomId = "directory";
    await render(props);

    expect(activeSubTab()).toBe("Front Desk");
    expect(container.textContent).toContain("People And Routing");
    // Front Desk shows only the server-backed person, no Mail-derived contacts.
    expect(container.textContent).toContain("You");
    expect(container.textContent).toContain("local-operator");

    // Messages and File locks remain one tap away.
    await act(async () => subTabButton("Messages")!.click());
    expect(container.textContent).toContain("Subject 1");
    // An unrelated rerender with the same room must not reset the choice.
    await render({ ...props });
    expect(activeSubTab()?.startsWith("Messages")).toBe(true);

    // Leaving the room and re-entering it relands on Front Desk.
    await render({ ...props, activeRoomId: null });
    expect(activeSubTab()?.startsWith("Messages")).toBe(true);
    await render({ ...props, activeRoomId: "directory" });
    expect(activeSubTab()).toBe("Front Desk");
  });

  it("defaults to Messages without a directory room context", async () => {
    const props = defaultProps();
    await render(props);
    expect(activeSubTab()?.startsWith("Messages")).toBe(true);
    expect(container.textContent).not.toContain("People And Routing");
  });

  it("owns the directory pin on the ready Front Desk surface only, with zero mutations", async () => {
    const props = defaultProps();
    props.activeRoomId = "directory";
    await render(props);

    const pin = Array.from(container.querySelectorAll("button")).find(
      (button) =>
        button.getAttribute("aria-label") ===
        "Pin Directory / Front Desk to Office",
    );
    expect(pin).toBeTruthy();
    await act(async () => pin!.click());
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas",
    );
    // Pinning is config-only: no runtime-config, thread, message, attachment,
    // note, acknowledge, or lease mutation may fire.
    expect(props.peopleRouting.saveRoutingDraft).not.toHaveBeenCalled();
    expect(props.peopleRouting.patchRoutingDraft).not.toHaveBeenCalled();
    expect(props.peopleRouting.detachAgentRouting).not.toHaveBeenCalled();
    expect(props.onCreateDirectThread).not.toHaveBeenCalled();
    expect(props.onSendMessage).not.toHaveBeenCalled();
    expect(props.onAcknowledgeMessage).not.toHaveBeenCalled();
    expect(props.onDownloadAttachment).not.toHaveBeenCalled();
    expect(props.onSummarizeToNote).not.toHaveBeenCalled();
    expect(props.onCreateFileLease).not.toHaveBeenCalled();
    expect(props.onReleaseFileLease).not.toHaveBeenCalled();

    // Messages and File locks are never labeled as Directory data, and the
    // pin does not follow the user off the Front Desk surface.
    await act(async () => subTabButton("Messages")!.click());
    expect(
      Array.from(container.querySelectorAll("button")).find(
        (button) =>
          button.getAttribute("aria-label") ===
          "Pin Directory / Front Desk to Office",
      ),
    ).toBeUndefined();
  });

  it("shows no dead pin on the loading Front Desk surface", async () => {
    const props = defaultProps();
    props.activeRoomId = "directory";
    props.peopleRouting = stubPeopleRouting({
      routingDraft: null,
      routingConfig: null,
      routingLoading: true,
      humanRoutingCards: [],
    });
    await render(props);
    expect(container.textContent).toContain("Loading people and routing...");
    expect(
      Array.from(container.querySelectorAll("button")).find(
        (button) =>
          button.getAttribute("aria-label") ===
          "Pin Directory / Front Desk to Office",
      ),
    ).toBeUndefined();
  });

  it("keeps the unconfigured Front Desk state honest", async () => {
    const props = defaultProps();
    props.activeRoomId = "directory";
    props.peopleRouting = stubPeopleRouting({
      gatewayConfigured: false,
      routingDraft: null,
      routingConfig: null,
      humanRoutingCards: [],
    });
    await render(props);
    expect(container.textContent).toContain(
      "Connect Mission Control to the gateway before you manage people and routing.",
    );
  });
});

describe("MailPage File locks room seam and honest lease truth", () => {
  it("lands on File locks for the locks room and keeps internal choices across rerenders", async () => {
    const props = defaultProps();
    props.activeRoomId = "locks";
    await render(props);

    expect(activeSubTab()?.startsWith("File locks")).toBe(true);
    expect(container.textContent).toContain("Advisory file locks");

    // Messages remain one tap away; an unrelated rerender must not reset.
    await act(async () => subTabButton("Messages")!.click());
    expect(container.textContent).toContain("Subject 1");
    await render({ ...props });
    expect(activeSubTab()?.startsWith("Messages")).toBe(true);

    // Leaving the room and re-entering it relands on File locks.
    await render({ ...props, activeRoomId: null });
    expect(activeSubTab()?.startsWith("Messages")).toBe(true);
    await render({ ...props, activeRoomId: "locks" });
    expect(activeSubTab()?.startsWith("File locks")).toBe(true);
  });

  it("relands per the newest explicit stable room between Directory and File locks", async () => {
    const props = defaultProps();
    props.activeRoomId = "directory";
    await render(props);
    expect(activeSubTab()).toBe("Front Desk");

    await render({ ...props, activeRoomId: "locks" });
    expect(activeSubTab()?.startsWith("File locks")).toBe(true);

    await render({ ...props, activeRoomId: "directory" });
    expect(activeSubTab()).toBe("Front Desk");
  });

  it("owns the locks pin on the File locks surface only, with zero mutations", async () => {
    const props = defaultProps();
    props.activeRoomId = "locks";
    await render(props);

    const findPin = () =>
      Array.from(container.querySelectorAll("button")).find(
        (button) =>
          button.getAttribute("aria-label") === "Pin File locks to Office",
      );
    const pin = findPin();
    expect(pin).toBeTruthy();
    await act(async () => pin!.click());
    expect(container.querySelector('[role="status"]')?.textContent).toContain(
      "On the Office canvas",
    );
    // Pinning is config-only: no lease list/create/release, Mail, routing,
    // or note mutation may fire.
    expect(props.onRetryLeases).not.toHaveBeenCalled();
    expect(props.onCreateFileLease).not.toHaveBeenCalled();
    expect(props.onReleaseFileLease).not.toHaveBeenCalled();
    expect(props.onCreateDirectThread).not.toHaveBeenCalled();
    expect(props.onSendMessage).not.toHaveBeenCalled();
    expect(props.onAcknowledgeMessage).not.toHaveBeenCalled();
    expect(props.onSummarizeToNote).not.toHaveBeenCalled();
    expect(props.peopleRouting.saveRoutingDraft).not.toHaveBeenCalled();
    expect(props.peopleRouting.patchRoutingDraft).not.toHaveBeenCalled();
    expect(props.peopleRouting.detachAgentRouting).not.toHaveBeenCalled();

    // The pin does not follow the user off the File locks surface.
    await act(async () => subTabButton("Messages")!.click());
    expect(findPin()).toBeUndefined();

    // Only the matching room owns its pin: the same section under the
    // Directory room shows no File locks pin.
    await render({ ...defaultProps(), activeRoomId: "directory" });
    await act(async () => subTabButton("File locks")!.click());
    expect(findPin()).toBeUndefined();
  });

  it("distinguishes never-loaded, loading, error, and loaded-empty lease truth", async () => {
    const props = defaultProps();
    props.activeRoomId = "locks";
    props.leases = [];
    props.leasesLoaded = false;
    await render(props);

    expect(container.textContent).toContain("File locks haven't loaded yet.");
    expect(container.textContent).not.toContain("No active file locks.");
    expect(container.textContent).not.toContain("active file lock(s)");

    await render({ ...props, leasesLoading: true });
    expect(container.textContent).toContain("Loading file locks");
    expect(container.textContent).not.toContain("No active file locks.");

    await render({
      ...props,
      leasesLoading: false,
      leasesError: "Error: lease backend down",
    });
    expect(container.textContent).toContain("lease backend down");
    expect(container.textContent).not.toContain("No active file locks.");
    const retry = findButton("Retry");
    expect(retry).toBeTruthy();
    await act(async () => retry!.click());
    expect(props.onRetryLeases).toHaveBeenCalledTimes(1);

    await render({ ...props, leasesError: null, leasesLoaded: true });
    expect(container.textContent).toContain("No active file locks.");
    expect(container.textContent).toContain("0 active file lock(s)");
  });

  it("shows server-backed holder, glob, shared/exclusive, TTL, and note for each lease", async () => {
    const props = defaultProps();
    props.activeRoomId = "locks";
    const exclusiveLease = {
      ...leaseFixture(2),
      note: "hands off the release branch",
    };
    const sharedLease = { ...leaseFixture(3), ttl_ms: 3_600_000 };
    props.leases = [exclusiveLease, sharedLease];
    await render(props);

    const rows = Array.from(
      container.querySelectorAll(".mc-mail-lease-list > li"),
    );
    const exclusiveRow = rows.find((row) =>
      row.textContent?.includes("src/slice-2/**"),
    );
    const sharedRow = rows.find((row) =>
      row.textContent?.includes("src/slice-3/**"),
    );
    expect(exclusiveRow?.textContent).toContain("agent-root");
    expect(exclusiveRow?.textContent).toContain("exclusive");
    expect(exclusiveRow?.textContent).toContain("hands off the release branch");
    expect(exclusiveRow?.textContent).toContain("15m TTL");
    expect(sharedRow?.textContent).toContain("shared");
    expect(sharedRow?.textContent).toContain("1h TTL");
    expect(sharedRow?.textContent).not.toContain("exclusive");
  });

  it("states the cooperative advisory nature and never claims filesystem enforcement", async () => {
    const props = defaultProps();
    props.activeRoomId = "locks";
    await render(props);

    expect(container.textContent).toContain(
      "Cooperative advisory reservations",
    );
    expect(container.textContent).toContain(
      "Nothing is enforced at the filesystem level",
    );
  });
});
