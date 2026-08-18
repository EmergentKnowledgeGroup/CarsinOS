// @vitest-environment jsdom

import { act, useEffect } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { pinRoomBlocksToOffice } from "./pinToOffice";
import {
  useOfficeLayout,
  type OfficeLayoutController,
} from "./useOfficeLayout";

let container: HTMLDivElement;
let root: Root | null = null;
let controller: OfficeLayoutController | null = null;

function Harness() {
  const current = useOfficeLayout();
  useEffect(() => {
    controller = current;
  });
  return null;
}

beforeEach(async () => {
  localStorage.clear();
  container = document.createElement("div");
  document.body.appendChild(container);
  // @ts-expect-error test-only React harness flag
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  await act(async () => {
    root = createRoot(container);
    root.render(<Harness />);
  });
});

afterEach(async () => {
  await act(async () => root?.unmount());
  root = null;
  controller = null;
  container.remove();
  localStorage.clear();
});

describe("useOfficeLayout external config synchronization", () => {
  it("shows a room block pinned while the Office is already mounted", async () => {
    expect(
      controller?.placements.find((placement) => placement.id === "boards")
        ?.visible,
    ).toBe(false);

    await act(async () => {
      expect(pinRoomBlocksToOffice("boards").ok).toBe(true);
    });

    expect(
      controller?.placements.find((placement) => placement.id === "boards")
        ?.visible,
    ).toBe(true);
  });
});
