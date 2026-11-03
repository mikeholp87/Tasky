import { FastForward, Square } from "lucide-react";
import { Card } from "@/components/ui/Card";
import { cancelRun, loadBenchmarkData, skipCooldown } from "@/services/benchmarks";
import { backend, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import { isTerminal, useBenchmarkStore } from "@/stores/benchmarkStore";
import { KIND_LABELS, PHASE_LABELS, formatElapsed, formatMetricText } from "../format";
import { secondaryButton } from "./styles";

/** Live status of the active run, with Cancel. Renders nothing when idle. */
export function RunPanel() {
  const run = useBenchmarkStore((s) => s.run);
  const orphan = useBenchmarkStore((s) => s.capabilities?.runningRunId ?? null);
  if (!run || isTerminal(run.phase)) {
    // A run started before the window reloaded keeps going in the backend;
    // offer a way to stop it even though its progress cannot be shown.
    return orphan && orphan !== run?.runId ? <OrphanRun runId={orphan} /> : null;
  }
  const p = run.progress;
  const percent = Math.round(p?.percent ?? 0);
  const title = p?.kind ? `${KIND_LABELS[p.kind]} benchmark` : "Benchmark";
  const status = [
    PHASE_LABELS[run.phase],
    p?.subtest,
    p?.iteration != null && p.totalIterations != null
      ? `Iteration ${p.iteration} of ${p.totalIterations}`
      : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <Card className="mb-4 p-4" aria-label="Running benchmark">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
            {title}
          </h2>
          <p className="mt-1 font-medium" aria-live="polite">
            {status}
          </p>
          {p?.message && <p className="mt-0.5 text-fg-muted">{p.message}</p>}
        </div>
        <div className="flex gap-2">
          {run.phase === "coolingDown" && (
            <button type="button" className={secondaryButton} onClick={() => void skipCooldown()}>
              <FastForward size={14} aria-hidden />
              Skip cooldown
            </button>
          )}
          <button type="button" className={secondaryButton} onClick={() => void cancelRun()}>
            <Square size={14} aria-hidden />
            Cancel benchmark
          </button>
        </div>
      </div>
      <div
        role="progressbar"
        aria-label={`${title} progress`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        aria-valuetext={`${percent}% — ${status}`}
        className="mt-3 h-2 overflow-hidden rounded-full bg-surface-2"
      >
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-200"
          style={{ width: `${percent}%` }}
        />
      </div>
      <dl className="num mt-3 grid grid-cols-2 gap-x-6 gap-y-1 sm:grid-cols-3">
        <div>
          <dt className="text-fg-muted">Progress</dt>
          <dd className="font-medium">{percent}%</dd>
        </div>
        <div>
          <dt className="text-fg-muted">Elapsed</dt>
          <dd className="font-medium">{formatElapsed(p?.elapsedMs ?? 0)}</dd>
        </div>
        {p?.measurement && (
          <div>
            <dt className="text-fg-muted">{p.measurement.label}</dt>
            <dd className="font-medium">
              {formatMetricText(p.measurement.value, p.measurement.unit)}
            </dd>
          </div>
        )}
      </dl>
    </Card>
  );
}

function OrphanRun({ runId }: { runId: string }) {
  const stop = async () => {
    try {
      await backend.cancelBenchmark(runId);
    } catch (e) {
      toast.error(toAppError(e).message);
    }
    // Give the backend a moment to clean up, then refresh.
    setTimeout(() => void loadBenchmarkData(), 1500);
  };
  return (
    <Card className="mb-4 flex flex-wrap items-center justify-between gap-3 p-4" role="status">
      <p>A benchmark started earlier is still running. Its progress cannot be shown here.</p>
      <button type="button" className={secondaryButton} onClick={() => void stop()}>
        <Square size={14} aria-hidden />
        Cancel benchmark
      </button>
    </Card>
  );
}
