import { OnboardingStepShell } from "../OnboardingStepShell";
import type { OnboardingPreflightState } from "../useOnboardingController";

interface StepPreflightProps {
  preflight: OnboardingPreflightState;
  onRun: () => Promise<void>;
  onNext: () => void;
  onBack: () => void;
}

function renderState(value: boolean | null): string {
  if (value === null) {
    return "pending";
  }
  return value ? "pass" : "fail";
}

export function StepPreflight(props: StepPreflightProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 2 of 8"
      title="Preflight Checks"
      subtitle="Run a quick readiness check before applying setup actions."
      actions={
        <>
          <button type="button" className="ghost" onClick={props.onBack}>
            Back
          </button>
          <button type="button" className="ghost" onClick={() => void props.onRun()}>
            {props.preflight.running ? "Running..." : "Run Checks"}
          </button>
          <button type="button" onClick={props.onNext}>
            Continue
          </button>
        </>
      }
    >
      <div className="mc-onboarding-preflight-grid">
        <div className={`mc-onboarding-preflight-item ${renderState(props.preflight.gatewayReachable)}`}>
          <strong>Gateway reachable</strong>
          <span>{renderState(props.preflight.gatewayReachable)}</span>
        </div>
        <div className={`mc-onboarding-preflight-item ${renderState(props.preflight.authValidated)}`}>
          <strong>Token accepted</strong>
          <span>{renderState(props.preflight.authValidated)}</span>
        </div>
        <div className={`mc-onboarding-preflight-item ${renderState(props.preflight.canReadCore)}`}>
          <strong>Core reads allowed</strong>
          <span>{renderState(props.preflight.canReadCore)}</span>
        </div>
        <div className={`mc-onboarding-preflight-item ${renderState(props.preflight.canManageSetup)}`}>
          <strong>Setup writes allowed</strong>
          <span>{renderState(props.preflight.canManageSetup)}</span>
        </div>
      </div>
      <p className="mc-onboarding-note">{props.preflight.detail}</p>
    </OnboardingStepShell>
  );
}
