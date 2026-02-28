import clsx from "clsx";

interface ChipProps {
  label: string;
  tone?: string;
  className?: string;
}

export function Chip(props: ChipProps) {
  return (
    <span
      className={clsx(
        "chip",
        props.tone ? `chip-${props.tone}` : null,
        props.className
      )}
    >
      {props.label}
    </span>
  );
}
