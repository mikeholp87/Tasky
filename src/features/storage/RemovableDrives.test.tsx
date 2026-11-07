import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { Volume } from "@/types";
import { RemovableDrives } from "./RemovableDrives";

const api = vi.hoisted(() => ({
  listVolumes: vi.fn<() => Promise<Volume[]>>(),
  mountVolume: vi.fn<(device: string) => Promise<string | null>>(),
  unmountVolume: vi.fn<(device: string) => Promise<void>>(),
}));
vi.mock("@/services/ipc", async (orig) => ({
  ...(await orig<typeof import("@/services/ipc")>()),
  isBackendAvailable: () => true,
  backend: api,
}));

const mounted: Volume = {
  device: "/dev/sdb1",
  label: "BACKUP",
  fileSystem: "exfat",
  sizeBytes: 1_000_000_000,
  mountPoint: "/run/media/a/BACKUP",
};
const unmounted: Volume = { ...mounted, device: "/dev/sdc1", label: null, mountPoint: null };

beforeEach(() => {
  Object.values(api).forEach((m) => m.mockReset());
});

it("shows nothing when there are no external drives", async () => {
  api.listVolumes.mockResolvedValue([]);
  const { container } = render(<RemovableDrives />);
  await waitFor(() => expect(api.listVolumes).toHaveBeenCalled());
  expect(container).toBeEmptyDOMElement();
});

it("offers Unmount for a mounted drive and Mount for an unmounted one", async () => {
  api.listVolumes.mockResolvedValue([mounted, unmounted]);
  render(<RemovableDrives />);
  expect(await screen.findByRole("button", { name: "Unmount BACKUP" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Mount /dev/sdc1" })).toBeInTheDocument();
  expect(
    screen.getByText(/System and internal partitions can't be mounted or unmounted/),
  ).toBeInTheDocument();
});

it("passes the device to the backend and reloads the list", async () => {
  api.listVolumes.mockResolvedValue([mounted]);
  api.unmountVolume.mockResolvedValue();
  render(<RemovableDrives />);
  await userEvent.click(await screen.findByRole("button", { name: "Unmount BACKUP" }));
  expect(api.unmountVolume).toHaveBeenCalledWith("/dev/sdb1");
  await waitFor(() => expect(api.listVolumes).toHaveBeenCalledTimes(2));
});
