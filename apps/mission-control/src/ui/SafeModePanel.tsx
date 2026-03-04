interface SafeModePanelProps {
  reason: string;
  onResume: () => void;
}

export function SafeModePanel(props: SafeModePanelProps) {
  return (
    <section className="mc-crash-shell" role="alert">
      <div className="mc-crash-card mc-safe-mode-card">
        <p className="mc-crash-label">Safe Mode</p>
        <h2>Mission Control entered safe mode.</h2>
        <p>{props.reason}</p>
        <p>
          This means repeated crashes were detected. You can try to recover in-place or reload the
          app.
        </p>
        <div className="mc-crash-actions">
          <button type="button" className="ghost" onClick={props.onResume}>
            Retry recovery
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => {
              window.location.reload();
            }}
          >
            Reload app
          </button>
        </div>
      </div>
    </section>
  );
}
