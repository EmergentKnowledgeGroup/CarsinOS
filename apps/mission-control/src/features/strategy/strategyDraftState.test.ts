import { describe, expect, it } from "vitest";
import {
  isGoalDraftDirty,
  isProjectDraftDirty,
  isTaskDraftDirty,
} from "./strategyDraftState";

describe("strategyDraftState", () => {
  it("detects clean goal drafts", () => {
    const draft = {
      slug: "release",
      title: "Release",
      summary: "Ship",
      status: "active",
      owner_agent_id: "agent-1",
      target_date: "2026-03-13T09:00",
    };

    expect(isGoalDraftDirty(draft, draft)).toBe(false);
  });

  it("detects changed project drafts", () => {
    const baseline = {
      goal_id: "goal-1",
      slug: "ops",
      name: "Ops",
      summary: "",
      status: "active",
      owner_agent_id: "",
      workspace_root: ".",
      budget_month_usd: "",
    };
    const changed = {
      ...baseline,
      budget_month_usd: "2500",
    };

    expect(isProjectDraftDirty(changed, baseline)).toBe(true);
  });

  it("detects changed task linkage drafts", () => {
    const baseline = {
      task_id: "task-1",
      project_id: "project-1",
      parent_task_id: "",
      title: "Track issue",
      detail: "",
      status: "todo",
      priority: "normal",
      owner_agent_id: "",
      due_at: "",
      blocked_reason: "",
      linked_board_card_id: "",
      linked_job_id: "",
    };
    const changed = {
      ...baseline,
      linked_board_card_id: "card-42",
    };

    expect(isTaskDraftDirty(changed, baseline)).toBe(true);
  });

  it("does not mark equal drafts dirty when key insertion order differs", () => {
    const baseline = {
      goal_id: "goal-1",
      slug: "ops",
      name: "Ops",
      summary: "",
      status: "active",
      owner_agent_id: "",
      workspace_root: ".",
      budget_month_usd: "",
    };
    const reordered = {
      workspace_root: ".",
      goal_id: "goal-1",
      summary: "",
      name: "Ops",
      slug: "ops",
      status: "active",
      owner_agent_id: "",
      budget_month_usd: "",
    };

    expect(isProjectDraftDirty(reordered, baseline)).toBe(false);
  });
});
