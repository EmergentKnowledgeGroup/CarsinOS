import { useState } from "react";
import clsx from "clsx";

interface TagPickerProps {
  /** Comma-separated list of selected tags */
  value: string;
  onChange: (next: string) => void;
  /** Known tags to suggest (aggregated from existing cards) */
  suggestions: string[];
  label?: string;
  disabled?: boolean;
}

/**
 * Multi-select tag picker with chip toggles and an "add new" input.
 * Replaces CSV text input for board card tags.
 */
export function TagPicker({
  value,
  onChange,
  suggestions,
  label,
  disabled = false,
}: TagPickerProps) {
  const [newTag, setNewTag] = useState("");

  const selected = new Set(
    value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean)
  );

  const toggle = (tag: string) => {
    if (disabled) {
      return;
    }
    const next = new Set(selected);
    if (next.has(tag)) {
      next.delete(tag);
    } else {
      next.add(tag);
    }
    onChange(Array.from(next).join(", "));
  };

  const addNewTag = () => {
    if (disabled) {
      return;
    }
    const trimmed = newTag.trim();
    if (!trimmed) return;
    const next = new Set(selected);
    next.add(trimmed);
    onChange(Array.from(next).join(", "));
    setNewTag("");
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addNewTag();
    }
  };

  // Merge suggestions with already-selected tags to show everything
  const allTags = Array.from(new Set([...suggestions, ...selected]));

  return (
    <div className="mc-agent-picker">
      {label ? <span className="mc-agent-picker-label">{label}</span> : null}
      <div className="mc-agent-picker-chips">
        {allTags.map((tag) => (
          <button
            key={tag}
            type="button"
            className={clsx("chip", "mc-agent-chip", selected.has(tag) && "mc-agent-chip-selected")}
            disabled={disabled}
            onClick={() => toggle(tag)}
          >
            {tag}
          </button>
        ))}
      </div>
      <div className="mc-tag-add-row">
        <input
          value={newTag}
          disabled={disabled}
          onChange={(e) => setNewTag(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Add new tag..."
          className="mc-tag-add-input"
        />
        <button type="button" onClick={addNewTag} disabled={disabled || !newTag.trim()}>
          Add
        </button>
      </div>
    </div>
  );
}
