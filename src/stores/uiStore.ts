import { create } from "zustand";
import { useMetricsStore } from "./metricsStore";
import { createJSONStorage, persist } from "zustand/middleware";
import { ROUTES, type RouteId } from "@/app/routes";

interface UiState {
  route: RouteId;
  /** PID shown on the Process Details page. */
  selectedPid: number | null;
  selectedStartTime: number | null;
  /** Theme actually applied (system preference resolved). */
  resolvedTheme: "light" | "dark";
  navigate: (route: RouteId) => void;
  openProcess: (pid: number, startTime?: number) => void;
  setResolvedTheme: (theme: "light" | "dark") => void;
}

/** Process Details needs a PID, which does not survive a restart, so it is not restored. */
const restorable = (id: unknown): id is RouteId =>
  ROUTES.some((r) => r.id === id) && id !== "process-details";

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      route: "overview",
      selectedPid: null,
      selectedStartTime: null,
      resolvedTheme: "light",
      navigate: (route) => set({ route }),
      openProcess: (pid, startTime) =>
        set({
          selectedPid: pid,
          selectedStartTime:
            startTime ??
            useMetricsStore.getState().tick?.processes.find((p) => p.pid === pid)?.startTime ??
            null,
          route: "process-details",
        }),
      setResolvedTheme: (resolvedTheme) => set({ resolvedTheme }),
    }),
    {
      name: "tm:ui",
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({ route: s.route }),
      // Keep only a known page; anything else falls back to Overview.
      merge: (saved, current) => {
        const route = (saved as { route?: unknown } | undefined)?.route;
        return { ...current, ...(restorable(route) ? { route } : {}) };
      },
    },
  ),
);
