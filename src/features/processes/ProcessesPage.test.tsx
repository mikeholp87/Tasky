import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import { useUiStore } from "@/stores/uiStore";
import { NOW_MS, STATIC_INFO, makeProcessRow, makeTick } from "@/test/fixtures";
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

beforeEach(() => {
  useProcessStore.setState({
    search: "",
    preset: "all",
    sort: { column: "pid", direction: "asc" },
    selectedPid: null,
    selectedStartTime: null,
    mode: "list",
    collapsed: new Set(),
    expandedApps: new Set(),
  });
  useMetricsStore.setState({
    staticInfo: STATIC_INFO,
    tick: makeTick(processes),
    status: "live",
  });
});

describe("ProcessesPage", () => {
  it("expands application groups from the keyboard without selecting a real PID", async () => {
    useMetricsStore.setState({
      tick: makeTick([
        makeProcessRow({ pid: 10, name: "browser", exe: "C:\\browser.exe" }),
        makeProcessRow({ pid: 11, name: "browser", exe: "C:\\browser.exe" }),
      ]),
    });
    useProcessStore.setState({ mode: "apps" });
    render(<ProcessesPage />);
    screen.getByRole("treegrid", { name: "Processes" }).focus();
    await userEvent.keyboard("{Home}{Enter}");
    expect(screen.getByRole("button", { name: "Collapse browser.exe (2)" })).toBeInTheDocument();
    expect(dataRows()).toHaveLength(3);
    expect(useProcessStore.getState().selectedPid).toBeLessThan(0);
  });

  it("renders processes from the latest tick", () => {
    render(<ProcessesPage />);
    expect(screen.getByText("40 processes")).toBeInTheDocument();
    expect(dataRows()[0]).toHaveAttribute("data-pid", "100");
  });

  it("renders malicious OS names as text without creating HTML elements", () => {
    const name = "<img src=x onerror=alert(1)>";
    useMetricsStore.setState({ tick: makeTick([{ ...processes[0]!, name }]) });
    const { container } = render(<ProcessesPage />);
    expect(screen.getByText(name)).toBeInTheDocument();
    expect(container.querySelector("img, script, iframe")).toBeNull();
  });

  it("opens one shared context menu for the right-clicked row only", async () => {
    render(<ProcessesPage />);
    const target = dataRows()[2]!;
    const name = within(target).getAllByRole("gridcell")[0]!.textContent;
    await userEvent.pointer({ keys: "[MouseRight]", target });
    expect(await screen.findByRole("menu", { name: `Actions for ${name}` })).toBeInTheDocument();
    expect(screen.getAllByRole("menu")).toHaveLength(1);
  });

  it("does not open the process menu on the column header", async () => {
    render(<ProcessesPage />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("columnheader", { name: /^Name/ }),
    });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("filters instantly as the user types", async () => {
    useMetricsStore.setState({
      tick: makeTick([
        makeProcessRow({ pid: 10, name: "parent" }),
        makeProcessRow({ pid: 11, parentPid: 10, name: "python" }),
        makeProcessRow({ pid: 12, parentPid: 10, name: "other" }),
      ]),
    });
    render(<ProcessesPage />);
    await userEvent.type(screen.getByRole("searchbox", { name: "Search processes" }), "python");
    expect(dataRows().map((row) => row.getAttribute("data-pid"))).toEqual(["10", "11"]);
    expect(screen.getByText("2 of 3 processes")).toBeInTheDocument();
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
    const grid = screen.getByRole("treegrid", { name: "Processes" });
    grid.focus();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(useProcessStore.getState().selectedPid).toBe(processes[1]?.pid);
    await userEvent.keyboard("{Enter}");
    expect(useUiStore.getState().route).toBe("process-details");
    expect(useUiStore.getState().selectedPid).toBe(processes[1]?.pid);
  });

  it("does not transfer selection or Delete actions to a reused PID", async () => {
    const actions = await import("./actions");
    const spy = vi.spyOn(actions, "requestAction").mockResolvedValue();
    const selected = processes[0]!;
    useProcessStore.getState().select(selected.pid);
    render(<ProcessesPage />);
    act(() =>
      useMetricsStore.setState({
        tick: makeTick([
          { ...selected, startTime: selected.startTime + 100 },
          ...processes.slice(1),
        ]),
      }),
    );
    expect(dataRows().find((r) => r.dataset.pid === String(selected.pid))).toHaveAttribute(
      "aria-selected",
      "false",
    );
    screen.getByRole("treegrid", { name: "Processes" }).focus();
    await userEvent.keyboard("{Delete}");
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it("renders a bounded viewport with 10,000 processes", () => {
    useMetricsStore.setState({ tick: makeTick(mockProcesses(10_000, NOW_MS / 1000)) });
    render(<ProcessesPage />);
    expect(dataRows().length).toBeLessThan(100);
    expect(screen.getByRole("treegrid", { name: "Processes" })).toHaveAttribute(
      "aria-rowcount",
      "10001",
    );
  });

  it("collapses and expands all in list view", async () => {
    useMetricsStore.setState({
      tick: makeTick([
        makeProcessRow({ pid: 10, name: "parent" }),
        makeProcessRow({ pid: 11, parentPid: 10, name: "child" }),
      ]),
    });
    render(<ProcessesPage />);
    expect(dataRows()).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "Collapse all" }));
    expect(dataRows()).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "Expand all" }));
    expect(dataRows()).toHaveLength(2);
  });

  it.each([
    ["list", "none"],
    ["list", "search"],
    ["list", "preset"],
    ["list", "user"],
    ["tree", "none"],
    ["tree", "search"],
    ["tree", "preset"],
    ["tree", "user"],
    ["apps", "none"],
    ["apps", "search"],
    ["apps", "preset"],
    ["apps", "user"],
  ] as const)("collapses and expands all in %s view with %s filtering", async (mode, filter) => {
    const rows = [
      makeProcessRow({ pid: 10, name: "browser", cpuPercent: 50 }),
      makeProcessRow({ pid: 11, parentPid: 10, name: "browser", cpuPercent: 50 }),
      makeProcessRow({ pid: 12, parentPid: 11, name: "browser", cpuPercent: 50 }),
      makeProcessRow({ pid: 20, name: "worker", cpuPercent: 50 }),
      makeProcessRow({ pid: 21, parentPid: 20, name: "worker", cpuPercent: 50 }),
      makeProcessRow({ pid: 30, name: "alone", cpuPercent: 50 }),
    ];
    useMetricsStore.setState({
      staticInfo: { ...STATIC_INFO, selfPid: 10 },
      tick: makeTick(rows.map((row) => ({ ...row, user: "alice" }))),
    });
    useProcessStore.setState({ mode });
    render(<ProcessesPage />);
    if (filter === "search") {
      await userEvent.type(screen.getByRole("searchbox"), "browser");
    } else if (filter === "preset") {
      await userEvent.selectOptions(screen.getByRole("combobox", { name: "Filter" }), "highCpu");
    } else if (filter === "user") {
      await userEvent.selectOptions(screen.getByRole("combobox", { name: "Filter" }), "user");
    }

    const expandedCount =
      (filter === "search" ? 3 : 6) + (mode === "apps" ? (filter === "search" ? 1 : 2) : 0);
    const collapsedCount = filter === "search" ? 1 : 3;
    const expand = () => userEvent.click(screen.getByRole("button", { name: "Expand all" }));
    const collapse = () => userEvent.click(screen.getByRole("button", { name: "Collapse all" }));
    await expand();
    expect(dataRows()).toHaveLength(expandedCount);
    await collapse();
    await collapse();
    expect(dataRows()).toHaveLength(collapsedCount);
    for (const row of dataRows().filter((r) => r.hasAttribute("aria-expanded"))) {
      expect(row).toHaveAttribute("aria-expanded", "false");
    }

    // A metrics refresh must not reopen existing branches.
    act(() => {
      useMetricsStore.setState({
        tick: makeTick(rows.map((row) => ({ ...row, user: "alice", cpuPercent: 60 }))),
      });
    });
    expect(dataRows()).toHaveLength(collapsedCount);
    await userEvent.click(
      screen.getByRole("button", {
        name: mode === "apps" ? "Expand browser (3)" : "Expand browser",
      }),
    );
    expect(dataRows()).toHaveLength(collapsedCount + (mode === "apps" ? 3 : 1));
    await collapse();
    expect(dataRows()).toHaveLength(collapsedCount);
    await expand();
    await expand();
    expect(dataRows()).toHaveLength(expandedCount);
    for (const row of dataRows().filter((r) => r.hasAttribute("aria-expanded"))) {
      expect(row).toHaveAttribute("aria-expanded", "true");
    }
  });

  it("keeps tree and application expansion independent when switching views", async () => {
    useMetricsStore.setState({
      tick: makeTick([
        makeProcessRow({ pid: 10, name: "browser" }),
        makeProcessRow({ pid: 11, parentPid: 10, name: "browser" }),
      ]),
    });
    render(<ProcessesPage />);
    await userEvent.click(screen.getByRole("radio", { name: "Tree view" }));
    await userEvent.click(screen.getByRole("button", { name: "Collapse all" }));
    expect(dataRows()).toHaveLength(1);
    await userEvent.click(screen.getByRole("radio", { name: /Applications view/ }));
    await userEvent.click(screen.getByRole("button", { name: "Expand all" }));
    expect(dataRows()).toHaveLength(3);
    await userEvent.click(screen.getByRole("radio", { name: "Tree view" }));
    expect(dataRows()).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "Expand all" }));
    expect(dataRows()).toHaveLength(2);
    await userEvent.click(screen.getByRole("radio", { name: /Applications view/ }));
    expect(dataRows()).toHaveLength(3);
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
    useProcessStore.getState().select(processes[0]?.pid ?? null);
    render(<ProcessesPage />);
    screen.getByRole("treegrid", { name: "Processes" }).focus();
    await userEvent.keyboard("{Delete}");
    expect(spy).toHaveBeenCalledWith(processes[0], { type: "terminate" });
    spy.mockRestore();
  });
});
