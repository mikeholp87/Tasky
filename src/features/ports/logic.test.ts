import { describe, expect, it } from "vitest";
import type { Protocol, SocketEntry, TcpState } from "@/types";
import { asPort, filterRows, portOwners, toRows } from "./logic";

function socket(
  protocol: Protocol,
  localPort: number,
  state: TcpState | null,
  pid: number | null,
  processName: string | null,
  localAddress = "127.0.0.1",
): SocketEntry {
  return {
    connection: {
      protocol,
      localAddress,
      localPort,
      remoteAddress: state === "established" ? "10.0.0.5" : null,
      remotePort: state === "established" ? 443 : null,
      state,
    },
    pid,
    processName,
  };
}

const rows = toRows([
  socket("tcp", 3000, "listen", 28119, "node"),
  socket("tcp6", 3000, "listen", 28119, "node", "::1"),
  socket("tcp", 8080, "listen", 900, "java", "0.0.0.0"),
  socket("tcp", 80, "listen", 1, "nginx", "0.0.0.0"),
  socket("udp", 5353, null, 700, "avahi-daemon", "0.0.0.0"),
  socket("tcp", 40000, "established", 28119, "node"),
  socket("tcp", 3000, "timeWait", null, null),
]);

describe("ports", () => {
  it("lists only listening or bound sockets by default", () => {
    const shown = filterRows(rows, "listening", "");
    expect(shown.map((r) => r.port)).toEqual([80, 3000, 3000, 5353, 8080]);
    expect(filterRows(rows, "all", "")).toHaveLength(rows.length);
  });

  it("matches numeric queries by port prefix or exact PID", () => {
    expect(filterRows(rows, "listening", "80").map((r) => r.port)).toEqual([80, 8080]);
    expect(filterRows(rows, "listening", "900").map((r) => r.processName)).toEqual(["java"]);
    expect(filterRows(rows, "all", "40000")).toHaveLength(1);
  });

  it("matches text against process, protocol and state", () => {
    expect(filterRows(rows, "listening", "NODE")).toHaveLength(2);
    expect(filterRows(rows, "listening", "udp").map((r) => r.port)).toEqual([5353]);
    expect(filterRows(rows, "all", "time wait")).toHaveLength(1);
  });

  it("recognises port numbers", () => {
    expect(asPort(" 3000 ")).toBe(3000);
    expect(asPort("0")).toBeNull();
    expect(asPort("70000")).toBeNull();
    expect(asPort("node")).toBeNull();
  });

  it("reports one owner per process and ignores non-listening sockets", () => {
    const owners = portOwners(rows, 3000);
    expect(owners).toHaveLength(1);
    expect(owners[0]).toMatchObject({ pid: 28119, name: "node" });
    expect(owners[0]?.listeners).toEqual(["TCP 127.0.0.1", "TCP6 [::1]"]);
    expect(portOwners(rows, 9999)).toEqual([]);
    expect(portOwners(rows, 40000)).toEqual([]);
  });
});
