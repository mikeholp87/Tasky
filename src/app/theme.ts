import { useEffect } from "react";
import { useSettingsStore, type ThemePreference } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";

const DARK_QUERY = "(prefers-color-scheme: dark)";

export function resolveTheme(pref: ThemePreference, systemDark: boolean): "light" | "dark" {
  if (pref === "system") return systemDark ? "dark" : "light";
  return pref;
}

/** Applies the theme preference to <html> and follows OS changes in "system" mode. */
export function useApplyTheme(): void {
  const pref = useSettingsStore((s) => s.theme);
  const setResolved = useUiStore((s) => s.setResolvedTheme);
  useEffect(() => {
    const mql = window.matchMedia(DARK_QUERY);
    const apply = () => {
      const theme = resolveTheme(pref, mql.matches);
      document.documentElement.classList.toggle("dark", theme === "dark");
      setResolved(theme);
    };
    apply();
    if (pref !== "system") return;
    mql.addEventListener("change", apply);
    return () => mql.removeEventListener("change", apply);
  }, [pref, setResolved]);
}
