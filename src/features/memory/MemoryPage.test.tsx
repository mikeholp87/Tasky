import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import { makeTick, NOW_MS, STATIC_INFO } from "@/test/fixtures";
import { mockProcesses } from "@/features/processes/mock";
import { MemoryPage } from "./MemoryPage";

vi.mock("@/components/charts/TimeSeriesChart", () => ({ TimeSeriesChart: () => null }));
const GiB = 1024 ** 3;
const rows = [
  {
    ...mockProcesses(1, NOW_MS / 1000)[0]!,
    pid: 10,
    name: "browser.exe",
    exe: "C:\\Browser\\browser.exe",
    memoryBytes: 2 * GiB,
  },
  {
    ...mockProcesses(1, NOW_MS / 1000)[0]!,
    pid: 11,
    name: "browser.exe",
    exe: "C:\\Browser\\browser.exe",
    memoryBytes: GiB,
  },
  {
    ...mockProcesses(1, NOW_MS / 1000)[0]!,
    pid: 12,
    name: "editor.exe",
    exe: "C:\\Editor\\editor.exe",
    memoryBytes: GiB,
  },
];

beforeEach(() => {
  useMetricsStore.setState({
    staticInfo: { ...STATIC_INFO, platform: "windows" },
    tick: makeTick(rows, {
      memory: {
        ...makeTick().memory,
        usedPercent: 92,
        usedBytes: 14.72 * GiB,
        availableBytes: 1.28 * GiB,
        kernelNonpagedBytes: 2 * GiB,
        kernelPagedBytes: GiB,
        commitUsedBytes: 19 * GiB,
        commitLimitBytes: 20 * GiB,
      },
    }),
    memoryBaseline: null,
    status: "live",
    monitoring: { paused: false, intervalMs: 1000 },
  });
  useUiStore.setState({ route: "memory", selectedPid: null });
});

describe("Memory diagnostics page", () => {
  it("explains 92% usage and kernel/commit pressure, and opens an individual process", async () => {
    render(<MemoryPage />);
    expect(screen.getByText("Very high memory usage")).toBeInTheDocument();
    expect(screen.getByText(/is the largest reported group:/)).toHaveTextContent(
      "browser.exe is the largest reported group: 3.00 GB across 2 processes",
    );
    expect(screen.getByText(/The nonpaged kernel pool uses/)).toBeInTheDocument();
    expect(screen.getByText(/Committed memory is at 95/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Inspect browser.exe processes" }));
    const group = screen.getByRole("list", { name: "browser.exe processes" });
    await userEvent.click(within(group).getByRole("button", { name: "Inspect PID 11" }));
    expect(useUiStore.getState()).toMatchObject({ route: "process-details", selectedPid: 11 });
  });

  it("sorts by memory growth from ticks collected while other pages are open and filters by PID", async () => {
    useMetricsStore.setState({ tick: null });
    const ingest = useMetricsStore.getState().ingestTick;
    ingest(makeTick(rows, { seq: 1, timestampMs: NOW_MS }));
    ingest(
      makeTick(
        [{ ...rows[0]!, memoryBytes: 2 * GiB }, rows[1]!, { ...rows[2]!, memoryBytes: 2 * GiB }],
        {
          seq: 2,
          timestampMs: NOW_MS + 10_000,
        },
      ),
    );
    render(<MemoryPage />);
    expect(screen.getByText(/Change over the last 10s/)).toBeInTheDocument();
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Sort memory consumers" }),
      "growth",
    );
    expect(screen.getAllByRole("row")[1]).toHaveTextContent("editor.exe");
    expect(screen.getAllByRole("row")[1]).toHaveTextContent("+1.00 GB");
    await userEvent.type(screen.getByRole("searchbox", { name: "Search memory consumers" }), "11");
    expect(
      screen.queryByRole("button", { name: "Inspect editor.exe processes" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Inspect browser.exe processes" }),
    ).toBeInTheDocument();
  });

  it("keeps explanations available with missing process data and unavailable platform counters", () => {
    useMetricsStore.setState({ staticInfo: STATIC_INFO, tick: makeTick() });
    render(<MemoryPage />);
    expect(screen.getByText(/No process memory was reported/)).toBeInTheDocument();
    expect(screen.getByText("No process data available.")).toBeInTheDocument();
    expect(screen.queryByText("Kernel memory / drivers")).not.toBeInTheDocument();
    act(() => useMetricsStore.setState({ monitoring: { paused: true, intervalMs: 1000 } }));
    expect(screen.getByRole("status")).toHaveTextContent("Monitoring is paused");
  });

  it("explains overlapping counters even when process working sets exceed used RAM", async () => {
    useMetricsStore.setState({
      tick: makeTick(rows, { memory: { ...makeTick().memory, usedBytes: GiB } }),
    });
    render(<MemoryPage />);
    await userEvent.click(screen.getByText("Why don't these numbers add up to total RAM used?"));
    expect(screen.getByText(/Subtracting these totals cannot reliably measure/)).toBeVisible();
    expect(screen.queryByText(/unaccounted|hidden memory/i)).not.toBeInTheDocument();
  });
});
