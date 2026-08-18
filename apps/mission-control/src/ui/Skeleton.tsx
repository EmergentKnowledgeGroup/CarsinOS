import clsx from "clsx";

interface SkeletonProps {
  /** Width. Default: 100% */
  width?: string;
  /** Height. Default: 1em */
  height?: string;
  /** Border radius. Default: radius-sm */
  variant?: "text" | "circle" | "rect";
  className?: string;
}

/**
 * Loading placeholder with shimmer animation.
 * Use while data is being fetched.
 */
export function Skeleton({ width, height, variant = "text", className }: SkeletonProps) {
  return (
    <div
      className={clsx("mc-skeleton", `mc-skeleton-${variant}`, className)}
      style={{ width, height }}
    />
  );
}
