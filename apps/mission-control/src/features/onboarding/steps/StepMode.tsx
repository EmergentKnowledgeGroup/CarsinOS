import { OnboardingStepShell } from "../OnboardingStepShell";
import type { OnboardingMode } from "../onboardingState";

interface StepModeProps {
  mode: OnboardingMode;
  onModeChange: (value: OnboardingMode) => void;
  onNext: () => void;
}

export function StepMode(props: StepModeProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 1 of 8"
      title="Choose Setup Mode"
      subtitle="Quickstart keeps decisions minimal. Manual exposes advanced fields."
      actions={
        <button type="button" onClick={props.onNext}>
          Continue
        </button>
      }
    >
      <div className="mc-onboarding-choice-grid">
        <label className="mc-onboarding-choice">
          <input
            type="radio"
            name="onboarding-mode"
            value="quickstart"
            checked={props.mode === "quickstart"}
            onChange={() => props.onModeChange("quickstart")}
          />
          <div>
            <strong>Quickstart (Recommended)</strong>
            <p>Fast default path for first-time setup.</p>
          </div>
        </label>
        <label className="mc-onboarding-choice">
          <input
            type="radio"
            name="onboarding-mode"
            value="manual"
            checked={props.mode === "manual"}
            onChange={() => props.onModeChange("manual")}
          />
          <div>
            <strong>Manual</strong>
            <p>Expose advanced connection and provider fields.</p>
          </div>
        </label>
      </div>
    </OnboardingStepShell>
  );
}
