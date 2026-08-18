import type { ButtonHTMLAttributes, ReactNode } from "react";
import clsx from "clsx";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  busy?: boolean;
  busyLabel?: string;
  icon?: ReactNode;
}

export function Button({
  variant = "secondary",
  busy = false,
  busyLabel = "Working…",
  icon,
  children,
  className,
  disabled,
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      {...props}
      type={type}
      className={clsx(variant, className)}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
    >
      {icon ? <span className="mc-button-icon" aria-hidden="true">{icon}</span> : null}
      <span>{busy ? busyLabel : children}</span>
    </button>
  );
}
