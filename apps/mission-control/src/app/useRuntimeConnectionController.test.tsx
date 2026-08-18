// @vitest-environment jsdom

import { act, useEffect, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/api", () => ({
  getGatewayHealth: vi.fn(),
  listAgents: vi.fn(),
  listBoards: vi.fn(),
}));

vi.mock("../lib/runtime", () => ({
  clearGatewayToken: vi.fn(),
  getDesktopBootstrap: vi.fn(),
  isGatewayTokenConfigured: vi.fn(),
  isTauriRuntime: vi.fn(),
  persistConnectionSettings: vi.fn(),
  setGatewayToken: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

import { listen } from "@tauri-apps/api/event";
import { getGatewayHealth, listAgents, listBoards } from "../lib/api";
import {
  clearGatewayToken,
  getDesktopBootstrap,
  isGatewayTokenConfigured,
  isTauriRuntime,
  persistConnectionSettings,
  setGatewayToken,
} from "../lib/runtime";
import type { WsLifecycleState } from "../lib/ws";
import type { Agent, BoardDetail, RuntimeConnectionSettings } from "../types";
import type { Notice } from "./useAppController";
import {
  useRuntimeConnectionController,
  type BoardSummary,
} from "./useRuntimeConnectionController";

const healthMock = vi.mocked(getGatewayHealth);
const listAgentsMock = vi.mocked(listAgents);
const listBoardsMock = vi.mocked(listBoards);
const clearGatewayTokenMock = vi.mocked(clearGatewayToken);
const getDesktopBootstrapMock = vi.mocked(getDesktopBootstrap);
const isGatewayTokenConfiguredMock = vi.mocked(isGatewayTokenConfigured);
const isTauriRuntimeMock = vi.mocked(isTauriRuntime);
const persistConnectionSettingsMock = vi.mocked(persistConnectionSettings);
const setGatewayTokenMock = vi.mocked(setGatewayToken);
const listenMock = vi.mocked(listen);

const GATEWAY_URL = "http://127.0.0.1:18789/";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

type Controller = ReturnType<typeof useRuntimeConnectionController>;

interface HarnessSnapshot {
  settings: RuntimeConnectionSettings;
  gatewayDraft: string;
  tokenDraft: string;
  tokenConfigured: boolean;
  tokenConfiguredChecked: boolean;
  healthState: string;
  wsState: WsLifecycleState;
  boards: BoardSummary[];
  agents: Agent[];
  activeBoardId: string | null;
  board: BoardDetail | null;
  setGatewayDraft: (value: string) => void;
  setTokenDraft: (value: string) => void;
  setTokenConfigured: (value: boolean) => void;
  setWsState: (value: WsLifecycleState) => void;
}

let controller: Controller;
let snapshot: HarnessSnapshot;
let notices: Notice[];

function pushNotice(notice: Notice | null) {
  if (notice) notices.push(notice);
}

const refreshBoard = vi.fn<(boardId: string, runtimeSettings?: RuntimeConnectionSettings) => Promise<void>>();
const loadMissionControlReadModels = vi.fn<(runtimeSettings?: RuntimeConnectionSettings) => Promise<void>>();
const loadRunbookReadModels = vi.fn<(runtimeSettings?: RuntimeConnectionSettings) => Promise<void>>();
const loadAgentMailReadModels = vi.fn<(runtimeSettings?: RuntimeConnectionSettings) => Promise<void>>();
const onAuthIdentityChanged = vi.fn();

interface HarnessProps {
  initialGatewayUrl: string;
  initialTokenDraft?: string;
}

function Harness(props: HarnessProps) {
  const [settings, setSettings] = useState<RuntimeConnectionSettings>({
    gateway_url: props.initialGatewayUrl,
  });
  const [gatewayDraft, setGatewayDraft] = useState(props.initialGatewayUrl);
  const [tokenDraft, setTokenDraft] = useState(props.initialTokenDraft ?? "");
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const [tokenConfiguredChecked, setTokenConfiguredChecked] = useState(false);
  const [healthState, setHealthState] = useState("unknown");
  const [wsState, setWsState] = useState<WsLifecycleState>("idle");
  const [boards, setBoards] = useState<BoardSummary[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [activeBoardId, setActiveBoardId] = useState<string | null>(null);
  const [board, setBoard] = useState<BoardDetail | null>(null);

  const current = useRuntimeConnectionController({
    settings,
    gatewayDraft,
    tokenDraft,
    setSettings,
    setGatewayDraft,
    setTokenDraft,
    setTokenConfigured,
    setTokenConfiguredChecked,
    onAuthIdentityChanged,
    setHealthState,
    setWsState,
    setNotice: pushNotice,
    setBoards,
    setAgents,
    activeBoardId,
    setActiveBoardId,
    refreshBoard,
    setBoard,
    loadMissionControlReadModels,
    loadRunbookReadModels,
    loadAgentMailReadModels,
  });

  useEffect(() => {
    controller = current;
    snapshot = {
      settings,
      gatewayDraft,
      tokenDraft,
      tokenConfigured,
      tokenConfiguredChecked,
      healthState,
      wsState,
      boards,
      agents,
      activeBoardId,
      board,
      setGatewayDraft,
      setTokenDraft,
      setTokenConfigured,
      setWsState,
    };
  }, [
    current,
    settings,
    gatewayDraft,
    tokenDraft,
    tokenConfigured,
    tokenConfiguredChecked,
    healthState,
    wsState,
    boards,
    agents,
    activeBoardId,
    board,
  ]);

  return null;
}

let root: Root | null = null;
let container: HTMLDivElement;

beforeEach(() => {
  vi.clearAllMocks();
  notices = [];
  isTauriRuntimeMock.mockReturnValue(false);
  isGatewayTokenConfiguredMock.mockResolvedValue(true);
  healthMock.mockResolvedValue({ ok: true });
  listBoardsMock.mockResolvedValue({ items: [] });
  listAgentsMock.mockResolvedValue({ items: [] });
  setGatewayTokenMock.mockResolvedValue(undefined);
  clearGatewayTokenMock.mockResolvedValue(undefined);
  refreshBoard.mockResolvedValue(undefined);
  loadMissionControlReadModels.mockResolvedValue(undefined);
  loadRunbookReadModels.mockResolvedValue(undefined);
  loadAgentMailReadModels.mockResolvedValue(undefined);
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

async function mount(props: HarnessProps) {
  await act(async () => {
    root ??= createRoot(container);
    root.render(<Harness {...props} />);
  });
}

describe("useRuntimeConnectionController bootstrap identity", () => {
  it("checks configured-token truth exactly once on mount and marks the check settled", async () => {
    await mount({ initialGatewayUrl: GATEWAY_URL });
    expect(isGatewayTokenConfiguredMock).toHaveBeenCalledTimes(1);
    expect(snapshot.tokenConfigured).toBe(true);
    expect(snapshot.tokenConfiguredChecked).toBe(true);
  });

  it("reports an unconfigured token without inventing one", async () => {
    isGatewayTokenConfiguredMock.mockResolvedValue(false);
    await mount({ initialGatewayUrl: GATEWAY_URL });
    expect(snapshot.tokenConfigured).toBe(false);
    expect(snapshot.tokenConfiguredChecked).toBe(true);
  });

  it("ignores a stale mount-time token result after Forget Token completes", async () => {
    const initialTokenCheck = deferred<boolean>();
    isGatewayTokenConfiguredMock.mockReturnValue(initialTokenCheck.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL });

    await act(async () => {
      await controller.clearToken();
    });
    expect(snapshot.tokenConfigured).toBe(false);

    await act(async () => {
      initialTokenCheck.resolve(true);
      await initialTokenCheck.promise;
    });
    expect(snapshot.tokenConfigured).toBe(false);
  });

  it("does not touch desktop bootstrap or event listeners outside the Tauri runtime", async () => {
    await mount({ initialGatewayUrl: GATEWAY_URL });
    expect(getDesktopBootstrapMock).not.toHaveBeenCalled();
    expect(listenMock).not.toHaveBeenCalled();
  });

  it("adopts the desktop bootstrap gateway URL into settings and draft", async () => {
    isTauriRuntimeMock.mockReturnValue(true);
    listenMock.mockResolvedValue(() => {});
    getDesktopBootstrapMock.mockResolvedValue({
      gateway_url: "http://127.0.0.1:28789/",
      managed_gateway: true,
      startup_error: null,
    });
    await mount({ initialGatewayUrl: GATEWAY_URL });
    expect(snapshot.settings.gateway_url).toBe("http://127.0.0.1:28789/");
    expect(snapshot.gatewayDraft).toBe("http://127.0.0.1:28789/");
    expect(notices).toEqual([]);
  });

  it("does not let a late desktop bootstrap overwrite a newer connection save", async () => {
    const bootstrap = deferred<{
      gateway_url: string;
      managed_gateway: boolean;
      startup_error: string | null;
    } | null>();
    isTauriRuntimeMock.mockReturnValue(true);
    listenMock.mockResolvedValue(() => {});
    getDesktopBootstrapMock.mockReturnValue(bootstrap.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL });

    await act(async () => {
      await controller.saveConnectionFromInputs(
        "http://10.0.0.9:18789/",
        "",
      );
    });
    await act(async () => {
      bootstrap.resolve({
        gateway_url: "http://127.0.0.1:28789/",
        managed_gateway: true,
        startup_error: null,
      });
      await bootstrap.promise;
    });

    expect(snapshot.settings.gateway_url).toBe("http://10.0.0.9:18789/");
    expect(snapshot.gatewayDraft).toBe("http://10.0.0.9:18789/");
  });

  it("does not let a late desktop bootstrap overwrite a newer gateway draft", async () => {
    const bootstrap = deferred<{
      gateway_url: string;
      managed_gateway: boolean;
      startup_error: string | null;
    } | null>();
    isTauriRuntimeMock.mockReturnValue(true);
    listenMock.mockResolvedValue(() => {});
    getDesktopBootstrapMock.mockReturnValue(bootstrap.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL });

    await act(async () => {
      snapshot.setGatewayDraft("http://10.0.0.8:18789/");
    });
    await act(async () => {
      bootstrap.resolve({
        gateway_url: "http://127.0.0.1:28789/",
        managed_gateway: true,
        startup_error: null,
      });
      await bootstrap.promise;
    });

    expect(snapshot.settings.gateway_url).toBe(GATEWAY_URL);
    expect(snapshot.gatewayDraft).toBe("http://10.0.0.8:18789/");
  });

  it("surfaces a desktop startup error as down/idle with a critical notice", async () => {
    isTauriRuntimeMock.mockReturnValue(true);
    listenMock.mockResolvedValue(() => {});
    getDesktopBootstrapMock.mockResolvedValue({
      gateway_url: "http://127.0.0.1:28789/",
      managed_gateway: true,
      startup_error: "managed gateway failed to launch",
    });
    await mount({ initialGatewayUrl: GATEWAY_URL });
    expect(snapshot.healthState).toBe("down");
    expect(snapshot.wsState).toBe("idle");
    expect(notices).toEqual([
      { tone: "critical", message: "managed gateway failed to launch" },
    ]);
  });

  it("marks the gateway down when the managed gateway terminates", async () => {
    isTauriRuntimeMock.mockReturnValue(true);
    getDesktopBootstrapMock.mockResolvedValue({
      gateway_url: "http://127.0.0.1:28789/",
      managed_gateway: true,
      startup_error: null,
    });
    let terminatedHandler: ((event: { payload?: { message?: string } }) => void) | undefined;
    listenMock.mockImplementation((async (_event: string, handler: unknown) => {
      terminatedHandler = handler as (event: { payload?: { message?: string } }) => void;
      return () => {};
    }) as unknown as typeof listen);
    await mount({ initialGatewayUrl: GATEWAY_URL });
    expect(terminatedHandler).toBeDefined();
    await act(async () => {
      terminatedHandler?.({ payload: { message: "gateway crashed hard" } });
    });
    expect(snapshot.healthState).toBe("down");
    expect(snapshot.wsState).toBe("idle");
    expect(notices).toEqual([{ tone: "critical", message: "gateway crashed hard" }]);
  });
});

describe("useRuntimeConnectionController save truth", () => {
  it("persists the trimmed gateway URL and trims the token before the secure upsert", async () => {
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      await controller.saveConnectionFromInputs(
        "  http://10.0.0.9:18789/  ",
        "  secret-token  "
      );
    });
    expect(persistConnectionSettingsMock).toHaveBeenCalledWith({
      gateway_url: "http://10.0.0.9:18789/",
    });
    expect(snapshot.settings.gateway_url).toBe("http://10.0.0.9:18789/");
    expect(setGatewayTokenMock).toHaveBeenCalledWith("secret-token");
    expect(onAuthIdentityChanged).toHaveBeenCalledTimes(1);
  });

  it("adopts an authoritative save into the one shared gateway draft", async () => {
    // The onboarding wizard saves through this same controller; the Settings
    // and Setup surfaces must then show the saved URL, never a stale blank
    // draft that a blind re-save would persist as an empty connection.
    await mount({ initialGatewayUrl: "" });
    await act(async () => {
      await controller.saveConnectionFromInputs(
        "  http://10.0.0.9:18789/  ",
        "wizard-token"
      );
    });
    expect(snapshot.gatewayDraft).toBe("http://10.0.0.9:18789/");
  });

  it("reuses the already-configured token when no new token is typed", async () => {
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      await controller.saveConnectionFromInputs(GATEWAY_URL, "");
    });
    expect(setGatewayTokenMock).not.toHaveBeenCalled();
    expect(onAuthIdentityChanged).not.toHaveBeenCalled();
    expect(healthMock).toHaveBeenCalledTimes(1);
    expect(notices).toEqual([
      { tone: "info", message: "Connection settings saved." },
    ]);
  });

  it("does not run the baseline or claim success when no token is configured", async () => {
    isGatewayTokenConfiguredMock.mockResolvedValue(false);
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      await controller.saveConnectionFromInputs(GATEWAY_URL, "");
    });
    expect(healthMock).not.toHaveBeenCalled();
    expect(onAuthIdentityChanged).not.toHaveBeenCalled();
    expect(notices).toEqual([]);
  });

  it("keeps the typed token and reports failure when the secure upsert fails", async () => {
    setGatewayTokenMock.mockRejectedValue(new Error("keychain unavailable"));
    await mount({ initialGatewayUrl: GATEWAY_URL, initialTokenDraft: "typed-secret" });
    await act(async () => {
      await controller.saveConnection();
    });
    expect(snapshot.tokenDraft).toBe("typed-secret");
    expect(onAuthIdentityChanged).not.toHaveBeenCalled();
    expect(notices).toHaveLength(1);
    expect(notices[0].tone).toBe("critical");
    expect(notices[0].message).toContain("Connection save failed");
    expect(notices[0].message).not.toContain("typed-secret");
    expect(healthMock).not.toHaveBeenCalled();
  });

  it("clears the typed token only after secure upsert and validation succeed", async () => {
    const upsert = deferred<void>();
    setGatewayTokenMock.mockReturnValue(upsert.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL, initialTokenDraft: "typed-secret" });
    let savePromise: Promise<void> | undefined;
    await act(async () => {
      savePromise = controller.saveConnection();
    });
    expect(snapshot.tokenDraft).toBe("typed-secret");
    await act(async () => {
      upsert.resolve();
      await savePromise;
    });
    expect(snapshot.tokenDraft).toBe("");
  });

  it("rethrows from saveConnectionFromInputs so callers cannot claim success on failure", async () => {
    setGatewayTokenMock.mockRejectedValue(new Error("keychain unavailable"));
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      await expect(
        controller.saveConnectionFromInputs(GATEWAY_URL, "typed-secret")
      ).rejects.toThrow("keychain unavailable");
    });
  });

  it("reports an aggregated critical notice when the baseline fails after save", async () => {
    healthMock.mockRejectedValue(new Error("connect ECONNREFUSED"));
    listAgentsMock.mockRejectedValue(new Error("agents 502"));
    await mount({
      initialGatewayUrl: GATEWAY_URL,
      initialTokenDraft: "typed-secret",
    });
    await act(async () => {
      await controller.saveConnection();
    });
    expect(snapshot.healthState).toBe("down");
    expect(notices).toHaveLength(1);
    expect(notices[0].tone).toBe("critical");
    expect(notices[0].message).toContain("Gateway health unavailable");
    expect(notices[0].message).toContain("Agent roster unavailable");
    expect(notices[0].message).not.toContain("Connection settings saved");
    expect(snapshot.tokenDraft).toBe("typed-secret");
    expect(onAuthIdentityChanged).toHaveBeenCalledTimes(1);
  });
});

