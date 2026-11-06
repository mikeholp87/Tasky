import { create } from "zustand";
import type { ActivityEvent } from "@/types";

/** Matches the backend's log length. */
export const MAX_ACTIVITY_EVENTS = 1_000;

interface ActivityState {
  /** Newest first. */
  events: ActivityEvent[];
  /** Highest sequence number seen; the backfill starts after it. */
  lastSeq: number;
  /** Adds live events, ignoring any already seen (live and backfill overlap). */
  add: (events: readonly ActivityEvent[]) => void;
}

export const useActivityStore = create<ActivityState>((set) => ({
  events: [],
  lastSeq: 0,
  add: (incoming) =>
    set((s) => {
      const known = new Map(s.events.map((e) => [e.seq, e]));
      for (const event of incoming) known.set(event.seq, event);
      const events = [...known.values()]
        .sort((a, b) => b.seq - a.seq)
        .slice(0, MAX_ACTIVITY_EVENTS);
      if (events.length === s.events.length && events.every((e, i) => e === s.events[i])) return s;
      return {
        events,
        lastSeq: Math.max(s.lastSeq, events[0]?.seq ?? 0),
      };
    }),
}));
