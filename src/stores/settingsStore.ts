import { create } from "zustand";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import type { BenchmarkSettings, Settings, ThemePreference as GeneratedTheme } from "@/types";

export type ThemePreference = GeneratedTheme;

/** Mirrors `BenchmarkSettings::default()` in Rust. */
export const DEFAULT_BENCHMARK_SETTINGS: BenchmarkSettings = {
  profile: "standard",
  cpuWorkers: null,
  gpuAdapterId: null,
  diskMountPoint: null,
  diskAllowWrite: true,
  diskWriteLimit: "standard",
  networkProvider: "cloudflare",
  networkDataProfile: "standard",
  networkPrivacyAccepted: false,
  networkShareResults: false,
  saveHistory: true,
  cooldown: true,
  sustainedSecs: 60,
  showRawMeasurements: false,
  showMethodology: true,
  debugLogging: false,
};

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
  benchmark: DEFAULT_BENCHMARK_SETTINGS,
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
/** Counts local edits, so a failed save can tell whether newer edits exist. */
let edits = 0;
let queue: Promise<void> = Promise.resolve();
let resetGeneration = 0;

async function save(): Promise<void> {
  const sent = edits;
  const current = pick(useSettingsStore.getState());
  try {
    lastSaved = await backend.updateSettings(current);
  } catch (e) {
    // Roll back to what the backend last accepted, unless a newer edit is
    // already on its way (its own save will report and roll back if needed).
    if (edits === sent) useSettingsStore.setState(lastSaved);
    toast.error(`Settings were not saved: ${toAppError(e).message}`);
  }
}

/** One save at a time, in order, so a slow earlier save cannot land last. */
function persist(): Promise<void> {
  const generation = resetGeneration;
  queue = queue.then(() => (generation === resetGeneration ? save() : undefined));
  return queue;
}

/** Saves a pending edit now (the window is closing or hiding). */
export function flushSettings(): void {
  if (saveTimer === undefined) return;
  clearTimeout(saveTimer);
  saveTimer = undefined;
  void persist();
}

if (typeof window !== "undefined") {
  window.addEventListener("pagehide", flushSettings);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") flushSettings();
  });
}

export const useSettingsStore = create<SettingsState>((set) => ({
  ...DEFAULT_SETTINGS,
  loaded: false,

  update: (patch) => {
    set(patch);
    if (!isBackendAvailable()) return;
    edits += 1;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      saveTimer = undefined;
      void persist();
    }, SAVE_DELAY_MS);
  },

  hydrate: (settings) => {
    lastSaved = settings;
    set({ ...settings, loaded: true });
  },

  reset: async () => {
    clearTimeout(saveTimer);
    saveTimer = undefined;
    const sent = ++edits;
    resetGeneration += 1;
    set(DEFAULT_SETTINGS);
    if (!isBackendAvailable()) {
      set(DEFAULT_SETTINGS);
      return;
    }
    queue = queue.then(async () => {
      try {
        const fresh = await backend.resetSettings();
        lastSaved = fresh;
        if (edits === sent) set(fresh);
      } catch (e) {
        if (edits === sent) set(lastSaved);
        toast.error(toAppError(e).message);
      }
    });
    await queue;
  },
}));
