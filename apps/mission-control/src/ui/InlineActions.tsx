import clsx from "clsx";
import type { ReactNode } from "react";

interface InlineActionsProps {
  className?: string;
  children: ReactNode;
}

export function InlineActions(props: InlineActionsProps) {
  return <div className={clsx("mc-inline-actions", props.className)}>{props.children}</div>;
}
