import clsx from "clsx";
import type { ReactNode } from "react";

interface SurfaceProps {
  className?: string;
  headerClassName?: string;
  title?: ReactNode;
  subtitle?: ReactNode;
  headerRight?: ReactNode;
  children: ReactNode;
}

export function Surface(props: SurfaceProps) {
  const hasHeader =
    props.title != null || props.subtitle != null || props.headerRight != null;
  return (
    <article className={clsx("mc-surface", props.className)}>
      {hasHeader ? (
        <header className={clsx("mc-surface-header", props.headerClassName)}>
          <div>
            {props.title ? <h2>{props.title}</h2> : null}
            {props.subtitle ? <p>{props.subtitle}</p> : null}
          </div>
          {props.headerRight}
        </header>
      ) : null}
      {props.children}
    </article>
  );
}
