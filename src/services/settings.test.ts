import { describe, expect, it, vi } from "vitest";
import { DEFAULT_VISIBLE_COLUMNS } from "@/features/processes/logic";
import { useProcessStore } from "@/stores/processStore";
import { DEFAULT_SETTINGS, useSettingsStore } from "@/stores/settingsStore";
import { columnsFromSettings, loadSettings } from "./settings";

const api = vi.hoisted(() => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn((s: unknown) => Promise.resolve(s)),
}));
vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: api,
}));

describe("columnsFromSettings", () => {
  it("falls back to defaults when nothing is stored", () => {
    expect(columnsFromSettings({ visibleColumns: [], columnWidths: {} }).visibleColumns).toEqual(
      DEFAULT_VISIBLE_COLUMNS,
    );
  });

  it("drops unknown ids and always keeps the name column first", () => {
    const r = columnsFromSettings({
      visibleColumns: ["cpu", "bogus", "pid"],
      columnWidths: { cpu: 120, nope: 50 },
    });
    expect(r.visibleColumns).toEqual(["name", "cpu", "pid"]);
    expect(r.columnWidths).toEqual({ cpu: 120 });
  });
});

describe("column sync", () => {
  it("keeps the table and settings in step in both directions without echoing", async () => {
    api.getSettings.mockResolvedValue({
      ...DEFAULT_SETTINGS,
      visibleColumns: ["name", "cpu"],
      columnWidths: { cpu: 120 },
    });
    await loadSettings();
    expect(useProcessStore.getState().visibleColumns).toEqual(["name", "cpu"]);

    useProcessStore.getState().setColumnWidth("cpu", 150);
    expect(useSettingsStore.getState().columnWidths).toEqual({ cpu: 150 });

    // A rolled-back save or a reset replaces the settings from outside the table.
    useSettingsStore.setState({ visibleColumns: ["pid"], columnWidths: {} });
    expect(useProcessStore.getState().visibleColumns).toEqual(["name", "pid"]);
    expect(useProcessStore.getState().columnWidths).toEqual({});
    expect(useSettingsStore.getState().visibleColumns).toEqual(["pid"]);
  });
});
