import { useState, useCallback, useRef } from "react";
import type { ToastItem } from "./Toast";

/** Hook for managing toast state */
export function useToasts() {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const counterRef = useRef(0);

  const addToast = useCallback((message: string, tone: ToastItem["tone"] = "info") => {
    counterRef.current += 1;
    const id = `toast-${counterRef.current}-${Date.now()}`;
    setToasts((prev) => [...prev, { id, message, tone }]);
    return id;
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return { toasts, addToast, dismissToast };
}
