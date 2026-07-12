import type { ReactNode } from "react";

interface ScrollRegionProps {
  "aria-label": string;
  children: ReactNode;
  className?: string;
}

/**
 * A bounded, keyboard-focusable region for long streams. Users can tab into
 * the region and use the standard browser scroll keys without moving focus
 * through every item in the stream.
 */
export function ScrollRegion({
  children,
  className = "",
  "aria-label": ariaLabel,
}: ScrollRegionProps) {
  return (
    <div
      className={`mc-scroll-region${className ? ` ${className}` : ""}`}
      role="region"
      aria-label={ariaLabel}
      tabIndex={0}
    >
      {children}
    </div>
  );
}
