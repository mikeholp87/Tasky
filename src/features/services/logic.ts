import type { ServiceInfo } from "@/types";

export type ServiceFilter = "all" | "running" | "failed" | "stopped";

/** Plain words for systemd's states, so status never relies on colour. */
export function stateLabel(s: Pick<ServiceInfo, "activeState" | "subState">): string {
  switch (s.activeState) {
    case "active":
      return s.subState === "running" ? "Running" : "Active";
    case "activating":
      return "Starting";
    case "deactivating":
      return "Stopping";
    case "failed":
      return "Failed";
    case "reloading":
      return "Reloading";
    default:
      return "Stopped";
  }
}

const isRunning = (s: ServiceInfo) => s.activeState === "active" || s.activeState === "activating";

export function filterServices(
  items: readonly ServiceInfo[],
  query: string,
  filter: ServiceFilter,
): ServiceInfo[] {
  const q = query.trim().toLowerCase();
  return items.filter((s) => {
    if (filter === "running" && !isRunning(s)) return false;
    if (filter === "failed" && s.activeState !== "failed") return false;
    if (filter === "stopped" && (isRunning(s) || s.activeState === "failed")) return false;
    return (
      q === "" ||
      s.unit.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q) ||
      String(s.mainPid ?? "").includes(q)
    );
  });
}
