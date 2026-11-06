import type { Protocol, SocketEntry, TcpState } from "@/types";

export type PortView = "listening" | "all";

export interface PortRow {
  key: string;
  port: number;
  protocol: Protocol;
  address: string;
  remote: string | null;
  state: TcpState | null;
  pid: number | null;
  processName: string | null;
  /** Accepting connections (TCP LISTEN) or bound and waiting (UDP). */
  listening: boolean;
}

export const STATE_LABEL: Record<TcpState, string> = {
  listen: "Listening",
  established: "Established",
  timeWait: "Time wait",
  closeWait: "Close wait",
  synSent: "SYN sent",
  synReceived: "SYN received",
  finWait1: "FIN wait 1",
  finWait2: "FIN wait 2",
  closing: "Closing",
  lastAck: "Last ACK",
  closed: "Closed",
  unknown: "Unknown",
};

export const PROTOCOL_LABEL: Record<Protocol, string> = {
  tcp: "TCP",
  tcp6: "TCP6",
  udp: "UDP",
  udp6: "UDP6",
};

export function endpoint(address: string, port: number | null): string {
  const host = address.includes(":") ? `[${address}]` : address;
  return port == null ? host : `${host}:${port}`;
}

export function toRows(sockets: readonly SocketEntry[]): PortRow[] {
  return sockets.map((s, i) => {
    const c = s.connection;
    const tcp = c.protocol === "tcp" || c.protocol === "tcp6";
    return {
      key: `${i}:${c.protocol}:${c.localAddress}:${c.localPort}:${c.remotePort ?? ""}`,
      port: c.localPort,
      protocol: c.protocol,
      address: c.localAddress,
      remote: c.remoteAddress != null ? endpoint(c.remoteAddress, c.remotePort) : null,
      state: c.state,
      pid: s.pid,
      processName: s.processName,
      listening: tcp ? c.state === "listen" : true,
    };
  });
}

/** The query as a port number, when it is one. */
export function asPort(query: string): number | null {
  const q = query.trim();
  if (!/^\d{1,5}$/.test(q)) return null;
  const n = Number(q);
  return n >= 1 && n <= 65_535 ? n : null;
}

/**
 * Rows for the chosen view and search. A numeric query matches a port by
 * prefix (so "80" finds 80 and 8080) or a PID exactly; text matches the
 * process name, address, protocol or state.
 */
export function filterRows(rows: readonly PortRow[], view: PortView, query: string): PortRow[] {
  const q = query.trim().toLowerCase();
  const numeric = /^\d+$/.test(q);
  return rows
    .filter((r) => view === "all" || r.listening)
    .filter((r) => {
      if (!q) return true;
      if (numeric) return String(r.port).startsWith(q) || String(r.pid) === q;
      return (
        (r.processName?.toLowerCase().includes(q) ?? false) ||
        r.address.toLowerCase().includes(q) ||
        PROTOCOL_LABEL[r.protocol].toLowerCase() === q ||
        (r.state != null && STATE_LABEL[r.state].toLowerCase().includes(q))
      );
    })
    .sort((a, b) => a.port - b.port || a.protocol.localeCompare(b.protocol));
}

export interface PortOwner {
  pid: number | null;
  name: string | null;
  /** Distinct "protocol address" listeners of this process on the port. */
  listeners: string[];
}

/**
 * Who is listening on exactly `port`: one entry per owning process, so a
 * server bound on IPv4 and IPv6 is one owner. Owners are only reported for
 * listening sockets; a closing connection on the port is not a conflict.
 */
export function portOwners(rows: readonly PortRow[], port: number): PortOwner[] {
  const owners = new Map<string, PortOwner>();
  for (const r of rows) {
    if (r.port !== port || !r.listening) continue;
    const key = r.pid == null ? "unknown" : String(r.pid);
    const owner = owners.get(key) ?? { pid: r.pid, name: r.processName, listeners: [] };
    owner.listeners.push(`${PROTOCOL_LABEL[r.protocol]} ${endpoint(r.address, null)}`);
    owners.set(key, owner);
  }
  return [...owners.values()];
}
