import type { OnboardingProviderPath } from "../onboardingState";
import { OnboardingStepShell } from "../OnboardingStepShell";

interface StepReviewProps {
  connected: boolean;
  agentReady: boolean;
  providerReady: boolean;
  routingReady: boolean;
  selectedAgentId: string;
  providerPath: OnboardingProviderPath;
  providerProfileId: string | null;
  canFinishReview: boolean;
  onBack: () => void;
  onNext: () => void;
}

export function StepReview(props: StepReviewProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 7 of 8"
      title="Review"
      subtitle="Confirm setup status before finalizing onboarding."
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button type="button" disabled={!props.canFinishReview} onClick={props.onNext}>
            Finalize
          </button>
        </>
      }
    >
      <ul className="mc-onboarding-checklist">
        <li className={props.connected ? "done" : "todo"}>Connection verified</li>
        <li className={props.agentReady ? "done" : "todo"}>Agent ready</li>
        <li className={props.providerReady ? "done" : "todo"}>Provider path ready</li>
        <li className={props.routingReady ? "done" : "todo"}>Routing applied</li>
      </ul>
      <div className="mc-onboarding-summary-card">
        <p>
          Agent: <strong>{props.selectedAgentId || "not selected"}</strong>
        </p>
        <p>
          Provider path: <strong>{props.providerPath}</strong>
        </p>
        <p>
          Profile: <strong>{props.providerProfileId ?? "not required"}</strong>
        </p>
      </div>
    </OnboardingStepShell>
  );
}
