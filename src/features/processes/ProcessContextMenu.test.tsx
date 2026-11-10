import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import { useUiStore } from "@/stores/uiStore";
import { NOW_MS, STATIC_INFO, makeTick } from "@/test/fixtures";
import { mockProcesses } from "./mock";
import { ProcessesPage } from "./ProcessesPage";

const processes = mockProcesses(10, NOW_MS / 1000);

// jsdom has no layout; give the virtualized grid a viewport so rows render.
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get: () => 800,
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get: () => 1200,
  });
});
afterAll(() => {
  Reflect.deleteProperty(HTMLElement.prototype, "clientHeight");
  Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
});

beforeEach(() => {
  useProcessStore.setState({
    search: "",
    preset: "all",
    sort: { column: "pid", direction: "asc" },
    selectedPid: null,
    mode: "list",
    collapsed: new Set(),
    expandedApps: new Set(),
  });
  useMetricsStore.setState({ staticInfo: STATIC_INFO, tick: makeTick(processes), status: "live" });
  useUiStore.setState({ route: "processes", selectedPid: null, selectedStartTime: null });
});

const rowOf = (pid: number) =>
  screen.getAllByRole("row").find((r) => r.getAttribute("data-pid") === String(pid))!;

describe("ProcessContextMenu", () => {
  it("opens for the right-clicked row and acts on that process", async () => {
    render(<ProcessesPage />);
    const target = processes[3]!;
    await userEvent.pointer({ keys: "[MouseRight]", target: rowOf(target.pid) });
    const menu = await screen.findByRole("menu", { name: `Actions for ${target.name}` });
    expect(within(menu).getByText(`${target.name} · PID ${target.pid}`)).toBeInTheDocument();
    expect(useProcessStore.getState().selectedPid).toBe(target.pid);

    await userEvent.click(within(menu).getByRole("menuitem", { name: "Open details" }));
    expect(useUiStore.getState()).toMatchObject({
      route: "process-details",
      selectedPid: target.pid,
      selectedStartTime: target.startTime,
    });
  });

  it.each([
    ["Shift+F10", "{Shift>}{F10}{/Shift}"],
    ["the context menu key", "{ContextMenu}"],
  ])("opens for the selected row with %s", async (_name, keys) => {
    render(<ProcessesPage />);
    const grid = screen.getByRole("treegrid", { name: "Processes" });
    grid.focus();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    const selected = processes[1]!;
    expect(grid).toHaveAttribute("aria-activedescendant", rowOf(selected.pid).id);

    await userEvent.keyboard(keys);
    expect(
      await screen.findByRole("menu", { name: `Actions for ${selected.name}` }),
    ).toBeInTheDocument();
  });

  it("does nothing from the keyboard when no row is selected", async () => {
    render(<ProcessesPage />);
    screen.getByRole("treegrid", { name: "Processes" }).focus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
