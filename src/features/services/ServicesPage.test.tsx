import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useToastStore } from "@/stores/toastStore";
import type { ServiceAction, ServiceInfo, ServiceScope } from "@/types";
import { filterServices, stateLabel } from "./logic";

const api = vi.hoisted(() => ({
  listServices: vi.fn<(scope: ServiceScope) => Promise<ServiceInfo[]>>(),
  controlService: vi.fn<(s: ServiceScope, u: string, a: ServiceAction) => Promise<void>>(),
}));
vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: api,
}));
const { ServicesPage } = await import("./ServicesPage");

const svc = (over: Partial<ServiceInfo>): ServiceInfo => ({
  unit: "cups.service",
  description: "CUPS Scheduler",
  activeState: "active",
  subState: "running",
  startup: "enabled",
  mainPid: 812,
  protected: false,
  ...over,
});
const list = [
  svc({}),
  svc({
    unit: "docker.service",
    description: "Docker",
    activeState: "inactive",
    subState: "dead",
    mainPid: null,
  }),
  svc({
    unit: "geoclue.service",
    description: "Location",
    activeState: "failed",
    subState: "failed",
    mainPid: null,
  }),
  svc({ unit: "dbus.service", description: "D-Bus", mainPid: 500, protected: true }),
];

beforeEach(() => {
  Object.values(api).forEach((m) => m.mockReset());
  useToastStore.setState({ toasts: [] });
  api.listServices.mockResolvedValue(list);
});

describe("service state and filtering", () => {
  it("names every state in words", () => {
    expect(stateLabel({ activeState: "active", subState: "running" })).toBe("Running");
    expect(stateLabel({ activeState: "active", subState: "exited" })).toBe("Active");
    expect(stateLabel({ activeState: "failed", subState: "failed" })).toBe("Failed");
    expect(stateLabel({ activeState: "inactive", subState: "dead" })).toBe("Stopped");
  });

  it("filters by state and by name, description or PID", () => {
    expect(filterServices(list, "", "failed").map((s) => s.unit)).toEqual(["geoclue.service"]);
    expect(filterServices(list, "", "stopped").map((s) => s.unit)).toEqual(["docker.service"]);
    expect(filterServices(list, "812", "all").map((s) => s.unit)).toEqual(["cups.service"]);
    expect(filterServices(list, "d-bus", "all").map((s) => s.unit)).toEqual(["dbus.service"]);
  });
});

describe("services page", () => {
  it("offers Stop and Restart for a running service, Start for a stopped one, nothing for protected", async () => {
    render(<ServicesPage />);
    expect(await screen.findByRole("button", { name: "Stop cups.service" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restart cups.service" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start docker.service" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^(Start|Stop|Restart) dbus.service/ })).toBeNull();
    expect(screen.getByText("Protected")).toBeInTheDocument();
  });

  it("sends the scope, unit and action to the backend and reloads", async () => {
    api.controlService.mockResolvedValue();
    render(<ServicesPage />);
    await userEvent.click(await screen.findByRole("button", { name: "Stop cups.service" }));
    expect(api.controlService).toHaveBeenCalledWith("system", "cups.service", "stop");
    await waitFor(() => expect(api.listServices.mock.calls.length).toBeGreaterThan(1));
  });

  it("stays quiet when the user declines the confirmation", async () => {
    api.controlService.mockRejectedValue({
      kind: "operationCancelled",
      message: "Operation cancelled.",
    });
    render(<ServicesPage />);
    await userEvent.click(await screen.findByRole("button", { name: "Restart cups.service" }));
    await waitFor(() => expect(api.controlService).toHaveBeenCalled());
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it("shows the backend's reason when the action is refused", async () => {
    api.controlService.mockRejectedValue({
      kind: "actionUnavailable",
      message: "That service no longer exists.",
    });
    render(<ServicesPage />);
    await userEvent.click(await screen.findByRole("button", { name: "Start docker.service" }));
    await waitFor(() =>
      expect(useToastStore.getState().toasts[0]?.message).toBe("That service no longer exists."),
    );
  });
});
