import { describe, expect, it } from "vitest";
import {
  buildExecAssHeartbeatJobRequest,
  hasExecAssHeartbeatJob,
  resolveExecAssHeartbeatAgent,
} from "./execAssHeartbeatPresets";
import type { Agent } from "../../types";

const execAssAgent: Agent = {
  agent_id: "execass",
  name: "ExecAss",
  model_provider: "lmstudio",
  model_id: "gemma-4-e4b-uncensored-hauhaucs-aggressive",
};

describe("execAssHeartbeatPresets", () => {
  it("builds a quiet wakeup check-in job bound to the selected agent model", () => {
    const request = buildExecAssHeartbeatJobRequest({
      kind: "check_in",
      agent: execAssAgent,
      intervalMinutes: 90,
    });

    expect(request.name).toBe("ExecAss Check in");
    expect(request.schedule_kind).toBe("interval");
    expect(request.interval_seconds).toBe(5400);
    expect(request.agent_id).toBe("execass");
    expect(request.timeout_ms).toBe(300000);
    expect(request.payload_json).toMatchObject({
      mode: "execass.wakeup",
      preset: "execass.check_in",
      agent_id: "execass",
      assistant_agent_id: "execass",
      model_provider: "lmstudio",
      model_id: "gemma-4-e4b-uncensored-hauhaucs-aggressive",
      session_key: "execass:heartbeat:check_in",
      quiet_if_no_change: true,
      escalate_mode: "session.run",
      notify_policy: "attention_only",
    });
    expect(String(request.payload_json?.input)).toContain("internal preflight");
    expect(String(request.payload_json?.input)).toContain("Regular check-in");
    expect(String(request.payload_json?.input)).toContain("task board");
    expect(String(request.payload_json?.input)).not.toContain("Hello!");
  });

  it("clamps unsafe intervals and keeps daily learning as a once-a-day review", () => {
    const request = buildExecAssHeartbeatJobRequest({
      kind: "daily_learning",
      agent: execAssAgent,
      intervalMinutes: 5,
    });

    expect(request.interval_seconds).toBe(86400);
    expect(request.payload_json?.preset).toBe("execass.daily_learning");
    expect(String(request.payload_json?.input)).toContain("Daily learning review");
    expect(String(request.payload_json?.input)).toContain("Propose specific memory/runbook updates");
  });

  it("includes job-count context in the job watcher prompt", () => {
    const request = buildExecAssHeartbeatJobRequest({
      kind: "job_watch",
      agent: execAssAgent,
      intervalMinutes: 12,
      existingJobCount: 7,
    });

    expect(request.interval_seconds).toBe(720);
    expect(request.payload_json?.preset).toBe("execass.job_watch");
    expect(String(request.payload_json?.input)).toContain("about 7 visible scheduled jobs");
  });

  it("resolves the preferred agent, then default, then first agent", () => {
    const agents: Agent[] = [
      { ...execAssAgent, agent_id: "default", name: "Default" },
      { ...execAssAgent, agent_id: "special", name: "Special" },
    ];

    expect(resolveExecAssHeartbeatAgent(agents, "special")?.agent_id).toBe("special");
    expect(resolveExecAssHeartbeatAgent(agents, "missing")?.agent_id).toBe("default");
    expect(resolveExecAssHeartbeatAgent([{ ...execAssAgent, agent_id: "one" }], null)?.agent_id).toBe("one");
    expect(resolveExecAssHeartbeatAgent([], null)).toBeNull();
  });

  it("detects enabled ExecAss heartbeat jobs by preset", () => {
    const job = {
      job_id: "job-1",
      agent_id: "execass",
      name: "ExecAss Check in",
      enabled: true,
      schedule_kind: "interval",
      interval_seconds: 3600,
      run_at_ms: null,
      cron_expr: null,
      next_run_at: null,
      payload_json: JSON.stringify({ mode: "execass.wakeup", preset: "execass.check_in" }),
      lane: "next_up",
      primary_action: "run",
      max_retries: 1,
      retry_backoff_ms: 30000,
      timeout_ms: 300000,
      last_run_at: null,
      last_error: null,
      created_at: 1,
      updated_at: 1,
    };

    expect(hasExecAssHeartbeatJob([job], "check_in")).toBe(true);
    expect(hasExecAssHeartbeatJob([{ ...job, enabled: false }], "check_in")).toBe(false);
    expect(hasExecAssHeartbeatJob([job], "daily_learning")).toBe(false);
  });
});
