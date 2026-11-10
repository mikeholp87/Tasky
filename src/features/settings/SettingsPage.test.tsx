import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "@/stores/settingsStore";
import { SettingsPage } from "./SettingsPage";

const originalReset = useSettingsStore.getState().reset;
const reset = vi.fn(() => Promise.resolve());

beforeEach(() => {
  reset.mockClear();
  useSettingsStore.setState({ reset });
});
afterEach(() => useSettingsStore.setState({ reset: originalReset }));

describe("Reset all settings", () => {
  it("asks in an in-app dialog and does nothing on Cancel", async () => {
    const confirm = vi.spyOn(window, "confirm");
    render(<SettingsPage />);
    await userEvent.click(screen.getByRole("button", { name: "Reset" }));
    const dialog = screen.getByRole("alertdialog", { name: "Reset all settings?" });
    expect(dialog).toHaveTextContent(/alert rules/);
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(reset).not.toHaveBeenCalled();
    expect(confirm).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  it("resets once confirmed", async () => {
    render(<SettingsPage />);
    await userEvent.click(screen.getByRole("button", { name: "Reset" }));
    await userEvent.click(screen.getByRole("button", { name: "Reset settings" }));
    expect(reset).toHaveBeenCalledOnce();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("closes on Escape without resetting", async () => {
    render(<SettingsPage />);
    await userEvent.click(screen.getByRole("button", { name: "Reset" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(reset).not.toHaveBeenCalled();
  });
});
