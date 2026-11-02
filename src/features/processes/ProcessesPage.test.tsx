import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import { useUiStore } from "@/stores/uiStore";
import { NOW_MS, STATIC_INFO, makeTick } from "@/test/fixtures";
import { mockProcesses } from "./mock";
import { ProcessesPage } from "./ProcessesPage";

const processes = mockProcesses(40, NOW_MS / 1000);

// jsdom has no layout; give the virtualized grid a viewport so rows render.
// The real getters live on Element.prototype, so deleting the overrides restores them.
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

function dataRows() {
  return screen.getAllByRole("row").filter((r) => r.hasAttribute("data-pid"));
}

describe("ProcessesPage", () => {
  beforeEach(() => {
    useProcessStore.setState({
      search: "",
      preset: "all",
      sort: { column: "pid", direction: "asc" },
      selectedPid: null,
      mode: "list",
    });
    useMetricsStore.setState({
      staticInfo: STATIC_INFO,
      tick: makeTick(processes),
      status: "live",
    });
  });

  it("renders processes from the latest tick", () => {
    render(<ProcessesPage />);
    expect(screen.getByText("40 processes")).toBeInTheDocument();
    expect(dataRows()[0]).toHaveAttribute("data-pid", "100");
  });

  it("filters instantly as the user types", async () => {
    render(<ProcessesPage />);
    await userEvent.type(screen.getByRole("searchbox", { name: "Search processes" }), "python");
    const shown = dataRows();
    expect(shown.length).toBeGreaterThan(0);
    for (const r of shown)
      expect(within(r).getAllByRole("gridcell")[0]).toHaveTextContent("python");
    expect(screen.getByText(/of 40 processes/)).toBeInTheDocument();
  });

  it("sorts when a column header is clicked", async () => {
    render(<ProcessesPage />);
    await userEvent.click(screen.getByRole("button", { name: "PID" }));
    expect(screen.getByRole("columnheader", { name: "PID" })).toHaveAttribute(
      "aria-sort",
      "descending",
    );
    expect(dataRows()[0]).toHaveAttribute("data-pid", String(processes.at(-1)?.pid));
  });

  it("supports keyboard selection and opening details", async () => {
    render(<ProcessesPage />);
    const grid = screen.getByRole("grid", { name: "Processes" });
    grid.focus();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(useProcessStore.getState().selectedPid).toBe(processes[1]?.pid);
    await userEvent.keyboard("{Enter}");
    expect(useUiStore.getState().route).toBe("process-details");
    expect(useUiStore.getState().selectedPid).toBe(processes[1]?.pid);
  });
});

describe("Delete key", () => {
  it("opens the confirmation instead of killing", async () => {
    const actions = await import("./actions");
    const spy = vi.spyOn(actions, "requestAction").mockResolvedValue();
    useMetricsStore.setState({
      staticInfo: STATIC_INFO,
      tick: makeTick(processes),
      status: "live",
    });
    useProcessStore.setState({ selectedPid: processes[0]?.pid ?? null });
    render(<ProcessesPage />);
    screen.getByRole("grid", { name: "Processes" }).focus();
    await userEvent.keyboard("{Delete}");
    expect(spy).toHaveBeenCalledWith(processes[0], { type: "terminate" });
    spy.mockRestore();
  });
});
