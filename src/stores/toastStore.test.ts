import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast, useToastStore } from "./toastStore";

const messages = () => useToastStore.getState().toasts.map((t) => t.message);

beforeEach(() => {
  vi.useFakeTimers();
  useToastStore.setState({ toasts: [] });
});
afterEach(() => {
  vi.useRealTimers();
});

it("dismisses plain toasts after their duration", () => {
  toast.success("Saved");
  vi.advanceTimersByTime(3999);
  expect(messages()).toEqual(["Saved"]);
  vi.advanceTimersByTime(1);
  expect(messages()).toEqual([]);
});

it("keeps a toast with an action until it is used or dismissed", () => {
  toast.error("Could not close", { label: "Force end…", run: () => {} });
  vi.advanceTimersByTime(60_000);
  expect(messages()).toEqual(["Could not close"]);
  useToastStore.getState().dismiss(useToastStore.getState().toasts[0]!.id);
  expect(messages()).toEqual([]);
});

it("pauses the countdown and resumes with the time that was left", () => {
  toast.info("Copied");
  const { id } = useToastStore.getState().toasts[0]!;
  vi.advanceTimersByTime(3000);
  useToastStore.getState().pause(id);
  vi.advanceTimersByTime(60_000);
  expect(messages()).toEqual(["Copied"]);
  useToastStore.getState().resume(id);
  vi.advanceTimersByTime(999);
  expect(messages()).toEqual(["Copied"]);
  vi.advanceTimersByTime(1);
  expect(messages()).toEqual([]);
});
