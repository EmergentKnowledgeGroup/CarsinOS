import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";
import {
  buildErrorReport,
  nextCrashWindow,
  shouldEnterSafeMode,
  type CrashWindowState,
  type ErrorEventContext,
} from "../lib/errorRecovery";

const SAFE_MODE_CRASH_THRESHOLD = 3;
const SAFE_MODE_WINDOW_MS = 15_000;

interface AppErrorBoundaryProps {
  scope: "global" | "tab";
  title: string;
  subtitle: string;
  events?: readonly ErrorEventContext[];
  onResetScope?: () => void;
  onEnterSafeMode?: (reason: string) => void;
  children: ReactNode;
}

interface AppErrorBoundaryState {
  error: Error | null;
  componentStack: string | null;
  crashWindow: CrashWindowState;
  copyFeedback: "idle" | "ok" | "error";
}

function formatErrorText(
  props: AppErrorBoundaryProps,
  state: AppErrorBoundaryState
): string {
  if (!state.error) {
    return "";
  }
  return buildErrorReport(
    `${props.scope.toUpperCase()} boundary crash`,
    state.error,
    state.componentStack,
    props.events ?? []
  );
}

export class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = {
    error: null,
    componentStack: null,
    crashWindow: {
      windowStartMs: Date.now(),
      crashCount: 0,
    },
    copyFeedback: "idle",
  };

  static getDerivedStateFromError(error: Error): Partial<AppErrorBoundaryState> {
    return {
      error,
      copyFeedback: "idle",
    };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    const nowMs = Date.now();
    this.setState((previous) => {
      const nextWindow = nextCrashWindow(previous.crashWindow, nowMs, SAFE_MODE_WINDOW_MS);
      if (
        this.props.scope === "tab" &&
        shouldEnterSafeMode(nextWindow.crashCount, SAFE_MODE_CRASH_THRESHOLD)
      ) {
        this.props.onEnterSafeMode?.(
          `${this.props.title} kept crashing ${nextWindow.crashCount} times in ${SAFE_MODE_WINDOW_MS / 1000}s.`
        );
      }
      return {
        error,
        componentStack: info.componentStack || null,
        crashWindow: nextWindow,
      };
    });
  }

  private retry = () => {
    this.setState({
      error: null,
      componentStack: null,
      copyFeedback: "idle",
    });
  };

  private resetScope = () => {
    this.props.onResetScope?.();
    this.retry();
  };

  private reloadApp = () => {
    window.location.reload();
  };

  private copyErrorDetails = async () => {
    if (!this.state.error) {
      return;
    }

    try {
      const report = formatErrorText(this.props, this.state);
      await navigator.clipboard.writeText(report);
      this.setState({ copyFeedback: "ok" });
    } catch {
      this.setState({ copyFeedback: "error" });
    }
  };

  render(): ReactNode {
    if (!this.state.error) {
      return this.props.children;
    }

    const report = formatErrorText(this.props, this.state);
    const eventRows = (this.props.events ?? []).slice(0, 10);
    const showResetAction =
      this.props.scope === "tab" || (this.props.scope === "global" && Boolean(this.props.onResetScope));

    return (
      <section className="mc-crash-shell" role="alert">
        <div className="mc-crash-card">
          <p className="mc-crash-label">Crash Recovery</p>
          <h2>{this.props.title}</h2>
          <p>{this.props.subtitle}</p>

          <div className="mc-crash-actions">
            <button type="button" className="ghost" onClick={this.retry}>
              Retry
            </button>
            {showResetAction ? (
              <button type="button" className="ghost" onClick={this.resetScope}>
                {this.props.scope === "tab" ? "Reset tab state" : "Reset app state"}
              </button>
            ) : null}
            <button type="button" className="ghost" onClick={this.reloadApp}>
              Reload app
            </button>
            <button type="button" className="ghost" onClick={() => void this.copyErrorDetails()}>
              Copy error details
            </button>
          </div>

          {this.state.copyFeedback === "ok" ? (
            <p className="mc-crash-feedback">Error details copied.</p>
          ) : null}
          {this.state.copyFeedback === "error" ? (
            <p className="mc-crash-feedback">Clipboard unavailable. Expand details and copy manually.</p>
          ) : null}

          <details>
            <summary>Show last events ({eventRows.length})</summary>
            <ul className="mc-crash-events">
              {eventRows.length > 0 ? (
                eventRows.map((event) => (
                  <li key={event.event_id}>
                    <code>{event.ts_unix_ms}</code> {event.event_type} ({event.entity})
                  </li>
                ))
              ) : (
                <li>No events captured.</li>
              )}
            </ul>
          </details>

          <details>
            <summary>Error details</summary>
            <pre className="mc-crash-details">{report}</pre>
          </details>
        </div>
      </section>
    );
  }
}
