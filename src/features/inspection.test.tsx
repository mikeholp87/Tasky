import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { makeProcessRow, makeTick, STATIC_INFO } from "@/test/fixtures";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import { DEFAULT_SETTINGS, useSettingsStore } from "@/stores/settingsStore";
import type { FileUserList, SocketList, StartupItem } from "@/types";
import { PortsPage } from "./ports/PortsPage";
import { FileFinderPage } from "./file-finder/FileFinderPage";
import { StartupPage } from "./startup/StartupPage";
import { ModulesTab } from "./process-details/inspect/ModulesTab";
import { ThreadsTab } from "./process-details/inspect/ThreadsTab";

const ipc = vi.hoisted(() => ({
  listSockets: vi.fn(),
  findFileUsers: vi.fn(),
  pickPath: vi.fn(),
  listStartupItems: vi.fn(),
  setStartupEnabled: vi.fn(),
  getSettings: vi.fn(),
  getModules: vi.fn(),
  getThreads: vi.fn(),
}));
vi.mock("@/services/ipc", async (original) => ({
  ...(await original<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: ipc,
}));

const process = makeProcessRow({ pid: 42, startTime: 1_000, name: "server.exe" });
const sockets: SocketList = {
  inaccessibleProcesses: 0,
  truncated: false,
  sockets: [
    {
      pid: 42,
      processName: "server.exe",
      connection: {
        protocol: "tcp",
        localAddress: "127.0.0.1",
        localPort: 3000,
        remoteAddress: null,
        remotePort: null,
        state: "listen",
      },
    },
  ],
};
const fileUsers: FileUserList = {
  resolvedPath: "C:\\Temp\\open.txt",
  isDirectory: false,
  inaccessibleProcesses: 0,
  truncated: false,
  note: "Windows reports applications and services holding this file.",
  uses: [{ pid: 42, processName: "server.exe", usage: "inUse", path: "C:\\Temp\\open.txt" }],
};
const startup: StartupItem = {
  id: "win:test",
  name: "Editor",
  command: "editor.exe",
  comment: "Current user · Run",
  scope: "user",
  enabled: true,
  onlyIn: null,
  runsHere: true,
};

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  useMetricsStore.setState({
    staticInfo: { ...STATIC_INFO, platform: "windows" },
    tick: makeTick([process]),
  });
  useUiStore.setState({ selectedPid: null, selectedStartTime: null, route: "overview" });
  ipc.listSockets.mockResolvedValue(sockets);
  ipc.findFileUsers.mockResolvedValue(fileUsers);
  ipc.pickPath.mockResolvedValue(fileUsers.resolvedPath);
  ipc.listStartupItems.mockResolvedValue([startup]);
  ipc.setStartupEnabled.mockResolvedValue(undefined);
  ipc.getSettings.mockResolvedValue(DEFAULT_SETTINGS);
  useSettingsStore.setState(DEFAULT_SETTINGS);
  ipc.getModules.mockResolvedValue([
    {
      name: "kernel32.dll",
      path: "C:\\Windows\\System32\\kernel32.dll",
      base: "10000",
      size: 4096,
      executable: true,
    },
  ]);
  ipc.getThreads.mockResolvedValue({ threads: [], sampledAtMs: 1_000 });
});

describe("Windows diagnostic flows", () => {
  it("finds a port owner and opens the currently observed process instance", async () => {
    render(<PortsPage />);
    await screen.findByText("server.exe");
    await userEvent.type(screen.getByRole("searchbox", { name: "Search ports" }), "3000");
    expect(screen.getByRole("status")).toHaveTextContent("Port 3000 is already in use");
    await userEvent.click(screen.getByRole("button", { name: "Open process details" }));
    expect(useUiStore.getState()).toMatchObject({
      route: "process-details",
      selectedPid: 42,
      selectedStartTime: 1_000,
    });
  });

  it("uses the native file picker, shows owners and reports a rejected lookup", async () => {
    render(<FileFinderPage />);
    await userEvent.click(screen.getByRole("button", { name: /Choose file/ }));
    await screen.findByText("server.exe");
    expect(ipc.pickPath).toHaveBeenCalledWith(false);
    expect(ipc.findFileUsers).toHaveBeenCalledWith(fileUsers.resolvedPath);
    ipc.findFileUsers.mockRejectedValueOnce({
      kind: "permissionDenied",
      message: "Access denied for this file.",
    });
    await userEvent.click(screen.getByRole("button", { name: "Find" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Access denied for this file");
    expect(screen.queryByText("server.exe")).not.toBeInTheDocument();
  });

  it("disables the startup switch while saving, then reads back the updated state", async () => {
    let complete!: () => void;
    ipc.setStartupEnabled.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          complete = resolve;
        }),
    );
    render(<StartupPage />);
    const toggle = await screen.findByRole("switch", { name: "Start Editor at login" });
    await userEvent.click(toggle);
    expect(ipc.setStartupEnabled).toHaveBeenCalledWith("win:test", false);
    expect(toggle).toBeDisabled();
    ipc.listStartupItems.mockResolvedValue([{ ...startup, enabled: false }]);
    await act(async () => complete());
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "false"));
    expect(toggle).not.toBeDisabled();
    expect(ipc.listStartupItems).toHaveBeenCalledTimes(2);
    expect(ipc.getSettings).toHaveBeenCalledOnce();
  });

  it("loads only the open inspection tab with its process identity", async () => {
    const view = render(<ModulesTab pid={42} startTime={1_000} windows />);
    await screen.findByText("kernel32.dll");
    expect(ipc.getModules).toHaveBeenCalledWith(42, 1_000);
    expect(ipc.getThreads).not.toHaveBeenCalled();
    view.unmount();
    ipc.getThreads.mockRejectedValueOnce({
      kind: "processNotFound",
      message: "Process 42 is no longer running.",
    });
    render(<ThreadsTab pid={42} startTime={1_000} linux={false} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("This process has exited");
    expect(ipc.getThreads).toHaveBeenCalledWith(42, 1_000);
  });
});
