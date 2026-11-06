/**
 * One-line summaries shown under each section name in the System Information
 * list, so the list can be scanned without opening anything.
 */
import type {
  DiskSnapshot,
  GpuSnapshot,
  HardwareInfo,
  MemorySnapshot,
  NetworkInterfaceSnapshot,
  StaticSystemInfo,
  SensorReading,
} from "@/types";
import { formatBytes, formatDuration, formatPercent } from "@/utils/format";
import { uptimeSeconds, visibleInterfaces } from "@/utils/system";
import { describeScreen, type Screen } from "./format";

export const osSummary = (i: StaticSystemInfo): string =>
  i.distribution ?? i.longOsVersion ?? i.osName ?? "Unknown";

export function machineSummary(hw: HardwareInfo | null): string {
  if (!hw) return "Loading…";
  const { vendor, model } = hw.machine;
  const text = [vendor, model].filter(Boolean).join(" ");
  return text || "Not reported";
}

export const processorSummary = (i: StaticSystemInfo): string =>
  `${i.cpuBrand || "Unknown processor"} · ${i.logicalCoreCount} threads`;

export const memorySummary = (m: MemorySnapshot | undefined): string =>
  m ? `${formatBytes(m.usedBytes)} of ${formatBytes(m.totalBytes)} in use` : "…";

export function graphicsSummary(gpus: readonly GpuSnapshot[] | undefined): string {
  if (!gpus) return "…";
  if (gpus.length === 0) return "None detected";
  const [first] = gpus;
  return gpus.length === 1 ? (first?.name ?? "") : `${first?.name} and ${gpus.length - 1} more`;
}

export function storageSummary(disks: readonly DiskSnapshot[] | undefined): string {
  if (!disks) return "…";
  const total = disks.reduce((sum, d) => sum + d.totalBytes, 0);
  return `${disks.length} ${disks.length === 1 ? "volume" : "volumes"} · ${formatBytes(total)}`;
}

export function networkSummary(
  nets: readonly NetworkInterfaceSnapshot[] | undefined,
  showVirtual: boolean,
): string {
  if (!nets) return "…";
  const shown = visibleInterfaces(nets, showVirtual);
  const primary = shown.find((n) => n.isPrimary) ?? shown[0];
  if (!primary) return "No adapters";
  return `${primary.name}${primary.ipv4[0] ? ` · ${primary.ipv4[0]}` : ""}`;
}

export function powerSummary(hw: HardwareInfo | null): string {
  if (!hw) return "Loading…";
  const battery = hw.batteries[0];
  if (battery?.percent != null) return `Battery ${battery.percent}%`;
  if (hw.powerSource === "ac") return "Mains power";
  return hw.batteries.length === 0 ? "No battery" : "Battery";
}

export function sensorsSummary(temps: readonly SensorReading[] | undefined): string {
  if (!temps) return "…";
  if (temps.length === 0) return "No sensors";
  return `Hottest ${Math.round(Math.max(...temps.map((t) => t.celsius)))} °C`;
}

export const displaySummary = (s: Screen): string => describeScreen(s);

export function sessionSummary(i: StaticSystemInfo, nowMs: number): string {
  const up = uptimeSeconds(i.bootTimeSecs, nowMs);
  return up == null ? "Uptime unavailable" : `Up ${formatDuration(up)}`;
}

export const appSummary = (i: StaticSystemInfo): string => `Version ${i.appVersion}`;

/** Memory in use as a percentage, for the memory detail pane. */
export const usedLabel = (m: MemorySnapshot): string =>
  `${formatBytes(m.usedBytes)} (${formatPercent(m.usedPercent, 0)})`;
