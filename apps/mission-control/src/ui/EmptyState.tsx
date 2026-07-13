import clsx from "clsx";
import type { ReactNode } from "react";

interface EmptyStateProps {
  message: string;
  className?: string;
  title?: string;
  icon?: ReactNode;
  action?: ReactNode;
}

export function EmptyState(props: EmptyStateProps) {
  if (!props.title && !props.icon && !props.action) {
    return <p className={clsx("mc-empty-state", props.className)}>{props.message}</p>;
  }
  return (
    <div className={clsx("mc-empty-state mc-empty-state-rich", props.className)}>
      {props.icon ? <div className="mc-empty-state-icon" aria-hidden="true">{props.icon}</div> : null}
      {props.title ? <strong>{props.title}</strong> : null}
      <p>{props.message}</p>
      {props.action ? <div className="mc-empty-state-action">{props.action}</div> : null}
    </div>
  );
}
