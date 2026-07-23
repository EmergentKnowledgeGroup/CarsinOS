import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  notifyGlassConfigChanged,
  saveGlassConfig,
} from "../glass/config";
import { useResolvedElevator } from "./useResolvedElevator";

describe("useResolvedElevator", () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    localStorage.clear();
    container = document.createElement("div");
    document.body.appendChild(container);
  });

  afterEach(() => {
    container.remove();
    localStorage.clear();
  });

  it("removes hidden floors from the live navigation registry", async () => {
    function Harness() {
      const floors = useResolvedElevator(["team"]);
      return (
        <output>
          {floors.flatMap((floor) => floor.rooms.map((room) => room.id)).join(",")}
        </output>
      );
    }

    const root = createRoot(container);
    await act(async () => {
      root.render(<Harness />);
    });
    expect(container.textContent).toBe("staff,models");

    expect(
      saveGlassConfig({
        themeId: "auto",
        customThemes: [],
        floorOverrides: { basement: { hidden: true } },
      }).ok,
    ).toBe(true);
    await act(async () => notifyGlassConfigChanged());

    expect(container.textContent).toBe("staff");
    await act(async () => root.unmount());
  });
});
