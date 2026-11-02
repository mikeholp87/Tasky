import type { MetricsTick } from "@/types";

/** Five minutes at 1 s. Per-device history starts when the app starts. */
export const DEVICE_HISTORY_POINTS = 300;

/** Column-oriented series for one device: timestamps plus named values. */
export interface DeviceSeries {
  t: number[];
  values: Record<string, number[]>;
}

/**
 * Bounded per-device history (disks by mount point, interfaces by name),
 * mutated in place each tick. Devices that disappear are dropped.
 */
export class DeviceHistory {
  private series = new Map<string, DeviceSeries>();

  constructor(private readonly capacity = DEVICE_HISTORY_POINTS) {}

  get(key: string): DeviceSeries | undefined {
    return this.series.get(key);
  }

  record(t: number, devices: Iterable<[string, Record<string, number>]>): void {
    const seen = new Set<string>();
    for (const [key, values] of devices) {
      seen.add(key);
      let s = this.series.get(key);
      if (!s) {
        s = { t: [], values: {} };
        this.series.set(key, s);
      }
      const last = s.t[s.t.length - 1];
      if (last !== undefined && t <= last) continue;
      s.t.push(t);
      for (const [name, v] of Object.entries(values)) {
        // A metric that appears later starts with zeros to stay aligned.
        const col = (s.values[name] ??= new Array(s.t.length - 1).fill(0));
        col.push(v);
      }
      if (s.t.length > this.capacity) {
        const drop = s.t.length - this.capacity;
        s.t.splice(0, drop);
        for (const col of Object.values(s.values)) col.splice(0, drop);
      }
    }
    for (const key of this.series.keys()) if (!seen.has(key)) this.series.delete(key);
  }
}

export const diskHistory = new DeviceHistory();
export const networkHistory = new DeviceHistory();

/** Feeds one tick into the per-device buffers. */
export function recordDevices(tick: MetricsTick): void {
  diskHistory.record(
    tick.timestampMs,
    tick.disks.map((d) => [
      d.mountPoint,
      {
        read: d.readBytesPerSec ?? 0,
        write: d.writeBytesPerSec ?? 0,
        active: d.activePercent ?? 0,
      },
    ]),
  );
  networkHistory.record(
    tick.timestampMs,
    tick.networks.map((n) => [n.name, { rx: n.rxBytesPerSec, tx: n.txBytesPerSec }]),
  );
}
