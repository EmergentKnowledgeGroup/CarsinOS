// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { fixturePolicyResponse } from "../../glass/execass/fixtures";
import { PolicyRoomPage } from "./PolicyRoomPage";
import type {
  ExecassPolicyController,
  PolicyDraft,
} from "./useExecassPolicyController";

let container: HTMLDivElement;
let root: Root | null = null;

beforeEach(() => {
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
});

function stubController(
  overrides: Partial<ExecassPolicyController> = {},
): ExecassPolicyController {
  return {
    phase: "loaded",
    policy: fixturePolicyResponse(),
    error: null,
    draft: null,
    conflict: false,
    updateBusy: false,
    refresh: vi.fn(async () => {}),
    beginDraft: vi.fn(),
    setDraftProfile: vi.fn(),
    setDraftRule: vi.fn(),
    setDraftChangeSummary: vi.fn(),
    discardDraft: vi.fn(),
    reconcileDraft: vi.fn(() => ({ ok: true as const, message: "rebased" })),
    applyDraft: vi.fn(async () => ({ ok: true as const, message: "ok" })),
    ...overrides,
  };
}

function draftFixture(overrides: Partial<PolicyDraft> = {}): PolicyDraft {
  return {
    profile: "full_send",
    rules: fixturePolicyResponse().rules.map((rule) => ({ ...rule })),
    changeSummary: "Go faster",
    baseRevision: 7,
    baseProfile: fixturePolicyResponse().profile ?? null,
    baseRules: fixturePolicyResponse().rules.map((rule) => ({
      ...rule,
      technical_resource_quotas: rule.technical_resource_quotas.map((quota) => ({
        ...quota,
      })),
    })),
    ...overrides,
  };
}

async function render(controller: ExecassPolicyController) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(<PolicyRoomPage controller={controller} />);
  });
}

function text(): string {
  return container.textContent ?? "";
}

function click(selector: string) {
  const el = container.querySelector<HTMLElement>(selector);
  expect(el, `expected element ${selector}`).toBeTruthy();
  act(() => {
    el!.click();
  });
}

