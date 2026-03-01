import { useState, useEffect, useCallback } from "react";

export type ThemeFamily = "obsidian" | "phosphor" | "arctic" | "ember" | "brutalist";
export type ThemeMode = "dark" | "light";
export type ThemeId = `${ThemeFamily}-${ThemeMode}`;

export interface ThemeMeta {
  family: ThemeFamily;
  label: string;
  accent: string;
  description: string;
}

export const THEME_FAMILIES: ThemeMeta[] = [
  { family: "obsidian", label: "Obsidian Ops", accent: "#ff6a13", description: "Professional precision — amber on dark" },
  { family: "phosphor", label: "Phosphor", accent: "#39ff14", description: "Retro terminal — CRT scanlines" },
  { family: "arctic", label: "Arctic", accent: "#60b4ff", description: "Scandinavian calm — ice-blue" },
  { family: "ember", label: "Midnight Ember", accent: "#e0a06e", description: "Luxury warmth — copper glow" },
  { family: "brutalist", label: "Brutalist", accent: "#ff2020", description: "Raw industrial — no compromise" },
];

/** Google Fonts import URLs per theme family */
const THEME_FONT_URLS: Record<ThemeFamily, string> = {
  obsidian: "https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500&family=Outfit:wght@300;400;500;600;700&family=Unbounded:wght@500;600;700&display=swap",
  phosphor: "https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600&family=VT323&display=swap",
  arctic: "https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500&family=Outfit:wght@300;400;500;600;700&display=swap",
  ember: "https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;500;600&family=DM+Serif+Text&family=JetBrains+Mono:wght@400;500&display=swap",
  brutalist: "https://fonts.googleapis.com/css2?family=Archivo:wght@400;600;700&family=Archivo+Black&family=IBM+Plex+Mono:wght@400;500;600&display=swap",
};

function getStoredFamily(): ThemeFamily {
  if (typeof window === "undefined") return "obsidian";
  return (localStorage.getItem("mc-theme-name") as ThemeFamily) || "obsidian";
}

function getStoredMode(): ThemeMode {
  if (typeof window === "undefined") return "dark";
  return (localStorage.getItem("mc-theme-mode") as ThemeMode) || "dark";
}

let activeFontLink: HTMLLinkElement | null = null;

function loadThemeFonts(family: ThemeFamily) {
  const url = THEME_FONT_URLS[family];
  if (!url) return;

  // Don't reload same font
  if (activeFontLink && activeFontLink.getAttribute("href") === url) return;

  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = url;
  link.id = "mc-theme-fonts";

  // Replace existing
  const existing = document.getElementById("mc-theme-fonts");
  if (existing) {
    existing.parentNode?.replaceChild(link, existing);
  } else {
    document.head.appendChild(link);
  }
  activeFontLink = link;
}

function applyTheme(family: ThemeFamily, mode: ThemeMode) {
  const themeId: ThemeId = `${family}-${mode}`;
  document.documentElement.setAttribute("data-theme", themeId);
  localStorage.setItem("mc-theme-name", family);
  localStorage.setItem("mc-theme-mode", mode);
  loadThemeFonts(family);
}

export function useTheme() {
  const [family, setFamily] = useState<ThemeFamily>(getStoredFamily);
  const [mode, setMode] = useState<ThemeMode>(getStoredMode);

  useEffect(() => {
    applyTheme(family, mode);
  }, [family, mode]);

  const toggleMode = useCallback(() => {
    setMode((m) => (m === "dark" ? "light" : "dark"));
  }, []);

  const selectFamily = useCallback((f: ThemeFamily) => {
    setFamily(f);
  }, []);

  return {
    family,
    mode,
    themeId: `${family}-${mode}` as ThemeId,
    toggleMode,
    selectFamily,
    setMode,
  };
}
