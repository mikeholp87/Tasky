import { describe, expect, it } from "vitest";
import { DeviceHistory } from "./deviceHistory";

describe("DeviceHistory", () => {
  it("records aligned columns per device and caps length", () => {
    const h = new DeviceHistory(2);
    h.record(1, [["C:\\", { read: 1, write: 2 }]]);
    h.record(2, [["C:\\", { read: 3, write: 4 }]]);
    h.record(3, [["C:\\", { read: 5, write: 6 }]]);
    expect(h.get("C:\\")).toEqual({ t: [2, 3], values: { read: [3, 5], write: [4, 6] } });
  });

  it("drops devices that disappear and ignores stale timestamps", () => {
    const h = new DeviceHistory(10);
    h.record(1, [
      ["eth0", { rx: 1 }],
      ["wlan0", { rx: 2 }],
    ]);
    h.record(1, [["eth0", { rx: 9 }]]);
    expect(h.get("wlan0")).toBeUndefined();
    expect(h.get("eth0")?.values.rx).toEqual([1]);
  });

  it("pads metrics that appear later so columns stay aligned", () => {
    const h = new DeviceHistory(10);
    h.record(1, [["d", { read: 1 }]]);
    h.record(2, [["d", { read: 2, active: 50 }]]);
    expect(h.get("d")?.values.active).toEqual([0, 50]);
  });
});
