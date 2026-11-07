import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/types";
import { DEFAULT_SETTINGS, flushSettings, useSettingsStore } from "./settingsStore";

const updateSettings = vi.fn<(s: Settings) => Promise<Settings>>();
const resetSettings = vi.fn<() => Promise<Settings>>();

vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: {
    updateSettings: (s: Settings) => updateSettings(s),
    resetSettings: () => resetSettings(),
  },
}));

describe("settings store saving", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    updateSettings.mockReset();
    resetSettings.mockReset().mockResolvedValue(DEFAULT_SETTINGS);
    useSettingsStore.getState().hydrate(DEFAULT_SETTINGS);
  });
  afterEach(() => vi.useRealTimers());

  it("flush saves a pending edit without waiting for the debounce", async () => {
    updateSettings.mockImplementation((s) => Promise.resolve(s));
    useSettingsStore.getState().update({ compactTable: true });
    expect(updateSettings).not.toHaveBeenCalled();
    flushSettings();
    await vi.runAllTimersAsync();
    expect(updateSettings).toHaveBeenCalledTimes(1);
    expect(updateSettings.mock.calls[0]?.[0].compactTable).toBe(true);
  });

  it("a failed save does not discard an edit made while it was in flight", async () => {
    let fail: (e: unknown) => void = () => {};
    updateSettings.mockImplementationOnce(() => new Promise((_, reject) => (fail = reject)));
    updateSettings.mockImplementation((s) => Promise.resolve(s));

    useSettingsStore.getState().update({ compactTable: true });
    await vi.advanceTimersByTimeAsync(300); // first save is now in flight
    useSettingsStore.getState().update({ heatMapping: false }); // newer edit
    fail({ kind: "systemQueryFailed", message: "disk full" });
    await vi.runAllTimersAsync();

    expect(useSettingsStore.getState().heatMapping).toBe(false);
    expect(updateSettings).toHaveBeenCalledTimes(2); // saves never overlap
  });

  it("waits for an in-flight save and discards pre-reset queued saves", async () => {
    let finish: (settings: Settings) => void = () => {};
    updateSettings.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    updateSettings.mockImplementation(async (settings) => settings);
    useSettingsStore.getState().update({ compactTable: true });
    await vi.advanceTimersByTimeAsync(300);
    useSettingsStore.getState().update({ heatMapping: false });
    await vi.advanceTimersByTimeAsync(300);
    const resetting = useSettingsStore.getState().reset();
    expect(resetSettings).not.toHaveBeenCalled();
    finish({ ...DEFAULT_SETTINGS, compactTable: true });
    await resetting;
    expect(updateSettings).toHaveBeenCalledTimes(1);
    expect(resetSettings).toHaveBeenCalledTimes(1);
    expect(useSettingsStore.getState().compactTable).toBe(false);
    expect(useSettingsStore.getState().heatMapping).toBe(true);
  });

  it("keeps edits made while Reset is in flight and saves them after defaults", async () => {
    let finish: (settings: Settings) => void = () => {};
    resetSettings.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    updateSettings.mockImplementation(async (settings) => settings);
    useSettingsStore.getState().hydrate({ ...DEFAULT_SETTINGS, compactTable: true });
    const resetting = useSettingsStore.getState().reset();
    await vi.advanceTimersByTimeAsync(0);
    useSettingsStore.getState().update({ heatMapping: false });
    await vi.advanceTimersByTimeAsync(300);
    expect(updateSettings).not.toHaveBeenCalled();
    finish(DEFAULT_SETTINGS);
    await resetting;
    await vi.runAllTimersAsync();
    expect(useSettingsStore.getState().heatMapping).toBe(false);
    expect(updateSettings).toHaveBeenLastCalledWith({ ...DEFAULT_SETTINGS, heatMapping: false });
  });

  it("restores persisted settings when Reset fails after discarding a pending edit", async () => {
    resetSettings.mockRejectedValueOnce(new Error("fixture reset failed"));
    useSettingsStore.getState().update({ theme: "dark" });
    await useSettingsStore.getState().reset();
    await vi.runAllTimersAsync();
    expect(updateSettings).not.toHaveBeenCalled();
    expect(useSettingsStore.getState().theme).toBe(DEFAULT_SETTINGS.theme);
  });
});
