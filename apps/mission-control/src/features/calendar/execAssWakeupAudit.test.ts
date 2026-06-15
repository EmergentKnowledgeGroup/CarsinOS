import { describe, expect, it } from "vitest";
import type { JobRunResponse } from "../../types";
import {
  summarizeExecAssWakeupAudit,
  type ExecAssWakeupAuditSummary,
} from "./execAssWakeupAudit";

function runWithOutput(output: unknown): JobRunResponse {
  return {
    job_run_id: "job-run-1",
    job_id: "job-1",
    trigger_kind: "scheduler",
    status: "succeeded",
    attempt: 1,
    started_at: 100,
    ended_at: 200,
    error_text: null,
    output_json: JSON.stringify(output),
    created_at: 90,
  };
}

describe("summarizeExecAssWakeupAudit", () => {
  it("turns a quiet execass wakeup packet into an audit receipt", () => {
    const summary = summarizeExecAssWakeupAudit([
      runWithOutput({
        mode: "execass.wakeup",
        coverage_version: "execass.wakeup.coverage.v1",
        status: "quiet",
        llm_invoked: false,
        reason: "no_attention_items",
        checked: [
          {
            category: "jobs",
            status: "clear",
            summary: "No attention items found",
            count: 0,
          },
          {
            category: "memory",
            status: "clear",
            summary: "No attention items found",
            count: 0,
          },
        ],
        attention_items: [],
      }),
    ]);

    expect(summary).toMatchObject<ExecAssWakeupAuditSummary>({
      jobRunId: "job-run-1",
      status: "quiet",
      llmInvoked: false,
      reason: "no_attention_items",
      coverageVersion: "execass.wakeup.coverage.v1",
      checkedCount: 2,
      attentionCount: 0,
      checked: [
        {
          category: "jobs",
          status: "clear",
          summary: "No attention items found",
          count: 0,
        },
        {
          category: "memory",
          status: "clear",
          summary: "No attention items found",
          count: 0,
        },
      ],
      attentionItems: [],
      rawOutput: expect.stringContaining("\"execass.wakeup\""),
      error: null,
    });
  });

  it("shows attention categories when wakeup escalated to an LLM run", () => {
    const summary = summarizeExecAssWakeupAudit([
      runWithOutput({
        mode: "execass.wakeup",
        coverage_version: "execass.wakeup.coverage.v1",
        status: "escalated",
        llm_invoked: true,
        reason: "attention_items_found",
        checked: [
          {
            category: "approvals",
            status: "attention",
            summary: "1 attention item(s) found",
            count: 1,
          },
        ],
        attention_items: [
          {
            category: "approvals",
            kind: "pending_approvals",
            severity: "needs_user",
            summary: "Pending approval needs operator review",
          },
        ],
      }),
    ]);

    expect(summary?.status).toBe("escalated");
    expect(summary?.llmInvoked).toBe(true);
    expect(summary?.attentionCount).toBe(1);
    expect(summary?.attentionItems).toEqual([
      {
        category: "approvals",
        kind: "pending_approvals",
        severity: "needs_user",
        summary: "Pending approval needs operator review",
      },
    ]);
  });

  it("skips non-wakeup job history rows", () => {
    const summary = summarizeExecAssWakeupAudit([
      runWithOutput({ mode: "session.run", output: "hello" }),
    ]);

    expect(summary).toBeNull();
  });
});
