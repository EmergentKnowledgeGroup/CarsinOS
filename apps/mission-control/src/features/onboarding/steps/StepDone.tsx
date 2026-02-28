import { OnboardingStepShell } from "../OnboardingStepShell";

interface StepDoneProps {
  onGoBoards: () => void;
}

export function StepDone(props: StepDoneProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 8 of 8"
      title="Setup Complete"
      subtitle="Mission Control onboarding is complete. You can start operating now."
      actions={
        <button type="button" onClick={props.onGoBoards}>
          Go to Boards
        </button>
      }
    >
      <ul className="mc-onboarding-checklist">
        <li className="done">Connection configured</li>
        <li className="done">Agent selected</li>
        <li className="done">Provider attached</li>
        <li className="done">Routing validated</li>
      </ul>
    </OnboardingStepShell>
  );
}
