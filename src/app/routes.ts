import {
  Activity,
  Bell,
  Cpu,
  FileSearch,
  Gauge,
  HardDrive,
  Info,
  LayoutDashboard,
  ListTree,
  MemoryStick,
  Network,
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
}

export const ROUTES: readonly RouteDef[] = [
  { id: "overview", label: "Overview", icon: LayoutDashboard, group: "main" },
  { id: "processes", label: "Processes", icon: ListTree, group: "main" },
  { id: "performance", label: "Performance", icon: Activity, group: "main" },
  { id: "cpu", label: "CPU", icon: Cpu, group: "resources" },
  { id: "memory", label: "Memory", icon: MemoryStick, group: "resources" },
  { id: "storage", label: "Storage", icon: HardDrive, group: "resources" },
  { id: "network", label: "Network", icon: Network, group: "resources" },
  { id: "process-details", label: "Process Details", icon: FileSearch, group: "tools" },
  { id: "system", label: "System Information", icon: Info, group: "tools" },
  { id: "benchmarks", label: "Benchmarks", icon: Timer, group: "tools" },
  { id: "alerts", label: "Alerts", icon: Bell, group: "tools" },
  { id: "settings", label: "Settings", icon: Settings, group: "tools" },
] as const;

export const ROUTE_ICON_FALLBACK = Gauge;
