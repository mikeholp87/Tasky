import { create } from "zustand";
import type { RouteId } from "@/app/routes";

interface UiState {
  route: RouteId;
  /** PID shown on the Process Details page. */
  selectedPid: number | null;
  navigate: (route: RouteId) => void;
  openProcess: (pid: number) => void;
}

export const useUiStore = create<UiState>((set) => ({
  route: "overview",
  selectedPid: null,
  navigate: (route) => set({ route }),
  openProcess: (pid) => set({ selectedPid: pid, route: "process-details" }),
}));
