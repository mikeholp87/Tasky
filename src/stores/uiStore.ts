import { create } from "zustand";
import type { RouteId } from "@/app/routes";

interface UiState {
  route: RouteId;
  /** PID shown on the Process Details page. */
  selectedPid: number | null;
  /** Theme actually applied (system preference resolved). */
  resolvedTheme: "light" | "dark";
  /** Whether the primary navigation sidebar is collapsed to an icon rail. */
  sidebarCollapsed: boolean;
  navigate: (route: RouteId) => void;
  openProcess: (pid: number) => void;
  setResolvedTheme: (theme: "light" | "dark") => void;
  toggleSidebar: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  route: "overview",
  selectedPid: null,
  resolvedTheme: "light",
  sidebarCollapsed: true,
  navigate: (route) => set({ route }),
  openProcess: (pid) => set({ selectedPid: pid, route: "process-details" }),
  setResolvedTheme: (resolvedTheme) => set({ resolvedTheme }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
}));
