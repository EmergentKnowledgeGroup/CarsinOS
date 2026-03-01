import { useEffect, useCallback, type ReactNode } from "react";
import { X } from "lucide-react";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  /** Optional subtitle below title */
  subtitle?: string;
  children: ReactNode;
  /** Footer actions — rendered below the body */
  footer?: ReactNode;
  /** Width constraint. Default: 520px */
  width?: string;
}

/**
 * Overlay modal with header, scrollable body, and optional footer.
 * Closes on Escape key and backdrop click.
 */
export function Modal({ open, onClose, title, subtitle, children, footer, width }: ModalProps) {
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    },
    [onClose]
  );

  useEffect(() => {
    if (!open) return;
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, handleKeyDown]);

  if (!open) return null;

  return (
    <div className="mc-modal-overlay" onClick={onClose}>
      <div
        className="mc-modal"
        onClick={(e) => e.stopPropagation()}
        style={width ? { width: `min(${width}, calc(100vw - 2rem))` } : undefined}
      >
        <div className="mc-modal-header">
          <div>
            <h2>{title}</h2>
            {subtitle ? <p className="mc-modal-subtitle">{subtitle}</p> : null}
          </div>
          <button type="button" className="mc-topbar-icon-btn" onClick={onClose}>
            <X size={18} />
          </button>
        </div>
        <div className="mc-modal-body">{children}</div>
        {footer ? <div className="mc-modal-actions">{footer}</div> : null}
      </div>
    </div>
  );
}
