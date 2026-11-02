/**
 * Deterministic mock process generator for tests and the developer
 * performance mode. Never used for real data.
 */
import type { ProcessRow } from "@/types";

const NAMES = [
  "chrome",
  "code",
  "node",
  "python",
  "docker",
  "postgres",
  "nginx",
  "systemd",
  "explorer",
  "svchost",
  "brave",
  "claude",
  "rust-analyzer",
  "java",
  "slack",
  "zoom",
  "bash",
  "pwsh",
];
const USERS = ["alice", "root", "SYSTEM", "www-data"];

/** Small seeded PRNG (mulberry32) so mock data is reproducible. */
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function mockProcesses(count: number, nowSecs: number, seed = 42): ProcessRow[] {
  const rand = rng(seed);
  const rows: ProcessRow[] = [];
  for (let i = 0; i < count; i++) {
    const pid = 100 + i * 3;
    const name = NAMES[i % NAMES.length] as string;
    rows.push({
      pid,
      parentPid: i === 0 ? null : 100 + Math.floor(rand() * i) * 3,
      name,
      status: rand() < 0.01 ? "stopped" : rand() < 0.3 ? "running" : "sleeping",
      cpuPercent: rand() < 0.9 ? rand() * 3 : rand() * 90,
      memoryBytes: Math.floor(rand() ** 3 * 4 * 1024 ** 3),
      virtualMemoryBytes: Math.floor(rand() * 16 * 1024 ** 3),
      diskReadPerSec: rand() < 0.95 ? 0 : rand() * 50 * 1024 ** 2,
      diskWritePerSec: rand() < 0.95 ? 0 : rand() * 20 * 1024 ** 2,
      user: USERS[Math.floor(rand() * USERS.length)] as string,
      startTime: nowSecs - Math.floor(rand() * 7 * 86_400),
      threads: 1 + Math.floor(rand() * 64),
      priority: 0,
      exe: `/usr/bin/${name}`,
      cmd: `${name} --flag-${i} --config /etc/${name}/${i}.conf`,
      sessionId: 1,
    });
  }
  return rows;
}
