import {
  Activity,
  Bell,
  Cpu,
  FileSearch,
  FolderSearch,
  Gauge,
  HardDrive,
  Info,
  LayoutDashboard,
  ListTree,
  MemoryStick,
  Network,
  Plug,
  Rocket,
  ScrollText,
  Settings,
  Timer,
  type LucideIcon,
} from "lucide-react";

export type RouteId =
  | "overview"
  | "processes"
  | "performance"
  | "cpu"
  | "memory"
  | "storage"
  | "network"
  | "process-details"
  | "ports"
  | "startup"
  | "file-finder"
  | "activity"
  | "system"
  | "alerts"
  | "benchmarks"
  | "settings";

export interface RouteDef {
  id: RouteId;
  label: string;
  icon: LucideIcon;
  /** Group used to visually separate sidebar sections. */
  group: "main" | "resources" | "tools";
  /**
   * Needs context (a chosen process), so it is reached from the page that
   * provides it and is not a standing menu entry.
   */
  contextual?: boolean;
}

export const ROUTES: readonly RouteDef[] = [
  { id: "overview", label: "Overview", icon: LayoutDashboard, group: "main" },
  { id: "processes", label: "Processes", icon: ListTree, group: "main" },
  { id: "performance", label: "Performance", icon: Activity, group: "main" },
  { id: "cpu", label: "CPU", icon: Cpu, group: "resources" },
  { id: "memory", label: "Memory", icon: MemoryStick, group: "resources" },
  { id: "storage", label: "Storage", icon: HardDrive, group: "resources" },
  { id: "network", label: "Network", icon: Network, group: "resources" },
  {
    id: "process-details",
    label: "Process Details",
    icon: FileSearch,
    group: "tools",
    contextual: true,
  },
  { id: "ports", label: "Ports", icon: Plug, group: "tools" },
  { id: "startup", label: "Startup", icon: Rocket, group: "tools" },
  { id: "file-finder", label: "Find File Users", icon: FolderSearch, group: "tools" },
  { id: "activity", label: "Activity", icon: ScrollText, group: "tools" },
  { id: "system", label: "System Information", icon: Info, group: "tools" },
  { id: "benchmarks", label: "Benchmarks", icon: Timer, group: "tools" },
  { id: "alerts", label: "Alerts", icon: Bell, group: "tools" },
  { id: "settings", label: "Settings", icon: Settings, group: "tools" },
] as const;

export const ROUTE_ICON_FALLBACK = Gauge;
