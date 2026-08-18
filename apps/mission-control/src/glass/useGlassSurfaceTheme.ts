/**
 * Applies the active Glass theme's tokens to one surface element, scoped so
 * the legacy app chrome keeps its own theming. Re-applies on Glass config
 * saves and on system light/dark changes while "Follow system" is active.
 */

import { useEffect, type RefObject } from "react";

import { GLASS_CONFIG_EVENT, loadGlassConfig, resolveActiveTheme } from "./config";
import { applyTheme } from "./themes";

export function useGlassSurfaceTheme(ref: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const media = window.matchMedia?.("(prefers-color-scheme: dark)") ?? null;
    const apply = () => {
      const element = ref.current;
      if (!element) return;
      const theme = resolveActiveTheme(loadGlassConfig(), {
        prefersDark: media?.matches ?? false,
      });
      applyTheme(element, theme);
    };
    apply();
    window.addEventListener(GLASS_CONFIG_EVENT, apply);
    media?.addEventListener?.("change", apply);
    return () => {
      window.removeEventListener(GLASS_CONFIG_EVENT, apply);
      media?.removeEventListener?.("change", apply);
    };
  }, [ref]);
}
