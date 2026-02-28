import type { Agent } from "../../../types";
import { OnboardingStepShell } from "../OnboardingStepShell";

interface StepAgentProps {
  busy: boolean;
  agents: Agent[];
  selectedAgentId: string;
  agentIdDraft: string;
  agentNameDraft: string;
  workspaceRootDraft: string;
  toolProfileDraft: string;
  agentReady: boolean;
  onSelectedAgentIdChange: (value: string) => void;
  onAgentIdDraftChange: (value: string) => void;
  onAgentNameDraftChange: (value: string) => void;
  onWorkspaceRootDraftChange: (value: string) => void;
  onToolProfileDraftChange: (value: string) => void;
  onEnsureAgent: () => Promise<void>;
  onBack: () => void;
  onNext: () => void;
}

export function StepAgent(props: StepAgentProps) {
  const hasExisting = props.agents.length > 0;
  return (
    <OnboardingStepShell
      stepLabel="Step 4 of 8"
      title="Select or Create Agent"
      subtitle="Every run needs an agent identity and workspace context."
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button type="button" className="ghost" onClick={() => void props.onEnsureAgent()}>
            {props.busy ? "Saving..." : hasExisting ? "Use Selected Agent" : "Create Agent"}
          </button>
          <button type="button" disabled={!props.agentReady} onClick={props.onNext}>
            Continue
          </button>
        </>
      }
    >
      {hasExisting ? (
        <label>
          Existing agent
          <select
            value={props.selectedAgentId}
            onChange={(event) => props.onSelectedAgentIdChange(event.target.value)}
          >
            {props.agents.map((agent) => (
              <option key={agent.agent_id} value={agent.agent_id}>
                {agent.name} ({agent.agent_id})
              </option>
            ))}
          </select>
        </label>
      ) : (
        <div className="mc-onboarding-field-grid">
          <label>
            Agent ID
            <input
              value={props.agentIdDraft}
              onChange={(event) => props.onAgentIdDraftChange(event.target.value)}
              placeholder="lyra"
            />
          </label>
          <label>
            Agent name
            <input
              value={props.agentNameDraft}
              onChange={(event) => props.onAgentNameDraftChange(event.target.value)}
              placeholder="Lyra"
            />
          </label>
          <label>
            Workspace root
            <input
              value={props.workspaceRootDraft}
              onChange={(event) => props.onWorkspaceRootDraftChange(event.target.value)}
              placeholder="."
            />
          </label>
          <label>
            Tool profile
            <input
              value={props.toolProfileDraft}
              onChange={(event) => props.onToolProfileDraftChange(event.target.value)}
              placeholder="default"
            />
          </label>
        </div>
      )}
      <p className="mc-onboarding-status-row">
        Agent status: <strong>{props.agentReady ? "Ready" : "Not ready"}</strong>
      </p>
    </OnboardingStepShell>
  );
}
