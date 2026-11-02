import { describe, expect, it } from "vitest";
import { formatChange, formatElapsed, formatMetricText, formatScore } from "./format";

describe("benchmark formatting", () => {
  it("keeps storage MB/s and network Mbps distinct", () => {
    expect(formatMetricText(3481.4, "megabytesPerSec")).toBe("3,481 MB/s");
    expect(formatMetricText(487.2, "mbps")).toBe("487 Mbps");
    expect(formatMetricText(1250, "mbps")).toBe("1.25 Gbps");
  });

  it("scales compute and IOPS units", () => {
    expect(formatMetricText(14_800, "mops")).toBe("14.8 Gops/s");
    expect(formatMetricText(950, "mops")).toBe("950 Mops/s");
    expect(formatMetricText(487_000, "iops")).toBe("487K IOPS");
    expect(formatMetricText(812, "iops")).toBe("812 IOPS");
    expect(formatMetricText(8.384, "ratio")).toBe("8.38 ×");
  });

  it("formats latency with sensible precision", () => {
    expect(formatMetricText(18.04, "milliseconds")).toBe("18.0 ms");
    expect(formatMetricText(0.412, "milliseconds")).toBe("0.41 ms");
    expect(formatMetricText(85, "microseconds")).toBe("85 µs");
  });

  it("never formats missing values as zero", () => {
    expect(formatMetricText(null, "mbps")).toBe("—");
    expect(formatMetricText(Number.NaN, "iops")).toBe("—");
    expect(formatScore(null)).toBe("Not tested");
  });

  it("formats scores, changes and elapsed time", () => {
    expect(formatScore(13281.4)).toBe("13,281");
    expect(formatChange(4.12)).toBe("+4.1%");
    expect(formatChange(-2.5)).toBe("−2.5%");
    expect(formatElapsed(17_400)).toBe("00:17");
    expect(formatElapsed(3_725_000)).toBe("1:02:05");
  });
});
