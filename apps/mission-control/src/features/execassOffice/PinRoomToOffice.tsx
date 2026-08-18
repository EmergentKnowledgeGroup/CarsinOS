/**
 * The per-room "Pin to Office" affordance. Renders only when the room has
 * registered Office blocks — a room that cannot be pinned yet shows no
 * affordance rather than a dead control. Confirmation appears only after
 * the config save actually succeeded.
 */

import { useEffect, useState } from "react";

import { normalizeLayout } from "../../glass/blocks";
import {
  GLASS_CONFIG_EVENT,
  loadGlassConfig,
} from "../../glass/config";
import { DEFAULT_FLOORS, findRoom } from "../../glass/floors";
import { OFFICE_BLOCK_REGISTRY } from "./officeBlocks";
import { pinRoomBlocksToOffice } from "./pinToOffice";

type PinState =
  | { kind: "idle" }
  | { kind: "pinned" }
  | { kind: "already" }
  | { kind: "error"; message: string };

export function PinRoomToOffice(props: { roomId: string }) {
  const [state, setState] = useState<PinState>({ kind: "idle" });
  const found = findRoom(DEFAULT_FLOORS, props.roomId);
  const room = found?.room;
  const pinnable =
    room?.blocks.some((blockId) =>
      OFFICE_BLOCK_REGISTRY.some((def) => def.id === blockId),
    ) ?? false;

  useEffect(() => {
    const clearStaleFeedback = () => {
      if (!room) return;
      const layout = normalizeLayout(
        loadGlassConfig().layout,
        OFFICE_BLOCK_REGISTRY,
      );
      const visible = layout.some(
        (placement) =>
          room.blocks.includes(placement.id) && placement.visible,
      );
      if (!visible) setState({ kind: "idle" });
    };
    window.addEventListener(GLASS_CONFIG_EVENT, clearStaleFeedback);
    return () =>
      window.removeEventListener(GLASS_CONFIG_EVENT, clearStaleFeedback);
  }, [room]);

  if (!found || !pinnable) return null;

  const handlePin = () => {
    const result = pinRoomBlocksToOffice(props.roomId);
    if (!result.ok) {
      setState({
        kind: "error",
        message: result.error ?? "The pin could not be saved.",
      });
      return;
    }
    setState(result.pinned.length > 0 ? { kind: "pinned" } : { kind: "already" });
  };

  return (
    <div className="mc-pin-to-office">
      <button
        type="button"
        className="mc-pin-to-office-button"
        aria-label={`Pin ${found.room.label} to Office`}
        onClick={handlePin}
      >
        Pin to Office
      </button>
      {state.kind === "pinned" || state.kind === "already" ? (
        <span className="mc-pin-to-office-note" role="status">
          {state.kind === "pinned"
            ? "On the Office canvas."
            : "Already on the Office canvas."}
        </span>
      ) : null}
      {state.kind === "error" ? (
        <span className="mc-pin-to-office-note is-error" role="alert">
          {state.message}
        </span>
      ) : null}
    </div>
  );
}
