import { describe, expect, it } from "vitest";
import type { FloorDef } from "./floors";
import { buildGuidedTourSteps } from "./guidedTour";

const officeFloor: FloorDef = {
  id: "office",
  lamp: "4",
  icon: "bot",
  label: "The Office",
  hint: "brief · delegate · decide",
  shortcut: "4",
  order: 1,
  rooms: [
    {
      id: "desk",
      floorId: "office",
      label: "Your desk",
      route: "assistant",
      blocks: [],
    },
  ],
  defaultRoom: "desk",
};

const trenchesFloor: FloorDef = {
  id: "trenches",
  lamp: "2",
  icon: "kanban",
  label: "The Trenches",
  hint: "boards · calendar · staff",
  shortcut: "2",
  order: 3,
  rooms: [
    {
      id: "boards",
      floorId: "trenches",
      label: "Boards",
      route: "boards",
      blocks: [],
    },
    {
      id: "calendar",
      floorId: "trenches",
      label: "Calendar",
      route: "calendar",
      blocks: [],
    },
  ],
  defaultRoom: "boards",
};

describe("buildGuidedTourSteps", () => {
  it("creates one elevator stop per resolved floor, in resolved order", () => {
    const steps = buildGuidedTourSteps([officeFloor, trenchesFloor]);
    const floorSteps = steps.filter((step) => step.roomId);
    expect(floorSteps.map((step) => step.id)).toEqual([
      "floor-office",
      "floor-trenches",
    ]);
    expect(floorSteps.map((step) => step.targetId)).toEqual([
      "floor-office",
      "floor-trenches",
    ]);
  });

  it("lands each floor stop on the floor's default room by stable room id", () => {
    const steps = buildGuidedTourSteps([officeFloor, trenchesFloor]);
    expect(steps.find((step) => step.id === "floor-office")?.roomId).toBe(
      "desk",
    );
    expect(steps.find((step) => step.id === "floor-trenches")?.roomId).toBe(
      "boards",
    );
  });

  it("falls back to the first resolved room when defaultRoom was filtered away", () => {
    const floor: FloorDef = {
      ...trenchesFloor,
      defaultRoom: "history",
    };
    const steps = buildGuidedTourSteps([floor]);
    expect(steps.find((step) => step.id === "floor-trenches")?.roomId).toBe(
      "boards",
    );
  });

  it("titles floor stops with the elevator lamp and floor label", () => {
    const steps = buildGuidedTourSteps([officeFloor]);
    expect(steps.find((step) => step.id === "floor-office")?.title).toBe(
      "4F · The Office",
    );
  });

  it("names only the resolved rooms of a multi-room floor in the body", () => {
    const steps = buildGuidedTourSteps([trenchesFloor]);
    const body = steps.find((step) => step.id === "floor-trenches")?.body ?? "";
    expect(body).toContain("Boards");
    expect(body).toContain("Calendar");
    expect(body).not.toContain("Staff Directory");
    expect(body).not.toContain("History & Receipts");
  });

  it("skips floors that were resolved away entirely", () => {
    const steps = buildGuidedTourSteps([trenchesFloor]);
    expect(steps.some((step) => step.id === "floor-office")).toBe(false);
  });

  it("skips a floor left with zero rooms instead of pointing at a dead stop", () => {
    const emptyFloor: FloorDef = { ...trenchesFloor, rooms: [] };
    const steps = buildGuidedTourSteps([officeFloor, emptyFloor]);
    expect(steps.some((step) => step.id === "floor-trenches")).toBe(false);
  });

  it("gives an unknown future floor honest registry-derived copy", () => {
    const futureFloor: FloorDef = {
      id: "penthouse",
      lamp: "5",
      icon: "bot",
      label: "The Penthouse",
      hint: "future work",
      shortcut: "5",
      order: 0,
      rooms: [
        {
          id: "lounge",
          floorId: "penthouse",
          label: "Lounge",
          route: "assistant",
          blocks: [],
        },
      ],
      defaultRoom: "lounge",
    };
    const steps = buildGuidedTourSteps([futureFloor]);
    const step = steps.find((candidate) => candidate.id === "floor-penthouse");
    expect(step?.title).toBe("5F · The Penthouse");
    expect(step?.body).toContain("future work");
  });

  it("always appends the Help/Docs, Config, and command palette shell stops", () => {
    const steps = buildGuidedTourSteps([]);
    expect(steps.map((step) => step.targetId)).toEqual([
      "nav-help-shortcut",
      "nav-config",
      "topbar-command",
    ]);
    for (const step of steps) {
      expect(step.roomId).toBeUndefined();
    }
  });

  it("keeps Help/Docs described as a direct path so the tour is never the only way in", () => {
    const steps = buildGuidedTourSteps([officeFloor]);
    const help = steps.find((step) => step.targetId === "nav-help-shortcut");
    expect(help?.body).toMatch(/plain language/i);
  });
});
