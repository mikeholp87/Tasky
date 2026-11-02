import { create } from "zustand";
import type { AlertEvent } from "@/types";

const MAX_EVENTS = 100;

interface AlertState {
  /** Fired alerts, newest first. */
  events: AlertEvent[];
  add: (events: readonly AlertEvent[]) => void;
  replace: (events: readonly AlertEvent[]) => void;
}

export const useAlertStore = create<AlertState>((set) => ({
  events: [],
  add: (events) =>
    set((s) => ({ events: [...[...events].reverse(), ...s.events].slice(0, MAX_EVENTS) })),
  replace: (events) => set({ events: [...events].reverse().slice(0, MAX_EVENTS) }),
}));
