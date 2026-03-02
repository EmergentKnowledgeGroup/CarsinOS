import { useState, useCallback, useRef } from "react";
import type { ToastItem } from "./Toast";

export interface NotificationItem extends ToastItem {
  timestamp: number;
}

const NOTIFICATION_CAP = 50;

/** Hook for managing toast state + persistent notification history */
export function useToasts() {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [notifications, setNotifications] = useState<NotificationItem[]>([]);
  const counterRef = useRef(0);

  const addToast = useCallback((message: string, tone: ToastItem["tone"] = "info") => {
    counterRef.current += 1;
    const id = `toast-${counterRef.current}-${Date.now()}`;
    setToasts((prev) => [...prev, { id, message, tone }]);
    setNotifications((prev) =>
      [{ id, message, tone, timestamp: Date.now() }, ...prev].slice(0, NOTIFICATION_CAP),
    );
    return id;
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const dismissNotification = useCallback((id: string) => {
    setNotifications((prev) => prev.filter((n) => n.id !== id));
  }, []);

  const clearAllNotifications = useCallback(() => {
    setNotifications([]);
  }, []);

  return { toasts, addToast, dismissToast, notifications, dismissNotification, clearAllNotifications };
}
