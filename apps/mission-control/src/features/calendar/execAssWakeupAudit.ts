import type { JobRunResponse } from "../../types";

export interface ExecAssWakeupCheckedCategory {
  category: string;
  status: string;
  summary: string;
  count: number;
}

export interface ExecAssWakeupAttentionItem {
  category: string;
  kind: string;
  severity: string;
  summary: string;
}

export interface ExecAssWakeupAuditSummary {
  jobRunId: string;
  status: string;
  llmInvoked: boolean;
  reason: string;
  coverageVersion: string;
  checkedCount: number;
  attentionCount: number;
  checked: ExecAssWakeupCheckedCategory[];
  attentionItems: ExecAssWakeupAttentionItem[];
  rawOutput: string;
  error: string | null;
}

function asObject(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" && value.trim().length > 0 ? value : fallback;
}

function asNumber(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function parseOutput(outputJson: string | null): Record<string, unknown> | null {
  if (!outputJson) {
    return null;
  }
  try {
    return asObject(JSON.parse(outputJson));
  } catch {
    return null;
  }
}

function parseChecked(value: unknown): ExecAssWakeupCheckedCategory[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => {
      const object = asObject(item);
      if (!object) {
        return null;
      }
      const category = asString(object.category);
      if (!category) {
        return null;
      }
      return {
        category,
        status: asString(object.status, "unknown"),
        summary: asString(object.summary, "No summary"),
        count: asNumber(object.count),
      };
    })
    .filter((item): item is ExecAssWakeupCheckedCategory => item !== null);
}

function parseAttentionItems(value: unknown): ExecAssWakeupAttentionItem[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .map((item) => {
      const object = asObject(item);
      if (!object) {
        return null;
      }
      const category = asString(object.category);
      const kind = asString(object.kind);
      if (!category && !kind) {
        return null;
      }
      return {
        category: category || "unknown",
        kind: kind || "unknown",
        severity: asString(object.severity, "attention"),
        summary: asString(object.summary, "Attention item found"),
      };
    })
    .filter((item): item is ExecAssWakeupAttentionItem => item !== null);
}

export function summarizeExecAssWakeupAudit(
  runs: JobRunResponse[]
): ExecAssWakeupAuditSummary | null {
  for (const run of runs) {
    const output = parseOutput(run.output_json);
    if (!output || output.mode !== "execass.wakeup") {
      continue;
    }

    const checked = parseChecked(output.checked);
    const attentionItems = parseAttentionItems(output.attention_items);
    const rawOutput = run.output_json ?? "{}";

    return {
      jobRunId: run.job_run_id,
      status: asString(output.status, run.status),
      llmInvoked: output.llm_invoked === true,
      reason: asString(output.reason, run.error_text ?? ""),
      coverageVersion: asString(output.coverage_version, "unknown"),
      checkedCount: checked.length,
      attentionCount: attentionItems.length,
      checked,
      attentionItems,
      rawOutput,
      error: run.error_text,
    };
  }

  return null;
}
