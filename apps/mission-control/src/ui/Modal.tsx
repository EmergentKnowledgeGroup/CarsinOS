import { useEffect, useCallback, useRef, useId, type ReactNode } from "react";
import { X } from "lucide-react";

function isTabbable(element: HTMLElement): boolean {
  if (element.tabIndex < 0) {
    return false;
  }
  if ("disabled" in element && element.disabled) {
    return false;
  }
  if (
    element.hasAttribute("disabled") ||
    element.hasAttribute("hidden") ||
    element.hasAttribute("inert") ||
    element.getAttribute("aria-hidden") === "true"
  ) {
    return false;
  }
  if (element instanceof HTMLInputElement && element.type === "hidden") {
    return false;
  }
  const style = window.getComputedStyle(element);
  if (style.display === "none" || style.visibility === "hidden") {
    return false;
  }
  return element.getClientRects().length > 0;
}

function getTabbableElements(root: HTMLElement | null): HTMLElement[] {
  if (!root) {
    return [];
  }
  return Array.from(
    root.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    )
  ).filter(isTabbable);
}

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
  const subtitleId = useId();
  const modalRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<Element | null>(null);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key === "Tab" && modalRef.current) {
        const focusable = getTabbableElements(modalRef.current);
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
      const modal = modalRef.current;
      if (!modal) {
        return;
      }
      const activeElement =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      if (activeElement && modal.contains(activeElement)) {
        return;
      }
      const autofocusTarget = modal.querySelector<HTMLElement>("[autofocus]");
      if (autofocusTarget && isTabbable(autofocusTarget)) {
        autofocusTarget.focus();
        return;
      }
      const [first] = getTabbableElements(modal);
      if (first) {
        first.focus();
      } else {
        modal.focus();
      }
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
        aria-describedby={subtitle ? subtitleId : undefined}
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        style={width ? { width: `min(${width}, calc(100vw - 2rem))` } : undefined}
      >
        <div className="mc-modal-header">
          <div>
            <h2 id={titleId}>{title}</h2>
            {subtitle ? (
              <p id={subtitleId} className="mc-modal-subtitle">
                {subtitle}
              </p>
            ) : null}
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
