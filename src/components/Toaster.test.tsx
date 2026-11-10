import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast, useToastStore } from "@/stores/toastStore";
import { Toaster } from "./Toaster";

beforeEach(() => {
  vi.useFakeTimers();
  useToastStore.setState({ toasts: [] });
});
afterEach(() => {
  vi.useRealTimers();
});

it("announces errors assertively and the rest politely, without nested alert roles", () => {
  const { container } = render(<Toaster />);
  act(() => {
    toast.success("Saved");
    toast.error("Failed");
  });
  expect(screen.getByText("Saved").closest("[aria-live]")).toHaveAttribute("aria-live", "polite");
  expect(screen.getByText("Failed").closest("[aria-live]")).toHaveAttribute(
    "aria-live",
    "assertive",
  );
  expect(container.querySelector("[role]")).toBeNull();
});

it("stays while hovered", () => {
  render(<Toaster />);
  act(() => toast.info("Copied"));
  const item = screen.getByText("Copied").closest("div.pointer-events-auto")!;
  fireEvent.mouseEnter(item);
  act(() => vi.advanceTimersByTime(10_000));
  expect(screen.getByText("Copied")).toBeInTheDocument();
  fireEvent.mouseLeave(item);
  act(() => vi.advanceTimersByTime(4000));
  expect(screen.queryByText("Copied")).toBeNull();
});
