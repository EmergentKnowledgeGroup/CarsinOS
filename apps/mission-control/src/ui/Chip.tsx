import clsx from "clsx";

interface ChipProps {
  label: string;
  tone?: string;
  className?: string;
  title?: string;
  onClick?: () => void;
  /** For toggle chips — conveys pressed state to assistive tech */
  ariaPressed?: boolean;
}

export function Chip(props: ChipProps) {
  const className = clsx(
    "chip",
    props.tone ? `chip-${props.tone}` : null,
    props.onClick ? "chip-clickable" : null,
    props.className
  );

  if (props.onClick) {
    return (
      <button
        type="button"
        className={className}
        onClick={props.onClick}
        title={props.title}
        aria-pressed={props.ariaPressed}
      >
        {props.label}
      </button>
    );
  }

  return (
    <span className={className} title={props.title}>
      {props.label}
    </span>
  );
}