describe("useRuntimeConnectionController reconnect truth", () => {
  it("refreshes the baseline against current settings and reports success once", async () => {
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      await controller.reconnect();
    });
    expect(healthMock).toHaveBeenCalledTimes(1);
    expect(notices).toEqual([{ tone: "info", message: "Connection refreshed." }]);
  });

  it("reports reconnect failure without claiming a refresh", async () => {
    healthMock.mockRejectedValue(new Error("gateway down"));
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      await controller.reconnect();
    });
    expect(notices).toHaveLength(1);
    expect(notices[0].tone).toBe("critical");
    expect(notices[0].message).toContain("Reconnect failed");
  });

  it("does nothing when the gateway URL is blank", async () => {
    await mount({ initialGatewayUrl: "   " });
    await act(async () => {
      await controller.reconnect();
    });
    expect(healthMock).not.toHaveBeenCalled();
  });
});

describe("useRuntimeConnectionController clear-token truth", () => {
  it("reports cleared state only after the secure clear succeeds", async () => {
    const clearOp = deferred<void>();
    clearGatewayTokenMock.mockReturnValue(clearOp.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      snapshot.setWsState("connected");
    });
    let clearPromise: Promise<void> | undefined;
    await act(async () => {
      clearPromise = controller.clearToken();
    });
    expect(snapshot.tokenConfigured).toBe(true);
    expect(snapshot.wsState).toBe("connected");
    expect(notices).toEqual([]);
    await act(async () => {
      clearOp.resolve();
      await clearPromise;
    });
    expect(snapshot.tokenConfigured).toBe(false);
    expect(snapshot.wsState).toBe("idle");
    expect(notices).toEqual([{ tone: "info", message: "Gateway token cleared." }]);
    expect(onAuthIdentityChanged).toHaveBeenCalledTimes(1);
  });

  it("keeps the configured-token truth and reports failure when the secure clear fails", async () => {
    clearGatewayTokenMock.mockRejectedValue(new Error("keychain locked"));
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      snapshot.setWsState("connected");
    });
    await act(async () => {
      await controller.clearToken();
    });
    expect(snapshot.tokenConfigured).toBe(true);
    expect(snapshot.wsState).toBe("connected");
    expect(notices).toHaveLength(1);
    expect(notices[0].tone).toBe("critical");
    expect(notices[0].message).toContain("Forget token failed");
    expect(notices[0].message).not.toContain("Gateway token cleared");
    expect(onAuthIdentityChanged).not.toHaveBeenCalled();
  });
});

