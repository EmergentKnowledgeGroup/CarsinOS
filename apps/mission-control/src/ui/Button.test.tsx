import { createRoot } from "react-dom/client";
import { act } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { Button } from "./Button";

describe("Button", () => {
  let host: HTMLDivElement | null = null;

  afterEach(() => {
    host?.remove();
    host = null;
  });

  it("uses neutral hierarchy by default and exposes busy state", async () => {
    host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(<Button busy>Save changes</Button>);
    });

    const button = host.querySelector("button");
    expect(button?.classList.contains("secondary")).toBe(true);
    expect(button?.getAttribute("aria-busy")).toBe("true");
    expect(button?.hasAttribute("disabled")).toBe(true);
    expect(button?.textContent).toContain("Working…");

    await act(async () => root.unmount());
  });

  it("requires an explicit primary variant", async () => {
    host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(<Button variant="primary">Continue</Button>);
    });

    expect(host.querySelector("button")?.classList.contains("primary")).toBe(true);
    await act(async () => root.unmount());
  });
});
