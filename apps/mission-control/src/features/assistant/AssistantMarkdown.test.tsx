import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AssistantMarkdown } from "./AssistantMarkdown";

describe("AssistantMarkdown", () => {
  it("renders common assistant markdown without leaking raw emphasis syntax", () => {
    const html = renderToStaticMarkup(
      <AssistantMarkdown
        content={[
          "I have access to:",
          "",
          "* **file_manager**: Reads and writes local files.",
          "* **team_config**: Manages agent routes.",
          "",
          "Use `status` for a quick check.",
        ].join("\n")}
      />
    );

    expect(html).toContain("<ul>");
    expect(html).toContain("<strong>file_manager</strong>");
    expect(html).toContain("<strong>team_config</strong>");
    expect(html).toContain("<code>status</code>");
    expect(html).not.toContain("**file_manager**");
  });
});
