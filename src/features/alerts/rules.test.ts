import { describe, expect, it } from "vitest";
import type { AlertRule } from "@/types";
import {
  describeRule,
  displayThreshold,
  finalizeRule,
  newRule,
  ruleProblem,
  storedThreshold,
} from "./rules";

const base: AlertRule = {
  id: "x",
  name: "Node hot",
  enabled: true,
  metric: "processCpu",
  threshold: 80,
  durationSecs: 20,
  processName: "node",
};

describe("alert rules", () => {
  it("describes rules like the brief's IF/FOR example", () => {
    expect(describeRule(base)).toBe("node CPU above 80% for 20 s");
    expect(describeRule({ ...base, metric: "systemMemory", threshold: 85, durationSecs: 0 })).toBe(
      "System memory above 85%",
    );
  });

  it("edits process memory in GB but stores bytes", () => {
    expect(storedThreshold("processMemory", 4)).toBe(4 * 1024 ** 3);
    expect(displayThreshold({ metric: "processMemory", threshold: 4 * 1024 ** 3 })).toBe(4);
    expect(storedThreshold("systemCpu", 90)).toBe(90);
  });

  it("flags invalid input", () => {
    expect(ruleProblem(base)).toBeNull();
    expect(ruleProblem({ ...base, name: " " })).toMatch(/name/);
    expect(ruleProblem({ ...base, threshold: 150 })).toMatch(/100/);
    expect(ruleProblem({ ...base, processName: "" })).toMatch(/process name/);
    expect(ruleProblem({ ...base, durationSecs: 9000 })).toMatch(/3600/);
  });

  it("drops the process name for system metrics", () => {
    expect(finalizeRule({ ...base, metric: "systemCpu" }).processName).toBeNull();
    expect(finalizeRule({ ...base, name: "  Spaced  " }).name).toBe("Spaced");
  });

  it("creates unique ids", () => {
    expect(newRule().id).not.toBe(newRule().id);
  });
});
