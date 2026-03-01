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
  critical: 0, // manual dismiss only
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
    <div className={clsx("mc-toast", `mc-toast-${toast.tone}`)}>
      <span className="mc-toast-message">{toast.message}</span>
      <button type="button" className="mc-toast-dismiss" onClick={() => onDismiss(toast.id)}>
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
 * Auto-dismisses info (4s) and error (8s). Critical requires manual dismiss.
 */
export function ToastStack({ toasts, onDismiss }: ToastStackProps) {
  const visible = toasts.slice(0, 4);
  if (visible.length === 0) return null;

  return (
    <div className="mc-toast-stack">
      {visible.map((toast) => (
        <ToastEntry key={toast.id} toast={toast} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

