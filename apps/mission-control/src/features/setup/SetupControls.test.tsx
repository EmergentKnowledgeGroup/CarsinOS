// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DEFAULT_OPSUX_RUNTIME_CONFIG } from "../../lib/opsUxConfig";
import type { OpsUxRuntimeConfig } from "../../lib/opsUxConfig";
import {
  ConnectionControls,
  FeatureControls,
  type ConnectionControlsProps,
  type FeatureControlsProps,
} from "./SetupControls";

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
});

async function render(node: React.ReactElement) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(node);
  });
}

function connectionProps(
  overrides: Partial<ConnectionControlsProps> = {},
): ConnectionControlsProps {
  return {
    idPrefix: "test",
    gatewayDraft: "http://127.0.0.1:18789/",
    onGatewayDraftChange: vi.fn(),
    tokenDraft: "",
    onTokenDraftChange: vi.fn(),
    tokenConfigured: true,
    healthState: "up",
    wsState: "connected",
    onSaveConnection: vi.fn(async () => {}),
    onReconnect: vi.fn(async () => {}),
    onClearToken: vi.fn(async () => {}),
    onOpenSetupWizard: vi.fn(),
    onOpenGuidedTour: vi.fn(),
    ...overrides,
  };
}

function featureProps(
  overrides: Partial<FeatureControlsProps> = {},
  config: OpsUxRuntimeConfig = DEFAULT_OPSUX_RUNTIME_CONFIG,
): FeatureControlsProps {
  return {
    opsUxConfig: config,
    opsUxConfigError: null,
    onPatchOpsUxControls: vi.fn(),
    usageChartsEnabled: false,
    ...overrides,
  };
}

function findButton(label: string): HTMLButtonElement {
  const match = Array.from(document.querySelectorAll("button")).find(
    (button) => button.textContent?.trim() === label,
  );
  if (!match) {
    throw new Error(`button not found: ${label}`);
  }
  return match as HTMLButtonElement;
}

function toggleByLabel(label: string): HTMLInputElement {
  const span = Array.from(document.querySelectorAll("label span")).find(
    (el) => el.textContent?.trim() === label,
  );
  const input = span?.closest("label")?.querySelector("input");
  if (!input) {
    throw new Error(`toggle not found: ${label}`);
  }
  return input as HTMLInputElement;
}

describe("ConnectionControls", () => {
  it("keeps the token input as a password field and never echoes a configured token", async () => {
    await render(<ConnectionControls {...connectionProps()} />);
    const tokenInput = Array.from(
      document.querySelectorAll<HTMLInputElement>("input"),
    ).find((input) => input.type === "password");
    expect(tokenInput).toBeDefined();
    expect(tokenInput?.value).toBe("");
    expect(tokenInput?.placeholder).toBe("token configured");
    expect(document.body.textContent).not.toContain("secret");
  });

  it("initiates one save and reports initiation synchronously", async () => {
    const props = connectionProps();
    const initiated = vi.fn();
    await render(
      <ConnectionControls {...props} onSaveInitiated={initiated} />,
    );
    await act(async () => {
      findButton("Save and connect").click();
    });
    expect(props.onSaveConnection).toHaveBeenCalledTimes(1);
    expect(initiated).toHaveBeenCalledTimes(1);
  });

  it("still saves when optional gateway history storage is unavailable", async () => {
    const props = connectionProps();
    const setItem = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(() => {
        throw new DOMException("quota exceeded", "QuotaExceededError");
      });
    await render(<ConnectionControls {...props} />);

    await act(async () => {
      findButton("Save and connect").click();
    });

    expect(props.onSaveConnection).toHaveBeenCalledTimes(1);
    setItem.mockRestore();
  });

  it("requires exactly one confirmation before forgetting the token", async () => {
    const props = connectionProps();
    await render(<ConnectionControls {...props} />);
    await act(async () => {
      findButton("Forget token").click();
    });
    expect(props.onClearToken).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain(
      "disconnect the WebSocket connection",
    );
    await act(async () => {
      findButton("Clear Token").click();
    });
    expect(props.onClearToken).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).not.toContain("Clear Token?");
  });

  it("cancelling the confirmation changes nothing", async () => {
    const props = connectionProps();
    await render(<ConnectionControls {...props} />);
    await act(async () => {
      findButton("Forget token").click();
    });
    await act(async () => {
      findButton("Cancel").click();
    });
    expect(props.onClearToken).not.toHaveBeenCalled();
    expect(document.body.textContent).not.toContain("Clear Token?");
  });

  it("routes wizard and tour launches through the shared callbacks", async () => {
    const props = connectionProps();
    await render(<ConnectionControls {...props} />);
    await act(async () => {
      findButton("Open setup wizard").click();
    });
    await act(async () => {
      findButton("Start guided tour").click();
    });
    expect(props.onOpenSetupWizard).toHaveBeenCalledTimes(1);
    expect(props.onOpenGuidedTour).toHaveBeenCalledTimes(1);
  });
});

describe("FeatureControls", () => {
  it("patches a single control through the one shared authority", async () => {
    const props = featureProps();
    await render(<FeatureControls {...props} />);
    const memoryToggle = toggleByLabel("Memory page");
    await act(async () => {
      memoryToggle.click();
    });
    expect(props.onPatchOpsUxControls).toHaveBeenCalledTimes(1);
    const patch = vi.mocked(props.onPatchOpsUxControls).mock.calls[0][0];
    expect(Object.keys(patch)).toEqual(["memory_hub"]);
  });

  it("enabling a feature under the kill switch flips both in one patch", async () => {
    const config: OpsUxRuntimeConfig = {
      ...DEFAULT_OPSUX_RUNTIME_CONFIG,
      controls: {
        ...DEFAULT_OPSUX_RUNTIME_CONFIG.controls,
        global_kill_switch: true,
        memory_hub: false,
      },
    };
    const props = featureProps({}, config);
    await render(<FeatureControls {...props} />);
    await act(async () => {
      toggleByLabel("Memory page").click();
    });
    expect(props.onPatchOpsUxControls).toHaveBeenCalledTimes(1);
    expect(vi.mocked(props.onPatchOpsUxControls).mock.calls[0][0]).toEqual({
      global_kill_switch: false,
      memory_hub: true,
    });
  });

  it("shows the persisted feature-control error verbatim", async () => {
    const props = featureProps({
      opsUxConfigError: "Feature settings could not be saved.",
    });
    await render(<FeatureControls {...props} />);
    expect(document.body.textContent).toContain(
      "Feature settings could not be saved.",
    );
  });
});
