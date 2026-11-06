import { beforeEach, describe, expect, it } from "vitest";
import type { ActivityEvent } from "@/types";
import { MAX_ACTIVITY_EVENTS, useActivityStore } from "./activityStore";

const ev = (seq: number): ActivityEvent => ({
  seq,
  timestampMs: seq,
  kind: "processStarted",
  pid: seq,
  name: "p",
  detail: null,
});

describe("activity store", () => {
  beforeEach(() => useActivityStore.setState({ events: [], lastSeq: 0 }));

  it("keeps newest first and ignores events it already has", () => {
    const { add } = useActivityStore.getState();
    add([ev(1), ev(2)]);
    add([ev(2), ev(3)]); // backfill overlapping a live tick
    const s = useActivityStore.getState();
    expect(s.events.map((e) => e.seq)).toEqual([3, 2, 1]);
    expect(s.lastSeq).toBe(3);
  });

  it("is bounded", () => {
    const { add } = useActivityStore.getState();
    add(Array.from({ length: MAX_ACTIVITY_EVENTS + 25 }, (_, i) => ev(i + 1)));
    const s = useActivityStore.getState();
    expect(s.events).toHaveLength(MAX_ACTIVITY_EVENTS);
    expect(s.events[0]?.seq).toBe(MAX_ACTIVITY_EVENTS + 25);
  });

  it("merges an older backfill after a newer live event without losing the gap", () => {
    const { add } = useActivityStore.getState();
    add([ev(8)]);
    add([ev(5), ev(6), ev(7), ev(8)]);
    expect(useActivityStore.getState().events.map((e) => e.seq)).toEqual([8, 7, 6, 5]);
    expect(useActivityStore.getState().lastSeq).toBe(8);
  });
});
