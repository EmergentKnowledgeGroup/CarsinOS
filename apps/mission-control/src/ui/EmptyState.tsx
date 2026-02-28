import clsx from "clsx";

interface EmptyStateProps {
  message: string;
  className?: string;
}

export function EmptyState(props: EmptyStateProps) {
  return <p className={clsx("mc-empty-events", props.className)}>{props.message}</p>;
}
