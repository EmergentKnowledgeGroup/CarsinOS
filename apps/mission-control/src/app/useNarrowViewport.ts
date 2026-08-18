import { useEffect, useState } from "react";

/**
 * Tracks a max-width media query. Safe where matchMedia is absent
 * (jsdom without a stub): reports false and never subscribes.
 */
export function useNarrowViewport(maxWidthPx: number): boolean {
  const query = `(max-width: ${maxWidthPx}px)`;
  const [narrow, setNarrow] = useState(
    () => window.matchMedia?.(query).matches ?? false,
  );
  useEffect(() => {
    const media = window.matchMedia?.(query);
    if (!media) return;
    const update = () => setNarrow(media.matches);
    update();
    media.addEventListener?.("change", update);
    return () => media.removeEventListener?.("change", update);
  }, [query]);
  return narrow;
}
