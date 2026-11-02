import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useToastStore } from "@/stores/toastStore";
import type { ProcessRow, ProtectionAssessment } from "@/types";
import { ConfirmActionDialog } from "./ConfirmActionDialog";
import { mockProcesses } from "./mock";

const backend = vi.hoisted(() => ({
  assessProcessAction: vi.fn(),
  performProcessAction: vi.fn(),
  revealExecutable: vi.fn(),
}));

vi.mock("@/services/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/ipc")>();
  return { ...actual, backend, isBackendAvailable: () => true };
});

// Imported after the mock is registered.
const { requestAction, useActionStore } = await import("./actions");

const row: ProcessRow = { ...(mockProcesses(1, 1_800_000_000)[0] as ProcessRow), name: "node" };
const allowed: ProtectionAssessment = { level: "allowed", reasons: [] };

describe("process action flow", () => {
  beforeEach(() => {
    backend.performProcessAction.mockResolvedValue({ message: "Done." });
  });
  afterEach(() => {
    vi.clearAllMocks();
    useActionStore.setState({ pending: null, busy: false });
    useToastStore.setState({ toasts: [] });
  });

  it("always confirms before ending a process", async () => {
    backend.assessProcessAction.mockResolvedValue(allowed);
    render(<ConfirmActionDialog />);
    await requestAction(row, { type: "terminate" });

    expect(await screen.findByRole("alertdialog", { name: "End node?" })).toBeInTheDocument();
    expect(screen.getByText(/may contain unsaved data/)).toBeInTheDocument();
    expect(backend.performProcessAction).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "End process" }));
    expect(backend.performProcessAction).toHaveBeenCalledWith(
      row.pid,
      row.startTime,
      { type: "terminate" },
      false,
    );
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });

  it("uses stronger wording for force end", async () => {
    backend.assessProcessAction.mockResolvedValue(allowed);
    render(<ConfirmActionDialog />);
    await requestAction(row, { type: "forceKill" });
    expect(await screen.findByText(/without a chance to save/)).toBeInTheDocument();
  });

  it("offers no way to confirm a blocked action", async () => {
    backend.assessProcessAction.mockResolvedValue({
      level: "blocked",
      reasons: ["Windows marks this process as critical."],
    });
    render(<ConfirmActionDialog />);
    await requestAction(row, { type: "forceKill" });

    expect(await screen.findByText("Windows marks this process as critical.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Force end/ })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(backend.performProcessAction).not.toHaveBeenCalled();
  });

  it("passes the acknowledgement only after a warning is confirmed", async () => {
    backend.assessProcessAction.mockResolvedValue({
      level: "warning",
      reasons: ["This is the Windows shell."],
    });
    render(<ConfirmActionDialog />);
    await requestAction(row, { type: "suspend" });
    await userEvent.click(await screen.findByRole("button", { name: "Suspend anyway" }));
    expect(backend.performProcessAction).toHaveBeenCalledWith(
      row.pid,
      row.startTime,
      { type: "suspend" },
      true,
    );
  });

  it("runs harmless allowed actions without a dialog", async () => {
    backend.assessProcessAction.mockResolvedValue(allowed);
    render(<ConfirmActionDialog />);
    await requestAction(row, { type: "resume" });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(backend.performProcessAction).toHaveBeenCalledTimes(1);
    expect(useToastStore.getState().toasts[0]?.message).toBe("Done.");
  });

  it("offers force end when a graceful close is impossible", async () => {
    backend.assessProcessAction.mockResolvedValue(allowed);
    backend.performProcessAction.mockRejectedValueOnce({
      kind: "actionUnavailable",
      message: "This process has no window that can be asked to close.",
    });
    render(<ConfirmActionDialog />);
    await requestAction(row, { type: "terminate" });
    await userEvent.click(await screen.findByRole("button", { name: "End process" }));

    const toast = await waitFor(() => {
      const t = useToastStore.getState().toasts[0];
      expect(t?.action?.label).toBe("Force end…");
      return t;
    });
    // The follow-up goes through its own confirmation.
    toast?.action?.run();
    expect(await screen.findByRole("alertdialog", { name: "Force end node?" })).toBeInTheDocument();
  });
});
