import { useState, useRef, useEffect, useCallback } from "react";
import { Bell, X, Trash2 } from "lucide-react";
import clsx from "clsx";
import { Badge } from "./Badge";
import { formatRelative } from "../utils/datetime";
import type { NotificationItem } from "./useToasts";

interface NotificationCenterProps {
  notifications: NotificationItem[];
  onDismiss: (id: string) => void;
  onClearAll: () => void;
}

export function NotificationCenter({ notifications, onDismiss, onClearAll }: NotificationCenterProps) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  const toggle = useCallback(() => setOpen((o) => !o), []);

  // Close on click-outside
  useEffect(() => {
    if (!open) return;
    function handleDown(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleDown);
    return () => document.removeEventListener("mousedown", handleDown);
  }, [open]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [open]);

  return (
    <div className="mc-notification-center-wrap" ref={wrapRef}>
      <button
        type="button"
        className="mc-topbar-icon-btn mc-notification-bell"
        onClick={toggle}
        title="Notifications"
      >
        <Bell size={16} />
        <Badge count={notifications.length} tone="warn" className="mc-notification-badge" />
      </button>

      {open && (
        <div className="mc-notification-panel">
          <div className="mc-notification-panel-header">
            <span className="mc-notification-panel-title">Notifications</span>
            {notifications.length > 0 && (
              <button
                type="button"
                className="mc-notification-clear-btn"
                onClick={onClearAll}
                title="Clear all"
              >
                <Trash2 size={14} />
              </button>
            )}
          </div>
          <div className="mc-notification-panel-body">
            {notifications.length === 0 ? (
              <div className="mc-notification-empty">No notifications</div>
            ) : (
              notifications.map((n) => (
                <div key={n.id} className={clsx("mc-notification-row", `mc-notification-row-${n.tone}`)}>
                  <div className="mc-notification-row-content">
                    <span className="mc-notification-row-message">{n.message}</span>
                    <span className="mc-notification-row-time">{formatRelative(n.timestamp)}</span>
                  </div>
                  <button
                    type="button"
                    className="mc-notification-row-dismiss"
                    onClick={() => onDismiss(n.id)}
                    title="Dismiss"
                  >
                    <X size={12} />
                  </button>
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
