import { ChevronRight, Play } from "lucide-react";
import { Card, CardHeader } from "@/components/ui/Card";
import { isTerminal, useBenchmarkStore } from "@/stores/benchmarkStore";
import type { BenchmarkKind, BenchmarkSummary } from "@/types";
import { KIND_ICONS, KIND_LABELS, formatMetricText, formatScore } from "../format";
import { linkButton, secondaryButton } from "./styles";

/** Headline metrics shown on the overview card for each category. */
const HEADLINE: Record<BenchmarkKind, string[]> = {
  cpu: ["cpu.single.integer", "cpu.multi.integer", "cpu.scaling"],
  gpu: ["gpu.compute.fma", "gpu.memory.copy"],
  disk: ["disk.seq.read", "disk.seq.write", "disk.rand.read.qd1"],
  network: ["net.download", "net.upload", "net.latency.idle", "net.jitter.idle"],
};

interface Props {
  kind: BenchmarkKind;
  latest?: BenchmarkSummary;
}

export function CategoryCard({ kind, latest }: Props) {
  const run = useBenchmarkStore((s) => s.run);
  const requestLaunch = useBenchmarkStore((s) => s.requestLaunch);
  const setView = useBenchmarkStore((s) => s.setView);
  const Icon = KIND_ICONS[kind];
  const running = run !== null && !isTerminal(run.phase);
  const activeHere = running && run.progress?.kind === kind;
  const category = latest?.categories.find((c) => c.kind === kind);
  const metrics = category
    ? HEADLINE[kind]
        .map((id) => category.metrics.find((m) => m.id === id))
        .filter((m) => m !== undefined)
    : [];
  const label = KIND_LABELS[kind];

  return (
    <Card className="flex flex-col">
      <CardHeader
        title={label}
        icon={<Icon size={14} aria-hidden />}
        action={
          <span className="text-[12px] text-fg-muted">
            {activeHere ? "Running" : running ? "Waiting" : "Ready"}
          </span>
        }
      />
      <div className="flex-1 px-4 py-3">
        {metrics.length > 0 ? (
          <dl className="num space-y-1">
            {metrics.map((m) => (
              <div key={m.id} className="flex justify-between gap-3">
                <dt className="text-fg-muted">{m.label}</dt>
                <dd className="font-medium">{formatMetricText(m.value, m.unit)}</dd>
              </div>
            ))}
            {kind !== "network" && category?.score != null && (
              <div className="flex justify-between gap-3">
                <dt className="text-fg-muted">Score</dt>
                <dd className="font-medium">{formatScore(category.score)}</dd>
              </div>
            )}
          </dl>
        ) : (
          <p className="text-fg-muted">No previous result.</p>
        )}
      </div>
      <div className="flex items-center justify-between gap-2 border-t border-border px-4 py-2">
        <button
          type="button"
          className={secondaryButton}
          disabled={running}
          onClick={() => requestLaunch([kind])}
        >
          <Play size={14} aria-hidden />
          Run {label} test
        </button>
        <button
          type="button"
          className={`${linkButton} flex items-center gap-0.5`}
          onClick={() => setView({ name: "category", kind })}
          aria-label={`${label} benchmark details`}
        >
          Details
          <ChevronRight size={14} aria-hidden />
        </button>
      </div>
    </Card>
  );
}
