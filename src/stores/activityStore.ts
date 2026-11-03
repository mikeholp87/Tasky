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
      const fresh = incoming.filter((e) => e.seq > s.lastSeq).sort((a, b) => a.seq - b.seq);
      if (fresh.length === 0) return s;
      return {
        events: [...fresh.reverse(), ...s.events].slice(0, MAX_ACTIVITY_EVENTS),
        lastSeq: Math.max(s.lastSeq, ...fresh.map((e) => e.seq)),
      };
    }),
}));
