import { create } from "zustand";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import type { Settings, ThemePreference as GeneratedTheme } from "@/types";

export type ThemePreference = GeneratedTheme;

/** Mirrors `Settings::default()` in Rust; replaced by the stored values on load. */
export const DEFAULT_SETTINGS: Settings = {
  startWithOs: false,
  minimizeToTray: false,
  launchMinimized: false,
  rememberWindow: true,
  refreshIntervalMs: 1000,
  historyMinutes: 60,
  theme: "system",
  compactTable: false,
  heatMapping: true,
  visibleColumns: [],
  columnWidths: {},
  showKernelThreads: false,
  showVirtualAdapters: false,
  notificationsEnabled: true,
  alertRules: [],
  cpuSpikePercent: 80,
  memoryJumpMb: 512,
  newProcessSeconds: 10,
  debugLogging: false,
};

const SAVE_DELAY_MS = 300;

interface SettingsState extends Settings {
  loaded: boolean;
  /** Applies a change locally and persists it (debounced). */
  update: (patch: Partial<Settings>) => void;
  /** Replaces local state with settings loaded from the backend. */
  hydrate: (settings: Settings) => void;
  reset: () => Promise<void>;
}

function pick(state: SettingsState): Settings {
  const { loaded, update, hydrate, reset, ...settings } = state;
  return settings;
}

let saveTimer: ReturnType<typeof setTimeout> | undefined;
let lastSaved: Settings = DEFAULT_SETTINGS;

async function persist(): Promise<void> {
  const current = pick(useSettingsStore.getState());
  try {
    lastSaved = await backend.updateSettings(current);
  } catch (e) {
    // Roll back to what the backend last accepted.
    useSettingsStore.setState(lastSaved);
    toast.error(`Settings were not saved: ${toAppError(e).message}`);
  }
}

export const useSettingsStore = create<SettingsState>((set) => ({
  ...DEFAULT_SETTINGS,
  loaded: false,

  update: (patch) => {
    set(patch);
    if (!isBackendAvailable()) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void persist(), SAVE_DELAY_MS);
  },

  hydrate: (settings) => {
    lastSaved = settings;
    set({ ...settings, loaded: true });
  },

  reset: async () => {
    clearTimeout(saveTimer);
    if (!isBackendAvailable()) {
      set(DEFAULT_SETTINGS);
      return;
    }
    try {
      const fresh = await backend.resetSettings();
      lastSaved = fresh;
      set(fresh);
    } catch (e) {
      toast.error(toAppError(e).message);
    }
  },
}));
