import { Wand2 } from "lucide-react";
import {
  COCKPIT_WIDGET_PALETTE,
  type CockpitWidgetKind,
} from "./cockpitLayout";
import { Modal } from "../../ui/Modal";

interface WidgetPickerModalProps {
  open: boolean;
  onClose: () => void;
  onAddWidget: (widget: CockpitWidgetKind) => void;
  onOpenCustomBuilder: () => void;
}

export function WidgetPickerModal({
  open,
  onClose,
  onAddWidget,
  onOpenCustomBuilder,
}: WidgetPickerModalProps) {
  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Add Widget"
      subtitle="Choose a built-in widget or create a custom one."
      width="680px"
    >
      <div className="mc-widget-picker-grid">
        {COCKPIT_WIDGET_PALETTE.map((entry) => (
          <button
            key={entry.widget}
            type="button"
            className="mc-widget-picker-card"
            onClick={() => {
              onAddWidget(entry.widget);
              onClose();
            }}
          >
            <h4>{entry.title}</h4>
            <p>{entry.description}</p>
          </button>
        ))}
        <button
          type="button"
          className="mc-widget-picker-card mc-widget-picker-card-custom"
          onClick={onOpenCustomBuilder}
        >
          <Wand2 size={20} />
          <h4>Custom Widget</h4>
          <p>Query any API endpoint and choose how to display the data.</p>
        </button>
      </div>
    </Modal>
  );
}
