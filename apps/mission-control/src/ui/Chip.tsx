import clsx from "clsx";

interface ChipProps {
  label: string;
  tone?: string;
  className?: string;
  onClick?: () => void;
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
    >
      {props.label}
    </Tag>
  );
}
