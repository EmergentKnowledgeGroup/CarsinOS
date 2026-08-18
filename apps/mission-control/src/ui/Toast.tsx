import { useEffect, useRef } from "react";
import { X } from "lucide-react";
import clsx from "clsx";

export interface ToastItem {
  id: string;
  message: string;
  tone: "info" | "error" | "critical";
}

interface ToastStackProps {
  toasts: ToastItem[];
  onDismiss: (id: string) => void;
}

const AUTO_DISMISS_MS: Record<ToastItem["tone"], number> = {
  info: 4000,
  error: 8000,
  critical: 12000,
};

function ToastEntry({ toast, onDismiss }: { toast: ToastItem; onDismiss: (id: string) => void }) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const duration = AUTO_DISMISS_MS[toast.tone];

  useEffect(() => {
    if (duration <= 0) return;
    timerRef.current = setTimeout(() => onDismiss(toast.id), duration);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [toast.id, duration, onDismiss]);

  return (
    <div
      className={clsx("mc-toast", `mc-toast-${toast.tone}`)}
      role={toast.tone === "info" ? "status" : "alert"}
      aria-live={toast.tone === "info" ? "polite" : "assertive"}
      aria-atomic="true"
    >
      <span className="mc-toast-message">{toast.message}</span>
      <button
        type="button"
        className="mc-toast-dismiss"
        onClick={() => onDismiss(toast.id)}
        aria-label="Dismiss notification"
      >
        <X size={14} />
      </button>
      {duration > 0 ? (
        <div className="mc-toast-progress" style={{ animationDuration: `${duration}ms` }} />
      ) : null}
    </div>
  );
}

/**
 * Toast notification stack. Renders in top-right, max 4 visible.
 * Auto-dismisses info (4s), error (8s), and critical (12s).
 */
export function ToastStack({ toasts, onDismiss }: ToastStackProps) {
  const visible = toasts.slice(0, 4);
  if (visible.length === 0) return null;

  return (
    <div className="mc-toast-stack" aria-label="Notifications">
      {visible.map((toast) => (
        <ToastEntry key={toast.id} toast={toast} onDismiss={onDismiss} />
      ))}
    </div>
  );
}
