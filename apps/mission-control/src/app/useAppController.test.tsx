// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, test } from "vitest";

import { useAppController } from "./useAppController";
import { DEFAULT_FLOORS, resolveElevator } from "../glass/floors";

type AppController = ReturnType<typeof useAppController>;

let container: HTMLDivElement;
let root: Root | null = null;
let controller: AppController | null = null;

function Harness() {
  const current = useAppController();
  useEffect(() => {
    controller = current;
  });
  return null;
}

async function render() {
  await act(async () => {
    root ??= createRoot(container);
    root.render(<Harness />);
  });
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  controller = null;
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  container.remove();
});

describe("useAppController room identity", () => {
  test("derives the active room from the initial tab", async () => {
    await render();
    expect(controller?.activeTab).toBe("boards");
    expect(controller?.activeRoomId).toBe("boards");
  });

  test("selectRoom navigates by stable room id and keeps it lit on a shared route", async () => {
    await render();
    await act(async () => {
      controller?.selectRoom("models");
    });
    expect(controller?.activeTab).toBe("team");
    expect(controller?.activeRoomId).toBe("models");

    // A programmatic navigation to the same surface keeps the chosen room.
    await act(async () => {
      controller?.setActiveTab("team");
    });
    expect(controller?.activeRoomId).toBe("models");
  });

  test("a tab-only navigation resolves to the first registry owner of that tab", async () => {
    await render();
    await act(async () => {
      controller?.selectRoom("models");
    });
    await act(async () => {
      controller?.setActiveTab("window");
    });
    expect(controller?.activeRoomId).toBe("reef");

    // Coming back to the shared surface without an explicit room selection
    // resolves to the first registry owner, not the stale selection.
    await act(async () => {
      controller?.setActiveTab("team");
    });
    expect(controller?.activeRoomId).toBe("staff");
  });

  test("the shared connectors route keeps Setup and Connectors identities distinct", async () => {
    await render();
    await act(async () => {
      controller?.selectRoom("setup");
    });
    expect(controller?.activeTab).toBe("connectors");
    expect(controller?.activeRoomId).toBe("setup");

    await act(async () => {
      controller?.selectRoom("connectors");
    });
    expect(controller?.activeTab).toBe("connectors");
    expect(controller?.activeRoomId).toBe("connectors");

    // Leaving and returning by tab only resolves to the first registry
    // owner of the route, which is the Connectors room.
    await act(async () => {
      controller?.setActiveTab("boards");
    });
    await act(async () => {
      controller?.setActiveTab("connectors");
    });
    expect(controller?.activeRoomId).toBe("connectors");
  });

  test("the shared mail route keeps Directory and File locks identities distinct", async () => {
    await render();
    await act(async () => {
      controller?.selectRoom("locks");
    });
    expect(controller?.activeTab).toBe("mail");
    expect(controller?.activeRoomId).toBe("locks");

    await act(async () => {
      controller?.selectRoom("directory");
    });
    expect(controller?.activeTab).toBe("mail");
    expect(controller?.activeRoomId).toBe("directory");

    // Leaving and returning by tab only resolves to the first registry
    // owner of the route, which is the Directory / Front Desk room.
    await act(async () => {
      controller?.selectRoom("locks");
    });
    await act(async () => {
      controller?.setActiveTab("boards");
    });
    await act(async () => {
      controller?.setActiveTab("mail");
    });
    expect(controller?.activeRoomId).toBe("directory");
  });

  test("selectRoom fails closed on unknown room ids", async () => {
    await render();
    let selected: boolean | undefined;
    await act(async () => {
      selected = controller?.selectRoom("haunted-room");
    });
    expect(selected).toBe(false);
    expect(controller?.activeTab).toBe("boards");
    expect(controller?.activeRoomId).toBe("boards");
  });

  test("selectRoom fails closed when the resolved registry hides the room", async () => {
    await render();
    const visibleFloors = resolveElevator(DEFAULT_FLOORS, {
      capabilities: ["execass", "agent-mail"],
      overrides: { basement: { hidden: true } },
    });
    let selected: boolean | undefined;
    await act(async () => {
      selected = controller?.selectRoom("models", visibleFloors);
    });
    expect(selected).toBe(false);
    expect(controller?.activeTab).toBe("boards");
    expect(controller?.activeRoomId).toBe("boards");
  });

  test("selectRoom fails closed for a capability-filtered room", async () => {
    await render();
    const visibleFloors = resolveElevator(DEFAULT_FLOORS, {
      capabilities: ["execass"],
    });
    await act(async () => {
      controller?.selectRoom("chatter", visibleFloors);
    });
    expect(controller?.activeTab).toBe("boards");
    expect(controller?.activeRoomId).toBe("boards");
  });

  test("tabs no registry room owns report no active room", async () => {
    await render();
    await act(async () => {
      controller?.setActiveTab("help");
    });
    expect(controller?.activeRoomId).toBeNull();
  });
});
