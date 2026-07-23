/**
 * The per-room "Pin to Office" affordance. Renders only when the room has
 * registered Office blocks — a room that cannot be pinned yet shows no
 * affordance rather than a dead control. Confirmation appears only after
 * the config save actually succeeded.
 */

import { useState } from "react";

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
  const pinnable =
    found?.room.blocks.some((blockId) =>
      OFFICE_BLOCK_REGISTRY.some((def) => def.id === blockId),
    ) ?? false;
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
