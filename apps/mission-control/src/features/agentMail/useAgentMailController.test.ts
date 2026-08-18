import { describe, expect, it } from "vitest";
import {
  normalizeReactionEmoji,
  readThreadScopedFiles,
  writeThreadScopedFiles,
} from "./useAgentMailController";

describe("useAgentMailController helpers", () => {
  it("normalizes known reaction shortcodes to emoji glyphs", () => {
    expect(normalizeReactionEmoji(":+1:")).toBe("👍");
    expect(normalizeReactionEmoji(":rocket:")).toBe("🚀");
    expect(normalizeReactionEmoji(":heart:")).toBe("❤️");
    expect(normalizeReactionEmoji(":thinking:")).toBe("🤔");
    expect(normalizeReactionEmoji("👍")).toBe("👍");
  });

  it("stores attachments per thread so thread switches do not leak files", () => {
    const fileA = new File(["a"], "a.txt");
    const fileB = new File(["b"], "b.txt");

    const next = writeThreadScopedFiles({}, "thread-1", [fileA]);
    const then = writeThreadScopedFiles(next, "thread-2", [fileB]);

    expect(readThreadScopedFiles(then, "thread-1")).toEqual([fileA]);
    expect(readThreadScopedFiles(then, "thread-2")).toEqual([fileB]);
    expect(readThreadScopedFiles(then, "thread-3")).toEqual([]);
  });

  it("removes a thread attachment bucket when files are cleared", () => {
    const fileA = new File(["a"], "a.txt");
    const start = writeThreadScopedFiles({}, "thread-1", [fileA]);
    const next = writeThreadScopedFiles(start, "thread-1", []);

    expect(readThreadScopedFiles(next, "thread-1")).toEqual([]);
    expect(next).toEqual({});
  });
});
