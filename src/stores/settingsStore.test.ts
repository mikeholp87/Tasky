import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/types";
import { DEFAULT_SETTINGS, flushSettings, useSettingsStore } from "./settingsStore";

const updateSettings = vi.fn<(s: Settings) => Promise<Settings>>();

vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: { updateSettings: (s: Settings) => updateSettings(s) },
}));

describe("settings store saving", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    updateSettings.mockReset();
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
});
