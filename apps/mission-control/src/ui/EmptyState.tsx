import clsx from "clsx";

interface EmptyStateProps {
  message: string;
  className?: string;
}

export function EmptyState(props: EmptyStateProps) {
  return <p className={clsx("mc-empty-state", props.className)}>{props.message}</p>;
}
