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
  const Tag = props.onClick ? "button" : "span";
  return (
    <Tag
      type={props.onClick ? "button" : undefined}
      className={clsx(
        "chip",
        props.tone ? `chip-${props.tone}` : null,
        props.onClick ? "chip-clickable" : null,
        props.className
      )}
      onClick={props.onClick}
      title={props.title}
      aria-pressed={props.ariaPressed}
    >
      {props.label}
    </Tag>
  );
}
