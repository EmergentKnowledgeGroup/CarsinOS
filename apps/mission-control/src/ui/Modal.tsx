import { useEffect, useCallback, useRef, useId, type ReactNode } from "react";
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
 * Implements dialog semantics, focus trap, and focus return.
 */
export function Modal({ open, onClose, title, subtitle, children, footer, width }: ModalProps) {
  const titleId = useId();
  const modalRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<Element | null>(null);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key === "Tab" && modalRef.current) {
        const focusable = modalRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    },
    [onClose]
  );

  useEffect(() => {
    if (!open) return;
    triggerRef.current = document.activeElement;
    document.addEventListener("keydown", handleKeyDown);
    requestAnimationFrame(() => {
      const first = modalRef.current?.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      first?.focus();
    });
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      if (triggerRef.current instanceof HTMLElement) {
        triggerRef.current.focus();
      }
    };
  }, [open, handleKeyDown]);

  if (!open) return null;

  return (
    <div className="mc-modal-overlay" onClick={onClose}>
      <div
        ref={modalRef}
        className="mc-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(e) => e.stopPropagation()}
        style={width ? { width: `min(${width}, calc(100vw - 2rem))` } : undefined}
      >
        <div className="mc-modal-header">
          <div>
            <h2 id={titleId}>{title}</h2>
            {subtitle ? <p className="mc-modal-subtitle">{subtitle}</p> : null}
          </div>
          <button type="button" className="mc-topbar-icon-btn" onClick={onClose} aria-label="Close">
            <X size={18} />
          </button>
        </div>
        <div className="mc-modal-body">{children}</div>
        {footer ? <div className="mc-modal-actions">{footer}</div> : null}
      </div>
    </div>
  );
}
