import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { useProcessStore } from "@/stores/processStore";
import { useUiStore } from "@/stores/uiStore";
import { CommandPalette } from "./CommandPalette";

// jsdom does not implement scrolling.
beforeAll(() => {
  Element.prototype.scrollIntoView = () => {};
});

beforeEach(() => {
  useUiStore.setState({ route: "overview" });
  useProcessStore.setState({ search: "" });
});

const open = () => userEvent.keyboard("{Control>}k{/Control}");

describe("CommandPalette", () => {
  it("opens with Ctrl+K and closes with Escape", async () => {
    render(<CommandPalette />);
    expect(screen.queryByRole("dialog")).toBeNull();
    await open();
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Command" })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("filters commands as the user types", async () => {
    render(<CommandPalette />);
    await open();
    await userEvent.keyboard("memory");
    const options = screen.getAllByRole("option").map((o) => o.textContent);
    expect(options).toEqual(['Search processes for "memory"', "Go to Memory"]);
  });

  it("moves the active option with the arrow keys and runs it with Enter", async () => {
    render(<CommandPalette />);
    await open();
    await userEvent.keyboard("go to st");
    const input = screen.getByRole("combobox");
    expect(screen.getAllByRole("option")[0]).toHaveAttribute("aria-selected", "true");

    await userEvent.keyboard("{ArrowDown}");
    const storage = screen.getByRole("option", { name: "Go to Storage" });
    expect(storage).toHaveAttribute("aria-selected", "true");
    expect(input).toHaveAttribute("aria-activedescendant", storage.id);

    await userEvent.keyboard("{Enter}");
    expect(useUiStore.getState().route).toBe("storage");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("searches processes for free text", async () => {
    render(<CommandPalette />);
    await open();
    await userEvent.keyboard("firefox{Enter}");
    expect(useUiStore.getState().route).toBe("processes");
    expect(useProcessStore.getState().search).toBe("firefox");
  });
});