describe("PolicyRoomPage honest states", () => {
  it("shows a loading truth without inventing policy facts", async () => {
    await render(stubController({ phase: "loading", policy: null }));
    expect(
      container.querySelector('[data-testid="policy-room-loading"]'),
    ).toBeTruthy();
    expect(
      container.querySelector('[data-testid="policy-profile-balanced"]'),
    ).toBeNull();
  });

  it("shows the error truth with a working retry", async () => {
    const controller = stubController({
      phase: "error",
      policy: null,
      error: "The gateway could not complete this request.",
    });
    await render(controller);
    expect(text()).toContain("The gateway could not complete this request.");
    click('[data-testid="policy-room-retry"]');
    expect(controller.refresh).toHaveBeenCalledTimes(1);
  });

  it("states the unconfigured bootstrap truth without inventing a profile", async () => {
    await render(
      stubController({
        policy: fixturePolicyResponse({
          configured: false,
          profile: null,
          effective_operational_summary:
            "No owner policy is configured yet.",
        }),
      }),
    );
    expect(text()).toMatch(/hasn't set ground rules yet|no owner policy/i);
    expect(container.querySelector(".mc-policy-profile.is-current")).toBeNull();
  });
});

describe("PolicyRoomPage owner language", () => {
  it("presents the policy truth with the three plain-language commitments", async () => {
    await render(stubController());
    expect(text()).toContain(
      "Balanced autonomy - ordinary work proceeds, dangerous actions get one confirmation.",
    );
    expect(text()).toMatch(/revision 7/i);
    // Exact owner directions are not policed.
    expect(text()).toMatch(/never policed|not policed/i);
    // Dangerous work gets exactly one consequence confirmation.
    expect(text()).toMatch(/exactly one confirmation/i);
    // Derived or unattended work follows this policy.
    expect(text()).toMatch(/without a fresh instruction/i);
  });

  it("labels quotas as technical execution limits, never money", async () => {
    await render(stubController());
    const details = container.querySelector<HTMLDetailsElement>(
      '[data-testid="policy-rule-advanced-rule-1"]',
    );
    expect(details).toBeTruthy();
    act(() => {
      details!.open = true;
    });
    expect(text()).toMatch(/technical execution limits/i);
    expect(text()).toMatch(/not money/i);
    expect(text()).toMatch(/500,000|500000/);
  });

  it("renders the Policy pin only on this ready room surface", async () => {
    await render(stubController());
    expect(
      container.querySelector('[aria-label="Pin Policy to Office"]'),
    ).toBeTruthy();
  });
});

describe("PolicyRoomPage editing", () => {
  it("starts a draft from a profile card without touching anything else", async () => {
    const controller = stubController();
    await render(controller);
    click('[data-testid="policy-profile-full_send"]');
    expect(controller.beginDraft).toHaveBeenCalledTimes(1);
    expect(controller.setDraftProfile).toHaveBeenCalledWith("full_send");
    expect(controller.applyDraft).not.toHaveBeenCalled();
  });

  it("edits one bounded rule field while preserving every untouched field", async () => {
    const controller = stubController({ draft: draftFixture() });
    await render(controller);
    const details = container.querySelector<HTMLDetailsElement>(
      '[data-testid="policy-rule-advanced-rule-1"]',
    );
    act(() => {
      details!.open = true;
    });
    const input = container.querySelector<HTMLInputElement>(
      '[data-testid="policy-rule-parallelism-rule-1"]',
    );
    expect(input).toBeTruthy();
    act(() => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )!.set!;
      setter.call(input!, "6");
      input!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(controller.setDraftRule).toHaveBeenCalledTimes(1);
    const [index, rule] = vi.mocked(controller.setDraftRule).mock.calls[0]!;
    expect(index).toBe(0);
    expect(rule).toEqual({
      ...draftFixture().rules[0]!,
      parallelism_limit: 6,
    });
  });

  it("shows one review step and applies exactly once on confirmation", async () => {
    const controller = stubController({ draft: draftFixture() });
    await render(controller);
    click('[data-testid="policy-review-open"]');
    expect(
      container.querySelector('[data-testid="policy-review"]'),
    ).toBeTruthy();
    // The review states the concrete consequence before any confirmation.
    expect(text()).toMatch(/from revision 7/i);
    expect(text()).toMatch(/full send/i);
    await act(async () => {
      container
        .querySelector<HTMLElement>('[data-testid="policy-confirm"]')!
        .click();
    });
    expect(controller.applyDraft).toHaveBeenCalledTimes(1);
  });

  it("disables the confirmation while an apply is in flight", async () => {
    const controller = stubController({
      draft: draftFixture(),
      updateBusy: true,
    });
    await render(controller);
    click('[data-testid="policy-review-open"]');
    const confirm = container.querySelector<HTMLButtonElement>(
      '[data-testid="policy-confirm"]',
    );
    expect(confirm?.disabled).toBe(true);
  });

  it("requires the owner's change summary before the confirmation is live", async () => {
    const controller = stubController({
      draft: draftFixture({ changeSummary: "   " }),
    });
    await render(controller);
    click('[data-testid="policy-review-open"]');
    const confirm = container.querySelector<HTMLButtonElement>(
      '[data-testid="policy-confirm"]',
    );
    expect(confirm?.disabled).toBe(true);
  });

  it("keeps the retained draft visible with an honest conflict banner", async () => {
    const reconcileDraft = vi.fn(() => ({
      ok: true as const,
      message: "rebased",
    }));
    const controller = stubController({
      policy: fixturePolicyResponse({ revision: 8 }),
      draft: draftFixture(),
      conflict: true,
      reconcileDraft,
    });
    await render(controller);
    const banner = container.querySelector('[data-testid="policy-conflict"]');
    expect(banner).toBeTruthy();
    expect(banner?.textContent).toMatch(/changed while you were editing/i);
    expect(banner?.textContent).toMatch(/kept/i);
    click('[data-testid="policy-reconcile"]');
    expect(reconcileDraft).toHaveBeenCalledTimes(1);
  });

  it("disables confirmation until a conflicted draft is reconciled", async () => {
    await render(
      stubController({
        policy: fixturePolicyResponse({ revision: 8 }),
        draft: draftFixture(),
        conflict: true,
      }),
    );
    click('[data-testid="policy-review-open"]');
    expect(
      container.querySelector<HTMLButtonElement>(
        '[data-testid="policy-confirm"]',
      )?.disabled,
    ).toBe(true);
  });

  it("withholds the pin until authoritative policy truth is loaded", async () => {
    await render(
      stubController({ phase: "loading", policy: null, draft: null }),
    );
    expect(
      container.querySelector('[aria-label="Pin Policy to Office"]'),
    ).toBeNull();
    await render(stubController({ phase: "error", policy: null, draft: null }));
    expect(
      container.querySelector('[aria-label="Pin Policy to Office"]'),
    ).toBeNull();
  });
});
