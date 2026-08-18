import { useEffect, useMemo, useRef, useState } from "react";

export interface GuidedTourStep {
  id: string;
  targetId: string;
  title: string;
  body: string;
}

interface GuidedTourOverlayProps {
  open: boolean;
  steps: GuidedTourStep[];
  stepIndex: number;
  onClose: () => void;
  onPrev: () => void;
  onNext: () => void;
}

interface RectLike {
  top: number;
  left: number;
  width: number;
  height: number;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function GuidedTourOverlay(props: GuidedTourOverlayProps) {
  const [targetRect, setTargetRect] = useState<RectLike | null>(null);
  const [targetFound, setTargetFound] = useState(true);
  const bubbleRef = useRef<HTMLElement | null>(null);
  const previouslyFocusedRef = useRef<HTMLElement | null>(null);
  const step = props.steps[props.stepIndex] ?? null;
  const progressLabel = `${props.stepIndex + 1}/${props.steps.length}`;
  const progressPercent = ((props.stepIndex + 1) / Math.max(props.steps.length, 1)) * 100;

  useEffect(() => {
    if (!props.open || !step) {
      return;
    }

    const updateRect = () => {
      const element = document.querySelector<HTMLElement>(`[data-tour-id="${step.targetId}"]`);
      const rect = element?.getBoundingClientRect();
      // A hidden target (display:none, collapsed) reports a zero rect; a
      // spotlight there would be a lie, so treat it like a missing stop.
      if (!rect || (rect.width === 0 && rect.height === 0)) {
        setTargetRect(null);
        setTargetFound(false);
        return;
      }
      setTargetFound(true);
      setTargetRect({
        top: rect.top,
        left: rect.left,
        width: rect.width,
        height: rect.height,
      });
    };

    updateRect();
    const onWindowChange = () => {
      window.requestAnimationFrame(updateRect);
    };
    window.addEventListener("resize", onWindowChange);
    window.addEventListener("scroll", onWindowChange, true);

    return () => {
      window.removeEventListener("resize", onWindowChange);
      window.removeEventListener("scroll", onWindowChange, true);
    };
  }, [props.open, step]);

  useEffect(() => {
    if (!props.open) {
      return;
    }
    previouslyFocusedRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const rafId = window.requestAnimationFrame(() => {
      bubbleRef.current?.focus();
    });
    return () => {
      window.cancelAnimationFrame(rafId);
      const canReceiveFocus = (element: HTMLElement | null): element is HTMLElement =>
        element !== null &&
        element.isConnected &&
        element.getClientRects().length > 0;
      // The tour walks between rooms, so a launcher that lived in room
      // content may have unmounted; land on the equivalent launcher that
      // exists now instead of dropping focus on <body>.
      const fallbacks = [
        previouslyFocusedRef.current,
        document.querySelector<HTMLElement>('[data-tour-id="topbar-tour"]'),
        document.querySelector<HTMLElement>(".mc-tab-help-btn-tour"),
        document.querySelector<HTMLElement>('[data-tour-id="nav-help-shortcut"]'),
      ];
      previouslyFocusedRef.current = null;
      fallbacks.find(canReceiveFocus)?.focus();
    };
  }, [props.open]);

  const bubbleStyle = useMemo(() => {
    const panelWidth = 360;
    const bubbleHeight = 240;
    const sideGap = 14;
    const gutter = 16;
    const fallbackTop = 110;
    const maxLeft = Math.max(gutter, window.innerWidth - panelWidth - gutter);
    const maxTop = Math.max(gutter, window.innerHeight - bubbleHeight - gutter);

    if (!targetRect) {
      return {
        left: clamp((window.innerWidth - panelWidth) / 2, gutter, maxLeft),
        top: fallbackTop,
      };
    }

    const verticalCenterTop = clamp(
      targetRect.top + targetRect.height / 2 - bubbleHeight / 2,
      gutter,
      maxTop,
    );
    const preferredRight = targetRect.left + targetRect.width + sideGap;
    if (preferredRight <= maxLeft) {
      return { left: preferredRight, top: verticalCenterTop };
    }

    const preferredLeft = targetRect.left - panelWidth - sideGap;
    if (preferredLeft >= gutter) {
      return { left: preferredLeft, top: verticalCenterTop };
    }

    // Small viewports: keep side-oriented behavior by nudging into the visible bounds.
    return {
      left: clamp(preferredRight, gutter, maxLeft),
      top: verticalCenterTop,
    };
  }, [targetRect]);

  if (!props.open || !step) {
    return null;
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      props.onClose();
      return;
    }
    if (event.key !== "Tab") {
      return;
    }
    const bubble = bubbleRef.current;
    if (!bubble) {
      return;
    }
    const focusable = Array.from(
      bubble.querySelectorAll<HTMLElement>("button:not([disabled])"),
    );
    if (focusable.length === 0) {
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (event.shiftKey) {
      if (active === first || active === bubble) {
        event.preventDefault();
        last.focus();
      }
      return;
    }
    if (active === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <div
      className="mc-tour-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Mission Control guided tour"
      onKeyDown={handleKeyDown}
    >
      <div className="mc-tour-scrim" />
      {targetRect ? (
        <div
          className="mc-tour-highlight"
          style={{
            top: `${targetRect.top - 6}px`,
            left: `${targetRect.left - 6}px`,
            width: `${targetRect.width + 12}px`,
            height: `${targetRect.height + 12}px`,
          }}
        />
      ) : null}
      <section
        ref={bubbleRef}
        className="mc-tour-bubble"
        style={{ left: `${bubbleStyle.left}px`, top: `${bubbleStyle.top}px` }}
        tabIndex={-1}
        aria-live="polite"
      >
        <div className="mc-tour-header">
          <div className="mc-tour-progress-block">
            <span
              className="mc-tour-progress-chip"
              aria-label={`Guided tour step ${props.stepIndex + 1} of ${props.steps.length}`}
            >
              {progressLabel}
            </span>
            <p className="mc-tour-step">Guided tour</p>
          </div>
          <div className="mc-tour-progress-track" aria-hidden="true">
            <span
              className="mc-tour-progress-fill"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        </div>
        <h3>{step.title}</h3>
        <p>{step.body}</p>
        {!targetFound ? (
          <p className="mc-tour-missing" role="status">
            This stop isn't visible right now — it may be off or hidden on
            this screen. Use Next to keep going or End Tour to leave.
          </p>
        ) : null}
        <div className="mc-tour-actions">
          <button type="button" className="ghost" onClick={props.onClose}>
            End Tour
          </button>
          <div className="mc-tour-actions-right">
            <button type="button" className="ghost" onClick={props.onPrev} disabled={props.stepIndex === 0}>
              Back
            </button>
            <button type="button" onClick={props.onNext}>
              {props.stepIndex + 1 >= props.steps.length ? "Finish" : "Next"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
