import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { makeProcessRow, makeTick } from "@/test/fixtures";
import { useMetricsStore } from "@/stores/metricsStore";
import type { ProcessRow } from "@/types";
import { TopMemoryCard } from "./TopMemoryCard";

vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => false,
}));

const MB = 1024 ** 2;
const NOW_MS = 1_800_000_000_000;
const NOW = NOW_MS / 1000;

/** Twelve established programs, so a small new one falls outside the top ten. */
const established: ProcessRow[] = Array.from({ length: 12 }, (_, i) =>
  makeProcessRow({
    pid: 100 + i,
    name: `app${i}`,
    exe: `/usr/bin/app${i}`,
    memoryBytes: (900 - i * 50) * MB,
    startTime: NOW - 86_400,
  }),
);

function tick(processes: ProcessRow[]) {
  act(() => {
    useMetricsStore.getState().ingestTick({ ...makeTick(), timestampMs: NOW_MS, processes });
  });
}

beforeEach(() => {
  useMetricsStore.setState({ tick: null });
});

describe("What is using memory", () => {
  it("lists a program the moment it starts, even a small one, and drops it when it exits", () => {
    render(<TopMemoryCard />);
    tick(established);
    expect(screen.queryByText("newapp")).not.toBeInTheDocument();

    // The user opens a small program: it is listed on the very next tick,
    // although it is far outside the top ten by size, and marked New.
    const fresh = makeProcessRow({
      pid: 900,
      name: "newapp",
      exe: "/usr/bin/newapp",
      memoryBytes: 8 * MB,
      startTime: NOW - 2,
    });
    tick([...established, fresh]);
    expect(screen.getByText("newapp")).toBeInTheDocument();
    expect(screen.getByText("New")).toBeInTheDocument();

    // It is closed: gone on the next tick.
    tick(established);
    expect(screen.queryByText("newapp")).not.toBeInTheDocument();
    expect(screen.queryByText("New")).not.toBeInTheDocument();
  });

  it("stops marking a program as new after the window, and then ranks it by size", () => {
    render(<TopMemoryCard />);
    const old = makeProcessRow({
      pid: 900,
      name: "newapp",
      exe: "/usr/bin/newapp",
      memoryBytes: 8 * MB,
      startTime: NOW - 600,
    });
    tick([...established, old]);
    expect(screen.queryByText("newapp")).not.toBeInTheDocument();
  });
});
