import type { InterfaceKind, NetworkInterfaceSnapshot } from "@/types";

export const INTERFACE_KIND_LABEL: Record<InterfaceKind, string | null> = {
  ethernet: "Ethernet",
  wifi: "Wi-Fi",
  vpn: "VPN / tunnel",
  virtual: "Virtual",
  loopback: "Loopback",
  unknown: null,
};

/** Interfaces to list; bridges, veth and TAP devices only when asked for. */
export function visibleInterfaces(
  list: readonly NetworkInterfaceSnapshot[],
  showVirtual: boolean,
): NetworkInterfaceSnapshot[] {
  return list.filter((n) => showVirtual || n.kind !== "virtual");
}

/** Seconds since boot, derived from the static boot time and the latest tick. */
export function uptimeSeconds(bootTimeSecs: number | null, nowMs: number): number | null {
  if (bootTimeSecs == null || bootTimeSecs <= 0) return null;
  return Math.max(0, Math.floor(nowMs / 1000 - bootTimeSecs));
}
