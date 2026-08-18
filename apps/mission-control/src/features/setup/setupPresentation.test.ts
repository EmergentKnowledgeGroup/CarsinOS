import { describe, expect, it } from "vitest";

import { connectionStatusPresentation } from "./setupPresentation";

describe("connectionStatusPresentation", () => {
  it.each([
    ["up", "Gateway: Healthy", "up"],
    ["healthy", "Gateway: Healthy", "up"],
    ["down", "Gateway: Needs attention", "down"],
    ["degraded", "Gateway: Needs attention", "down"],
    ["checking", "Gateway: Checking", "checking"],
    ["idle", "Gateway: Waiting", "idle"],
  ])("normalizes runtime gateway state %s", (healthState, label, tone) => {
    expect(
      connectionStatusPresentation({
        healthState,
        wsState: "idle",
        tokenConfigured: false,
      }),
    ).toMatchObject({
      gatewayHealthLabel: label,
      gatewayHealthTone: tone,
    });
  });
});