describe("useRuntimeConnectionController synchronous action locks", () => {
  it("refuses a same-tick duplicate save so the authoritative upsert runs once", async () => {
    const upsert = deferred<void>();
    setGatewayTokenMock.mockReturnValue(upsert.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL, initialTokenDraft: "typed-secret" });
    await act(async () => {
      const first = controller.saveConnection();
      const second = controller.saveConnection();
      upsert.resolve();
      await Promise.all([first, second]);
    });
    expect(setGatewayTokenMock).toHaveBeenCalledTimes(1);
    expect(persistConnectionSettingsMock).toHaveBeenCalledTimes(1);
    expect(healthMock).toHaveBeenCalledTimes(1);
    expect(notices).toEqual([
      { tone: "info", message: "Connection settings saved." },
    ]);
  });

  it("rejects a locked saveConnectionFromInputs call without emitting a notice", async () => {
    const upsert = deferred<void>();
    setGatewayTokenMock.mockReturnValue(upsert.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      const first = controller.saveConnectionFromInputs(GATEWAY_URL, "typed-secret");
      await expect(
        controller.saveConnectionFromInputs(GATEWAY_URL, "typed-secret")
      ).rejects.toThrow(/already in progress/i);
      upsert.resolve();
      await first;
    });
    expect(setGatewayTokenMock).toHaveBeenCalledTimes(1);
    expect(notices).toEqual([
      { tone: "info", message: "Connection settings saved." },
    ]);
  });

  it("refuses a same-tick duplicate reconnect so the baseline runs once", async () => {
    const health = deferred<{ ok: boolean }>();
    healthMock.mockReturnValue(health.promise as ReturnType<typeof getGatewayHealth>);
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      const first = controller.reconnect();
      const second = controller.reconnect();
      health.resolve({ ok: true });
      await Promise.all([first, second]);
    });
    expect(healthMock).toHaveBeenCalledTimes(1);
    expect(notices).toEqual([{ tone: "info", message: "Connection refreshed." }]);
  });

  it("refuses a same-tick duplicate clear so the secure clear runs once", async () => {
    const clearOp = deferred<void>();
    clearGatewayTokenMock.mockReturnValue(clearOp.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL });
    await act(async () => {
      const first = controller.clearToken();
      const second = controller.clearToken();
      clearOp.resolve();
      await Promise.all([first, second]);
    });
    expect(clearGatewayTokenMock).toHaveBeenCalledTimes(1);
    expect(notices).toEqual([{ tone: "info", message: "Gateway token cleared." }]);
  });

  it("holds one lock across sibling connection actions while a save is in flight", async () => {
    const upsert = deferred<void>();
    setGatewayTokenMock.mockReturnValue(upsert.promise);
    await mount({ initialGatewayUrl: GATEWAY_URL, initialTokenDraft: "typed-secret" });
    let savePromise: Promise<void> | undefined;
    await act(async () => {
      savePromise = controller.saveConnection();
      await controller.reconnect();
      await controller.clearToken();
    });
    expect(clearGatewayTokenMock).not.toHaveBeenCalled();
    expect(healthMock).not.toHaveBeenCalled();
    await act(async () => {
      upsert.resolve();
      await savePromise;
    });
    expect(setGatewayTokenMock).toHaveBeenCalledTimes(1);
    expect(healthMock).toHaveBeenCalledTimes(1);
    expect(notices).toEqual([
      { tone: "info", message: "Connection settings saved." },
    ]);
  });

  it("releases the lock after a failed action so the owner can retry", async () => {
    setGatewayTokenMock.mockRejectedValueOnce(new Error("keychain unavailable"));
    await mount({ initialGatewayUrl: GATEWAY_URL, initialTokenDraft: "typed-secret" });
    await act(async () => {
      await controller.saveConnection();
    });
    expect(notices).toHaveLength(1);
    expect(notices[0].message).toContain("Connection save failed");
    setGatewayTokenMock.mockResolvedValueOnce(undefined);
    await act(async () => {
      await controller.saveConnection();
    });
    expect(setGatewayTokenMock).toHaveBeenCalledTimes(2);
    expect(notices).toHaveLength(2);
    expect(notices[1]).toEqual({
      tone: "info",
      message: "Connection settings saved.",
    });
  });
});
