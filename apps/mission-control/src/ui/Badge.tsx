import clsx from "clsx";

interface BadgeProps {
  /** Number to display. Hidden when 0. */
  count: number;
  /** Color tone */
  tone?: "accent" | "danger" | "warn" | "ok" | "info";
  className?: string;
}

/**
 * Notification count badge. Rendered as a small colored pill.
 * Shows nothing when count is 0.
 */
export function Badge({ count, tone = "accent", className }: BadgeProps) {
  if (count <= 0) return null;

  return (
    <span className={clsx("mc-badge", `mc-badge-${tone}`, className)}>
      {count > 99 ? "99+" : count}
    </span>
  );
}
