import { describe, expect, it } from "vitest";
import { makeProcessRow } from "@/test/fixtures";
import type { ProcessPoint, ThreadInfo } from "@/types";
import { diagnose, type DiagnoseInput } from "./diagnose";

const points = (n: number, cpu: number, io = 0, memory = 100 * 1024 ** 2): ProcessPoint[] =>
  Array.from({ length: n }, (_, i) => ({
    t: 1_000_000 + i * 1000,
    cpu,
    memory,
    read: io,
    write: 0,
  }));

const thread = (
  tid: number,
  state: ThreadInfo["state"],
  waitChannel: string | null = null,
): ThreadInfo => ({
  tid,
  name: null,
  state,
  cpuPercent: 0,
  cpuTimeMs: 0,
  priority: 0,
  startTime: null,
  processor: null,
  waitChannel,
});

const base = (over: Partial<DiagnoseInput> = {}): DiagnoseInput => ({
  row: makeProcessRow(),
  history: points(46, 0),
  threads: null,
  openFiles: null,
  connections: undefined,
  childCount: 0,
  events: [],
  ...over,
});

const texts = (input: DiagnoseInput) => diagnose(input).map((f) => f.text);

describe("diagnose", () => {
  it("states idle CPU and disk facts", () => {
    const t = texts(base());
    expect(t).toContain("Used less than 0.1% CPU over the last 45s.");
    expect(t).toContain("No disk activity over the last 45s.");
  });

  it("describes busy processes with an average and peak", () => {
    const history = points(30, 20).map((p, i) => ({ ...p, cpu: i === 3 ? 90 : 20 }));
    const t = texts(base({ history, row: makeProcessRow({ diskReadPerSec: 1024 }) }));
    expect(t.some((x) => x.startsWith("Averaged 22.3% CPU") && x.includes("peak 90.0%"))).toBe(
      true,
    );
  });

  it("counts waiting threads and common wait channels", () => {
    const threads = {
      sampledAtMs: 0,
      threads: [
        ...Array.from({ length: 19 }, (_, i) =>
          thread(i, "sleeping", i < 15 ? "futex_wait_queue" : "do_epoll_wait"),
        ),
        thread(100, "running"),
        thread(101, "diskSleep", "io_schedule"),
      ],
    };
    const findings = diagnose(base({ threads }));
    const text = findings.map((f) => f.text);
    expect(text).toContain("20 of 21 threads are waiting (sleeping or in disk wait).");
    expect(text).toContain("1 thread is in uninterruptible disk wait.");
    expect(text.find((x) => x.startsWith("Waiting in:"))).toContain("futex_wait_queue (15)");
  });

  it("only says 'consistent with idle or blocked' and never claims a hang or deadlock", () => {
    const threads = {
      sampledAtMs: 0,
      threads: Array.from({ length: 10 }, (_, i) => thread(i, "sleeping")),
    };
    const all = texts(base({ threads })).join(" ");
    expect(all).toContain("consistent with a process that is idle or blocked");
    expect(all).toContain("does not by itself show that the process is hung");
    expect(all.toLowerCase()).not.toContain("deadlock");
    // Any real activity removes the pattern.
    const busy = texts(base({ threads, history: points(46, 5) })).join(" ");
    expect(busy).not.toContain("consistent with");
  });

  it("reports suspended processes, restricted access and short history", () => {
    expect(texts(base({ row: makeProcessRow({ status: "stopped" }) }))[0]).toContain("suspended");
    expect(texts(base({ row: makeProcessRow({ restricted: true }) })).join(" ")).toContain(
      "restricts access",
    );
    expect(texts(base({ history: points(2, 0) })).join(" ")).toContain("Not enough history");
  });

  it("summarises memory change, sockets, descriptors and growth events", () => {
    const history = points(40, 0).map((p, i) => ({ ...p, memory: (100 + i * 5) * 1024 ** 2 }));
    const t = texts(
      base({
        history,
        childCount: 2,
        connections: [
          {
            protocol: "tcp",
            localAddress: "a",
            localPort: 1,
            remoteAddress: "b",
            remotePort: 2,
            state: "established",
          },
          {
            protocol: "tcp",
            localAddress: "a",
            localPort: 3,
            remoteAddress: null,
            remotePort: null,
            state: "listen",
          },
        ],
        openFiles: {
          truncated: false,
          files: [
            { descriptor: "1", kind: "file", target: "/a", access: "read" },
            { descriptor: "2", kind: "socket", target: "socket:[1]", access: null },
          ],
        },
        events: [
          {
            seq: 1,
            timestampMs: 0,
            kind: "memoryGrowth",
            pid: 1,
            name: "x",
            detail:
              "Sustained memory growth: 100 MB to 400 MB over 12 minutes. A growing cache can be normal.",
          },
        ],
      }),
    );
    expect(t).toContain("2 network sockets, 1 established.");
    expect(t).toContain("2 open descriptors, 1 of them files or folders.");
    expect(t).toContain("2 child processes.");
    expect(t.some((x) => x.startsWith("Memory rose from"))).toBe(true);
    expect(t.some((x) => x.startsWith("Sustained memory growth"))).toBe(true);
  });
});
