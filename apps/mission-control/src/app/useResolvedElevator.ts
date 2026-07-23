import { useEffect, useMemo, useState } from "react";

import {
  DEFAULT_FLOORS,
  resolveElevator,
  type FloorDef,
} from "../glass/floors";
import {
  GLASS_CONFIG_EVENT,
  loadGlassConfig,
} from "../glass/config";
import type { MissionControlTab } from "./useAppController";

export function useResolvedElevator(
  availableTabs: readonly MissionControlTab[],
): FloorDef[] {
  const [floorOverrides, setFloorOverrides] = useState(
    () => loadGlassConfig().floorOverrides,
  );

  useEffect(() => {
    const sync = () => setFloorOverrides(loadGlassConfig().floorOverrides);
    window.addEventListener(GLASS_CONFIG_EVENT, sync);
    return () => window.removeEventListener(GLASS_CONFIG_EVENT, sync);
  }, []);

  return useMemo(
    () =>
      resolveElevator(DEFAULT_FLOORS, {
        capabilities: ["execass", "agent-mail"],
        overrides: floorOverrides,
      })
        .map((floor) => ({
          ...floor,
          rooms: floor.rooms.filter((room) =>
            availableTabs.includes(room.route),
          ),
        }))
        .filter((floor) => floor.rooms.length > 0),
    [availableTabs, floorOverrides],
  );
}
