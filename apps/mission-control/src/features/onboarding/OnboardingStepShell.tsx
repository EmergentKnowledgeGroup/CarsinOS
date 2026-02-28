import type { ReactNode } from "react";

interface OnboardingStepShellProps {
  title: string;
  subtitle: string;
  stepLabel: string;
  children: ReactNode;
  actions?: ReactNode;
}

export function OnboardingStepShell(props: OnboardingStepShellProps) {
  return (
    <section className="mc-onboarding-step">
      <header className="mc-onboarding-step-header">
        <p className="mc-onboarding-step-label">{props.stepLabel}</p>
        <h3>{props.title}</h3>
        <p>{props.subtitle}</p>
      </header>
      <div className="mc-onboarding-step-body">{props.children}</div>
      {props.actions ? <footer className="mc-onboarding-step-actions">{props.actions}</footer> : null}
    </section>
  );
}
