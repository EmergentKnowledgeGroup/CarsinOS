import { useEffect, type RefObject } from "react";

const FOCUSABLE_SELECTOR = [
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "a[href]",
  "summary",
  '[tabindex]:not([tabindex="-1"])',
].join(", ");

function isRenderedFocusable(element: HTMLElement): boolean {
  if (!element.isConnected || element.closest("[hidden], [inert]")) {
    return false;
  }
  for (
    let ancestor = element.parentElement;
    ancestor;
    ancestor = ancestor.parentElement
  ) {
    if (ancestor instanceof HTMLDetailsElement && !ancestor.open) {
      const summary = ancestor.querySelector(":scope > summary");
      if (!summary?.contains(element)) {
        return false;
      }
    }
  }
  const style = window.getComputedStyle(element);
  return (
    style.display !== "none" &&
    style.visibility !== "hidden" &&
    element.getClientRects().length > 0
  );
}

function renderedFocusableControls(container: HTMLElement): HTMLElement[] {
  return Array.from(
    container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
  ).filter(isRenderedFocusable);
}

/**
 * Modal dialog focus behavior: when `active`, move focus into the
 * container, keep Tab cycling inside it, and on close return focus to
 * the exact element that had it when the dialog opened.
 *
 * Escape handling stays with the caller — dialogs differ on what closing
 * means (discard, keep drafts), so this hook never closes anything.
 */
export function useDialogFocus(
  active: boolean,
  containerRef: RefObject<HTMLElement | null>,
  initialFocusRef?: RefObject<HTMLElement | null>,
) {
  useEffect(() => {
    if (!active) {
      return;
    }
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const previouslyFocused =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;

    const rafId = window.requestAnimationFrame(() => {
      const preferred = initialFocusRef?.current;
      if (preferred) {
        preferred.focus();
        return;
      }
      if (container.contains(document.activeElement)) {
        return;
      }
      const firstControl = renderedFocusableControls(container)[0];
      (firstControl ?? container).focus();
    });

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Tab") {
        return;
      }
      const focusable = renderedFocusableControls(container);
      if (focusable.length === 0) {
        event.preventDefault();
        container.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const current = document.activeElement;
      if (event.shiftKey) {
        if (current === first || current === container) {
          event.preventDefault();
          last.focus();
        }
        return;
      }
      if (current === last || !container.contains(current)) {
        event.preventDefault();
        first.focus();
      }
    };
    container.addEventListener("keydown", handleKeyDown, true);

    return () => {
      window.cancelAnimationFrame(rafId);
      container.removeEventListener("keydown", handleKeyDown, true);
      previouslyFocused?.focus();
    };
  }, [active, containerRef, initialFocusRef]);
}
