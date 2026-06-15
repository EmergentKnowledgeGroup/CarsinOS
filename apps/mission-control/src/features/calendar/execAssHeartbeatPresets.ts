import type { Agent, CreateJobRequest, MissionControlCalendarJob } from "../../types";

export type ExecAssHeartbeatPresetKind = "check_in" | "daily_learning" | "job_watch";

export interface ExecAssHeartbeatAgent {
  agent_id: string;
  name: string;
  model_provider: string;
  model_id: string;
}

interface BuildExecAssHeartbeatPresetOptions {
  kind: ExecAssHeartbeatPresetKind;
  agent: ExecAssHeartbeatAgent;
  intervalMinutes?: number;
  existingJobCount?: number;
}

export interface ExecAssHeartbeatPresetSummary {
  key: ExecAssHeartbeatPresetKind;
  label: string;
  defaultMinutes: number;
  description: string;
}

type ExecAssHeartbeatJobLike = Pick<MissionControlCalendarJob, "enabled" | "name"> & {
  payload_json?: string;
};

export const EXECASS_HEARTBEAT_PRESETS: ExecAssHeartbeatPresetSummary[] = [
  {
    key: "check_in",
    label: "Check in",
    defaultMinutes: 60,
    description: "Run a quiet preflight on a regular cadence and only wake ExecAss when work needs attention.",
  },
  {
    key: "daily_learning",
    label: "Daily learning review",
    defaultMinutes: 1440,
    description: "Ask ExecAss once a day what changed, what it learned, and what memory updates need approval.",
  },
  {
    key: "job_watch",
    label: "Job watcher",
    defaultMinutes: 15,
    description: "Keep an eye on scheduled jobs, failures, runbooks, and task-board drift.",
  },
];

export function resolveExecAssHeartbeatAgent(
  agents: Agent[],
  preferredAgentId: string | null | undefined
): ExecAssHeartbeatAgent | null {
  const normalizedPreferred = preferredAgentId?.trim();
  const selected =
    agents.find((agent) => agent.agent_id === normalizedPreferred) ??
    agents.find((agent) => agent.agent_id === "default") ??
    agents[0] ??
    null;
  if (!selected) {
    return null;
  }
  return {
    agent_id: selected.agent_id,
    name: selected.name || selected.agent_id,
    model_provider: selected.model_provider,
    model_id: selected.model_id,
  };
}

export function hasExecAssHeartbeatJob(
  jobs: ExecAssHeartbeatJobLike[],
  kind: ExecAssHeartbeatPresetKind
): boolean {
  const preset = `execass.${kind}`;
  const presetSummary = EXECASS_HEARTBEAT_PRESETS.find((item) => item.key === kind);
  const expectedName = presetSummary ? `ExecAss ${presetSummary.label}` : "";
  return jobs.some((job) => {
    if (!job.enabled) {
      return false;
    }
    if (expectedName && job.name === expectedName) {
      return true;
    }
    if (!job.payload_json) {
      return false;
    }
    try {
      const payload = JSON.parse(job.payload_json) as { preset?: unknown };
      return payload.preset === preset;
    } catch {
      return false;
    }
  });
}

export function buildExecAssHeartbeatJobRequest({
  kind,
  agent,
  intervalMinutes,
  existingJobCount = 0,
}: BuildExecAssHeartbeatPresetOptions): CreateJobRequest {
  const preset = EXECASS_HEARTBEAT_PRESETS.find((item) => item.key === kind);
  if (!preset) {
    throw new Error(`Unknown ExecAss heartbeat preset: ${kind}`);
  }
  const minutes = normalizeIntervalMinutes(
    intervalMinutes ?? preset.defaultMinutes,
    kind === "daily_learning" ? 1440 : 5,
    kind === "daily_learning" ? 10080 : 1440
  );
  const sessionKey = `execass:heartbeat:${kind}`;
  const input = buildHeartbeatInput(kind, minutes, existingJobCount);

  return {
    agent_id: agent.agent_id,
    name: `ExecAss ${preset.label}`,
    enabled: true,
    schedule_kind: "interval",
    interval_seconds: minutes * 60,
    payload_json: {
      mode: "execass.wakeup",
      preset: `execass.${kind}`,
      source: "mission-control.execass-heartbeat",
      quiet_if_no_change: true,
      escalate_mode: "session.run",
      notify_policy: "attention_only",
      agent_id: agent.agent_id,
      assistant_agent_id: agent.agent_id,
      model_provider: agent.model_provider,
      model_id: agent.model_id,
      session_key: sessionKey,
      session_title: `ExecAss heartbeat / ${preset.label}`,
      input,
    },
    max_retries: 1,
    retry_backoff_ms: 30_000,
    timeout_ms: 300_000,
  };
}

function normalizeIntervalMinutes(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.max(min, Math.min(max, Math.round(value)));
}

function buildHeartbeatInput(
  kind: ExecAssHeartbeatPresetKind,
  intervalMinutes: number,
  existingJobCount: number
): string {
  const shared =
    "This is an internal preflight for ExecAss inside CarsinOS. " +
    "Do not produce user-facing chatter when no attention item exists. " +
    "If the preflight escalates to a model run, act as the user's proactive executive assistant. " +
    "Use the available CarsinOS context, task board, runbooks, memory surfaces, job status, and tool inventory when they are available. " +
    "Be concise and concrete. Do not claim work was completed unless you verified it. " +
    "If a durable memory or runbook update is useful, propose it for operator approval instead of silently persisting it.";

  if (kind === "daily_learning") {
    return `${shared}

Daily learning review:
- Summarize what changed since the last review.
- Identify what you learned about the user's preferences, active projects, recurring blockers, and useful skills/tools.
- Check whether memory context is helping or stale.
- Propose specific memory/runbook updates that would make future turns smarter.
- End with the highest-value next action for the user to approve or delegate.`;
  }

  if (kind === "job_watch") {
    return `${shared}

Scheduled job watch:
- Review configured jobs, recent job runs, failed runs, waiting approvals, and runbook/task links.
- There are currently about ${existingJobCount} visible scheduled jobs in Mission Control.
- If everything is healthy, stay quiet.
- If something needs attention, explain the issue, what you checked, and the safest next action.
- Do not restart or mutate jobs unless the user explicitly configured that behavior.`;
  }

  return `${shared}

Regular check-in:
- This wakeup is configured for about every ${intervalMinutes} minutes.
- Check current projects, tasks, approvals, recent events, and memory context.
- Ask the user for attention only when there is a decision, blocker, useful opportunity, or stale context.
- Otherwise stay quiet and let the scheduler record the internal heartbeat result.`;
}
