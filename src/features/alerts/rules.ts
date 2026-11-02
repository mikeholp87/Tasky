/**
 * Alert rule helpers for the UI. The backend validates again; these give
 * immediate feedback and human-readable descriptions.
 */
import type { AlertMetric, AlertRule } from "@/types";

const GIB = 1024 ** 3;

export const METRICS: readonly { id: AlertMetric; label: string; unit: "%" | "GB" }[] = [
  { id: "systemCpu", label: "System CPU", unit: "%" },
  { id: "systemMemory", label: "System memory", unit: "%" },
  { id: "diskUsage", label: "Drive capacity used", unit: "%" },
  { id: "processCpu", label: "Process CPU", unit: "%" },
  { id: "processMemory", label: "Process memory", unit: "GB" },
];

export const isProcessMetric = (m: AlertMetric) => m === "processCpu" || m === "processMemory";

export function metricInfo(m: AlertMetric) {
  return METRICS.find((x) => x.id === m) as (typeof METRICS)[number];
}

/** Threshold as the user edits it (GB for process memory, else percent). */
export function displayThreshold(rule: Pick<AlertRule, "metric" | "threshold">): number {
  return rule.metric === "processMemory"
    ? Math.round((rule.threshold / GIB) * 100) / 100
    : rule.threshold;
}

export function storedThreshold(metric: AlertMetric, display: number): number {
  return metric === "processMemory" ? Math.round(display * GIB) : display;
}

/** "IF node CPU > 80% FOR 20 s" style summary. */
export function describeRule(rule: AlertRule): string {
  const info = metricInfo(rule.metric);
  const subject = isProcessMetric(rule.metric)
    ? `${rule.processName || "?"} ${rule.metric === "processCpu" ? "CPU" : "memory"}`
    : info.label;
  const value = `${displayThreshold(rule)}${info.unit === "%" ? "%" : " GB"}`;
  const duration = rule.durationSecs > 0 ? ` for ${rule.durationSecs} s` : "";
  return `${subject} above ${value}${duration}`;
}

export function newRule(): AlertRule {
  return {
    id:
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `rule-${Date.now()}-${Math.random().toString(36).slice(2)}`,
    name: "",
    enabled: true,
    metric: "processCpu",
    threshold: 80,
    durationSecs: 20,
    processName: "",
  };
}

/** Returns a user-facing problem with the rule, or null when it is valid. */
export function ruleProblem(rule: AlertRule): string | null {
  if (!rule.name.trim()) return "Give the alert a name.";
  if (!Number.isFinite(rule.threshold) || rule.threshold < 0) return "Enter a positive threshold.";
  if (rule.metric !== "processMemory" && rule.threshold > 100) return "Percentages go up to 100.";
  if (!Number.isInteger(rule.durationSecs) || rule.durationSecs < 0 || rule.durationSecs > 3600) {
    return "Duration must be between 0 and 3600 seconds.";
  }
  if (isProcessMetric(rule.metric) && !rule.processName?.trim()) return "Enter a process name.";
  return null;
}

/** Normalizes before saving: process name only for process metrics. */
export function finalizeRule(rule: AlertRule): AlertRule {
  return {
    ...rule,
    name: rule.name.trim(),
    processName: isProcessMetric(rule.metric) ? (rule.processName ?? "").trim() : null,
  };
}
