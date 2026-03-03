import { useEffect, useMemo, useState } from "react";

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
  const step = props.steps[props.stepIndex] ?? null;

  useEffect(() => {
    if (!props.open || !step) {
      return;
    }

    const updateRect = () => {
      const element = document.querySelector<HTMLElement>(`[data-tour-id="${step.targetId}"]`);
      if (!element) {
        setTargetRect(null);
        return;
      }
      const rect = element.getBoundingClientRect();
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

  const bubbleStyle = useMemo(() => {
    const panelWidth = 360;
    const gutter = 16;
    const fallbackTop = 110;
    if (!targetRect) {
      return {
        left: clamp((window.innerWidth - panelWidth) / 2, gutter, window.innerWidth - panelWidth - gutter),
        top: fallbackTop,
      };
    }

    const preferredLeft = targetRect.left + targetRect.width / 2 - panelWidth / 2;
    const left = clamp(preferredLeft, gutter, window.innerWidth - panelWidth - gutter);

    const preferredBelow = targetRect.top + targetRect.height + 14;
    const bubbleHeight = 240;
    const fitsBelow = preferredBelow + bubbleHeight < window.innerHeight - gutter;
    const top = fitsBelow
      ? preferredBelow
      : clamp(targetRect.top - bubbleHeight - 14, gutter, window.innerHeight - bubbleHeight - gutter);

    return { left, top };
  }, [targetRect]);

  if (!props.open || !step) {
    return null;
  }

  return (
    <div className="mc-tour-overlay" role="dialog" aria-modal="true" aria-label="Mission Control guided tour">
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
        className="mc-tour-bubble"
        style={{ left: `${bubbleStyle.left}px`, top: `${bubbleStyle.top}px` }}
      >
        <p className="mc-tour-step">Step {props.stepIndex + 1} of {props.steps.length}</p>
        <h3>{step.title}</h3>
        <p>{step.body}</p>
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
