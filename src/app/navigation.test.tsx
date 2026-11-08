import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import { makeTick } from "@/test/fixtures";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";

beforeEach(() => {
  useUiStore.setState({ route: "overview" });
  useMetricsStore.setState({ tick: null });
});

it("lists Process Details in the sidebar only while it is open", () => {
  const { rerender } = render(<Sidebar />);
  expect(screen.queryByRole("button", { name: "Process Details" })).toBeNull();
  expect(screen.getByRole("button", { name: "Ports" })).toBeInTheDocument();

  useUiStore.setState({ route: "process-details" });
  rerender(<Sidebar />);
  expect(screen.getByRole("button", { name: "Process Details" })).toHaveAttribute(
    "aria-current",
    "page",
  );
});

it("says in words, not only colour, when CPU or memory is under pressure", () => {
  const calm = makeTick();
  useMetricsStore.setState({ tick: calm });
  const { rerender } = render(<TopBar />);
  expect(screen.queryByText(/usage$/)).toBeNull();

  useMetricsStore.setState({
    tick: {
      ...calm,
      cpu: { ...calm.cpu, usagePercent: 85 },
      memory: { ...calm.memory, usedPercent: 95 },
    },
  });
  rerender(<TopBar />);
  expect(screen.getByText("High CPU usage")).toBeInTheDocument();
  expect(screen.getByText("Very high Memory usage")).toBeInTheDocument();
});
