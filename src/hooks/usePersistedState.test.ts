import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { usePersistedState } from "./usePersistedState";

const VIEWS = ["listening", "all"] as const;

describe("usePersistedState", () => {
  beforeEach(() => localStorage.clear());

  it("saves a choice and restores it on the next mount", () => {
    const first = renderHook(() => usePersistedState("ports.view", "listening", VIEWS));
    act(() => first.result.current[1]("all"));
    first.unmount();
    const second = renderHook(() => usePersistedState("ports.view", "listening", VIEWS));
    expect(second.result.current[0]).toBe("all");
  });

  it("ignores a stored value that is no longer allowed", () => {
    localStorage.setItem("tm:ports.view", JSON.stringify("removed-option"));
    const { result } = renderHook(() => usePersistedState("ports.view", "listening", VIEWS));
    expect(result.current[0]).toBe("listening");
  });
});
