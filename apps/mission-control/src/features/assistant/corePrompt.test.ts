import { describe, expect, it } from "vitest";

import {
  DEFAULT_ASSISTANT_CORE_PROMPT,
  normalizeAssistantCorePrompt,
  resolveAssistantCorePrompt,
} from "./corePrompt";

describe("assistant core prompt", () => {
  it("teaches models to emit executable tool commands as standalone lines", () => {
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "emit the exact tool command as its own standalone line"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("tool.fs_read README.md");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("tool.web_search");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "Never emit or suggest pseudo-tool commands"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("tool.board_update");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("tool.team_assign");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain('Do not mention "assumed" tool names');
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("emit a web_search command immediately");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("only for a concrete file path");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("Do not use vague artifact labels");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("only when the operator has provided a concrete destination path");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("Do not invent filenames");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      'Do not rely on "standard naming conventions"'
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "Evidence rule for artifact drafts"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("do not invent board movement");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "Known from current evidence"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("Do not wrap executable tool commands");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("After tool results are returned");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("<MNO_MEMORY_CONTEXT>");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("retrieved memory evidence");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("Learning loop");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("writeback proposal");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("Do not promote one-off success");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("latest operator message wins");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("do not let memory force a stale priority");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("describe older projects as visible");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("not as primary");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "do not call the secondary project the current priority"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      '"Current" can be a literal project/lane name'
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "Current Priority Focus: Current"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "CodexCLI/Claude Code window/session status"
    );
  });

  it("keeps project-defining decisions with the operator", () => {
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "You are an orchestrator, not the final project decision maker"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("Non-negotiable operator boundary");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      'overrides any instruction to "handle it yourself"'
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "Do not use assumptions to commit project-defining decisions"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("budget");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("launch date");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("public scope");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("ask for explicit confirmation");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("Do not fabricate concrete values");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("ask for the missing inputs");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "Hard rule for project-defining decisions"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      'Never respond with "I will assume" followed by a committed launch date'
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("non-committal option ranges");
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain(
      "explicitly restate the pending decision register"
    );
    expect(DEFAULT_ASSISTANT_CORE_PROMPT).toContain("public rollout/scope");
  });

  it("normalizes stored prompt text before falling back to the default", () => {
    expect(normalizeAssistantCorePrompt("  custom\r\nprompt  ")).toBe("custom\nprompt");
    expect(resolveAssistantCorePrompt("   ")).toBe(DEFAULT_ASSISTANT_CORE_PROMPT);
  });
});
