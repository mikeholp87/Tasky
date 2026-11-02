import { create } from "zustand";

export type ThemePreference = "system" | "light" | "dark";

export interface Settings {
  theme: ThemePreference;
  compactTable: boolean;
  heatMapping: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  compactTable: false,
  heatMapping: true,
};

interface SettingsState extends Settings {
  update: (patch: Partial<Settings>) => void;
  reset: () => void;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  ...DEFAULT_SETTINGS,
  update: (patch) => set(patch),
  reset: () => set(DEFAULT_SETTINGS),
}));
