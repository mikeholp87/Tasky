import { beforeEach, describe, expect, it, vi } from "vitest";

const stored = (key: string, state: unknown) =>
  localStorage.setItem(key, JSON.stringify({ state, version: 0 }));

// The stores read storage once, when first imported, so each test re-imports them.
async function load() {
  vi.resetModules();
  const { useUiStore } = await import("./uiStore");
  const { useProcessStore } = await import("./processStore");
  return { ui: useUiStore.getState(), proc: useProcessStore.getState(), useUiStore };
}

describe("persisted app state", () => {
  beforeEach(() => localStorage.clear());

  it("restores the page and the process view", async () => {
    stored("tm:ui", { route: "ports" });
    stored("tm:processes", {
      preset: "highMemory",
      sort: { column: "memory", direction: "asc" },
      mode: "tree",
    });
    const { ui, proc } = await load();
    expect(ui.route).toBe("ports");
    expect([proc.preset, proc.sort, proc.mode]).toEqual([
      "highMemory",
      { column: "memory", direction: "asc" },
      "tree",
    ]);
  });

  it("falls back to defaults for stale or invalid values", async () => {
    stored("tm:ui", { route: "process-details" }); // needs a PID, so never restored
    stored("tm:processes", {
      preset: "gone",
      sort: { column: "gone", direction: "up" },
      mode: "x",
    });
    const { ui, proc } = await load();
    expect(ui.route).toBe("overview");
    expect([proc.preset, proc.sort, proc.mode]).toEqual([
      "all",
      { column: "cpu", direction: "desc" },
      "list",
    ]);
  });

  it("survives unreadable storage and saves changes", async () => {
    localStorage.setItem("tm:ui", "{not json");
    const { ui, useUiStore } = await load();
    expect(ui.route).toBe("overview");
    useUiStore.getState().navigate("memory");
    expect(JSON.parse(localStorage.getItem("tm:ui") ?? "{}").state.route).toBe("memory");
  });
});
