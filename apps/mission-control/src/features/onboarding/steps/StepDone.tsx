import { OnboardingStepShell } from "../OnboardingStepShell";

interface StepDoneAction {
  id: string;
  label: string;
  description: string;
  onClick: () => void;
}

interface StepDoneProps {
  actions: StepDoneAction[];
}

export function StepDone(props: StepDoneProps) {
  return (
    <OnboardingStepShell
      stepLabel="Step 6 of 6"
      title="Setup Complete"
      subtitle="You are ready to use Mission Control. Pick the next thing you want to do so you are not dropped into the app cold."
    >
      <ul className="mc-onboarding-checklist">
        <li className="done">Connection configured</li>
        <li className="done">Agent selected</li>
        <li className="done">Provider attached</li>
        <li className="done">Routing validated</li>
      </ul>

      <div className="mc-onboarding-next-actions">
        {props.actions.map((action) => (
          <button
            key={action.id}
            type="button"
            className="mc-onboarding-next-card"
            onClick={action.onClick}
          >
            <strong>{action.label}</strong>
            <span>{action.description}</span>
          </button>
        ))}
      </div>
    </OnboardingStepShell>
  );
}
