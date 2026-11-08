import { lazy, Suspense, useEffect, type ComponentType } from "react";
import { useGlobalShortcuts } from "@/hooks/useGlobalShortcuts";
import { startMonitoring } from "@/services/monitoring";
import { Toaster } from "@/components/Toaster";
import { ConfirmActionDialog } from "@/features/processes/ConfirmActionDialog";
import { CommandPalette } from "./CommandPalette";
import { useUiStore } from "@/stores/uiStore";
import { ActivityPage } from "@/features/activity/ActivityPage";
import { AlertsPage } from "@/features/alerts/AlertsPage";
import { CpuPage } from "@/features/cpu/CpuPage";
import { FileFinderPage } from "@/features/file-finder/FileFinderPage";
import { MemoryPage } from "@/features/memory/MemoryPage";
import { NetworkPage } from "@/features/network/NetworkPage";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { PerformancePage } from "@/features/performance/PerformancePage";
import { PortsPage } from "@/features/ports/PortsPage";
import { ProcessDetailsPage } from "@/features/process-details/ProcessDetailsPage";
import { ProcessesPage } from "@/features/processes/ProcessesPage";
import { ServicesPage } from "@/features/services/ServicesPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { StartupPage } from "@/features/startup/StartupPage";
import { StoragePage } from "@/features/storage/StoragePage";
import { SystemInfoPage } from "@/features/system-info/SystemInfoPage";
import type { RouteId } from "./routes";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { useApplyTheme } from "./theme";

const BenchmarksPage = lazy(() =>
  import("@/features/benchmarks/BenchmarksPage").then((m) => ({ default: m.BenchmarksPage })),
);

const PAGES: Record<RouteId, ComponentType> = {
  overview: OverviewPage,
  processes: ProcessesPage,
  performance: PerformancePage,
  cpu: CpuPage,
  memory: MemoryPage,
  storage: StoragePage,
  network: NetworkPage,
  "process-details": ProcessDetailsPage,
  ports: PortsPage,
  services: ServicesPage,
  startup: StartupPage,
  "file-finder": FileFinderPage,
  activity: ActivityPage,
  system: SystemInfoPage,
  alerts: AlertsPage,
  benchmarks: BenchmarksPage,
  settings: SettingsPage,
};

export function App() {
  useApplyTheme();
  useGlobalShortcuts();
  useEffect(() => {
    void startMonitoring();
  }, []);
  const route = useUiStore((s) => s.route);
  const Page = PAGES[route];

  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar />
        <main className="min-h-0 flex-1 overflow-auto p-5">
          <Suspense fallback={<div role="status">Loading page…</div>}>
            <Page />
          </Suspense>
        </main>
      </div>
      <ConfirmActionDialog />
      <CommandPalette />
      <Toaster />
    </div>
  );
}
