import type { OnboardingProviderPath } from "../onboardingState";
import { OnboardingStepShell } from "../OnboardingStepShell";

interface StepRoutingProps {
  busy: boolean;
  providerPath: OnboardingProviderPath;
  selectedAgentId: string;
  providerProfileId: string | null;
  routingReady: boolean;
  onApplyRouting: () => Promise<void>;
  onBack: () => void;
  onNext: () => void;
}

export function StepRouting(props: StepRoutingProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 6 of 8"
      title="Apply Agent Routing"
      subtitle="Set provider profile order so the selected agent uses your target provider."
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button
            type="button"
            className="ghost"
            disabled={props.busy}
            onClick={() => void props.onApplyRouting()}
          >
            {props.busy ? "Applying..." : "Apply Routing"}
          </button>
          <button type="button" disabled={!props.routingReady} onClick={props.onNext}>
            Continue
          </button>
        </>
      }
    >
      <div className="mc-onboarding-summary-card">
        <p>
          Agent: <strong>{props.selectedAgentId || "not selected"}</strong>
        </p>
        <p>
          Provider path: <strong>{props.providerPath}</strong>
        </p>
        <p>
          Profile: <strong>{props.providerProfileId ?? "not required / not selected"}</strong>
        </p>
      </div>
      <p className="mc-onboarding-status-row">
        Routing status: <strong>{props.routingReady ? "Ready" : "Not ready"}</strong>
      </p>
    </OnboardingStepShell>
  );
}
