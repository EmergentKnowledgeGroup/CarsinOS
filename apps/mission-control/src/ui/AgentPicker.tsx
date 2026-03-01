import clsx from "clsx";
import type { Agent } from "../types";

interface AgentPickerProps {
  agents: Agent[];
  /** Comma-separated list of selected agent IDs */
  value: string;
  onChange: (next: string) => void;
  /** Label shown above the picker */
  label?: string;
}

/**
 * Clickable chip list for multi-selecting agents.
 * Replaces CSV text inputs — "so easy a caveman could do it."
 */
export function AgentPicker({ agents, value, onChange, label }: AgentPickerProps) {
  const selected = new Set(
    value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean)
  );

  const toggle = (agentId: string) => {
    const next = new Set(selected);
    if (next.has(agentId)) {
      next.delete(agentId);
    } else {
      next.add(agentId);
    }
    onChange(Array.from(next).join(", "));
  };

  return (
    <div className="mc-agent-picker">
      {label ? <span className="mc-agent-picker-label">{label}</span> : null}
      <div className="mc-agent-picker-chips">
        {agents.map((agent) => (
          <button
            key={agent.agent_id}
            type="button"
            className={clsx("chip", "mc-agent-chip", selected.has(agent.agent_id) && "mc-agent-chip-selected")}
            onClick={() => toggle(agent.agent_id)}
          >
            {agent.name || agent.agent_id}
          </button>
        ))}
        {agents.length === 0 ? (
          <span className="mc-agent-picker-empty">No agents loaded</span>
        ) : null}
      </div>
    </div>
  );
}
