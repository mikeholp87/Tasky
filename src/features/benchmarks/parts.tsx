import { Play, Square } from "lucide-react";
import { Meter } from "@/components/ui/Meter";
import { SegmentedControl } from "@/components/ui/SegmentedControl";
import { cn } from "@/lib/cn";
import { cancelBenchmark } from "@/services/benchmarks";
import { isActive, useBenchmarkStore } from "@/stores/benchmarkStore";
import type { BenchmarkKind, BenchmarkProfile, BenchmarkResult, Stability } from "@/types";
import { formatDuration, formatTimestamp } from "@/utils/format";
import { compareScores, DIFFERENT_METHOD_NOTE } from "./compare";
import { formatDelta, formatThroughput } from "./format";
import { STABILITY } from "./stability";

/** Pieces shared by the benchmark cards. */

// --------------------------------------------------------------- shared

export function ProfilePicker({
  profiles,
  value,
  onChange,
  disabled,
}: {
  profiles: readonly { id: BenchmarkProfile; label: string }[];
  value: BenchmarkProfile;
  onChange: (p: BenchmarkProfile) => void;
  disabled: boolean;
}) {
  return (
    <SegmentedControl
      label="Benchmark profile"
      value={value}
      onChange={onChange}
      disabled={disabled}
      segments={profiles.map((p) => ({ value: p.id, label: p.label }))}
      className="inline-flex gap-1 rounded-lg p-1"
      itemClassName="rounded-md px-3 py-1.5 disabled:opacity-60"
    />
  );
}

/** Run, or Cancel when this card's benchmark is the one running. */
export function RunRow({
  active,
  mine,
  detail,
  label,
  onRun,
}: {
  active: boolean;
  mine: boolean;
  detail: string | undefined;
  label: string;
  onRun: () => void;
}) {
  const cancelling = useBenchmarkStore((s) => s.cancelling);
  return (
    <div className="flex flex-wrap items-center gap-3">
      {mine ? (
        <button
          type="button"
          onClick={() => void cancelBenchmark()}
          disabled={cancelling}
          className="flex h-9 items-center gap-2 rounded-lg border border-border px-4 font-medium hover:bg-surface-2"
        >
          <Square size="0.875rem" aria-hidden /> {cancelling ? "Cancelling…" : "Cancel"}
        </button>
      ) : (
        <button
          type="button"
          disabled={active}
          onClick={onRun}
          title={active ? "Another benchmark is running." : undefined}
          className="flex h-9 items-center gap-2 rounded-lg bg-accent px-4 font-medium text-accent-fg hover:opacity-90 disabled:opacity-50"
        >
          <Play size="0.875rem" aria-hidden /> {label}
        </button>
      )}
      <span className="text-fg-muted">{detail}</span>
    </div>
  );
}

/** Terminal outcome, including completed runs that could not be saved. */
export function Outcome({ kind }: { kind: BenchmarkKind }) {
  const status = useBenchmarkStore((s) => s.status);
  if (isActive(status) || status.kind !== kind) return null;
  if (status.state === "cancelled" && status.message) {
    return <p className="text-fg-muted">{status.message}</p>;
  }
  if (status.state === "failed" || (status.state === "completed" && status.message)) {
    return (
      <p role="alert" className={status.state === "failed" ? "text-crit" : "text-warn"}>
        {status.message ?? "The benchmark failed."}
      </p>
    );
  }
  return null;
}

const PROGRESS_COLOR: Record<BenchmarkKind, string> = {
  cpu: "--res-cpu",
  gpu: "--res-net",
  disk: "--res-disk",
  internet: "--res-net",
};

const PROGRESS_NOTE: Partial<Record<BenchmarkKind, string>> = {
  gpu: "GPU utilization, clock speed and temperature are not measured.",
  disk: "Using a temporary file, which is deleted when the test ends.",
  internet: "Sending and receiving generated test data only.",
};

export function Progress({ kind }: { kind: BenchmarkKind }) {
  const s = useBenchmarkStore((st) => st.status);
  const cancelling = useBenchmarkStore((st) => st.cancelling);
  const pct = Math.round(s.fraction * 100);
  const dash = <span className="text-fg-subtle">—</span>;
  return (
    <div className="space-y-3 rounded-xl bg-surface-2 p-4">
      <div className="flex items-baseline justify-between gap-3">
        <span role="status" className="font-medium">
          {cancelling ? "Cancelling…" : s.stage || "Starting"}
        </span>
        <span className="num text-fg-muted">{pct}%</span>
      </div>
      <Meter percent={pct} label="Benchmark progress" color={PROGRESS_COLOR[kind]} />
      <div className="flex flex-wrap items-baseline gap-x-8 gap-y-1">
        <span className="num text-2xl font-medium">
          {s.live ? formatThroughput(s.live.value, s.live.unit) : dash}
        </span>
        <span className="num text-fg-muted">Elapsed {formatDuration(s.elapsedMs / 1000)}</span>
        {kind === "cpu" ? (
          <>
            <span className="num text-fg-muted">
              CPU {s.cpuPercent != null ? `${s.cpuPercent.toFixed(0)}%` : "—"}
            </span>
            <span className="num text-fg-muted">
              {s.frequencyMhz != null ? `${(s.frequencyMhz / 1000).toFixed(2)} GHz` : "Frequency —"}
            </span>
            <span className="num text-fg-muted">
              {s.temperatureC != null ? `${s.temperatureC.toFixed(0)} °C` : "Temperature —"}
            </span>
          </>
        ) : (
          <span className="text-fg-muted">{PROGRESS_NOTE[kind]}</span>
        )}
      </div>
    </div>
  );
}

export function StabilityLabel({ stability }: { stability: Stability }) {
  const s = STABILITY[stability];
  return (
    <span
      className={cn("font-medium", s.tone)}
      title="How repeatable this run looked, from the spread between iterations and any warnings. It is not a statistical confidence interval."
    >
      {s.label}
    </span>
  );
}

/** Not-scored message, comparison text and warnings shared by both results. */
const SAME_PLACE: Partial<Record<BenchmarkKind, string>> = {
  gpu: " on this adapter",
  disk: " on this drive",
  internet: " to this server",
};

export function ResultNotes({
  result,
  history,
  previous,
  scored,
  unscoredNote,
}: {
  result: BenchmarkResult;
  history: readonly BenchmarkResult[];
  previous: BenchmarkResult | null;
  scored: boolean;
  /** Replaces the default reason shown when there is no score. */
  unscoredNote?: string;
}) {
  const others = history.filter((h) => h.id !== result.id && h.kind === result.kind);
  const comparable = previous ? compareScores(result, previous) : null;
  return (
    <>
      {!scored && (
        <p className="text-fg-muted">
          {unscoredNote ??
            "Not scored: development builds and software renderers are not representative, or a measurement was unavailable."}
        </p>
      )}
      {comparable && previous && (
        <p className="text-fg-muted">
          Compared with your previous {result.profile} run on{" "}
          {formatTimestamp(previous.startedAtMs)}. One run is not proof of a real difference.
        </p>
      )}
      {previous === null && others.length > 0 && (
        <p className="text-fg-muted">
          No earlier {result.profile} result measured the same way
          {SAME_PLACE[result.kind] ?? ""} to compare with.
        </p>
      )}
      {others.some((h) => h.method.methodId !== result.method.methodId) && (
        <p className="text-fg-subtle">{DIFFERENT_METHOD_NOTE}</p>
      )}
      {result.warnings.length > 0 && (
        <ul className="space-y-1 rounded-lg bg-warn-soft px-3 py-2 text-warn">
          {result.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
    </>
  );
}

export function Sustained({
  title,
  peak,
  sustained,
  decline,
  note,
}: {
  title: string;
  peak: string;
  sustained: string;
  decline: number;
  note: string | null;
}) {
  return (
    <div className="space-y-1">
      <h3 className="font-medium">{title}</h3>
      <p className="num text-fg-muted">
        Peak {peak}, sustained {sustained}
        {decline >= 0.5 && ` (${decline.toFixed(0)}% lower than at the start)`}.
      </p>
      {note && <p className="text-fg-muted">{note}</p>}
    </div>
  );
}

export function EnvironmentFooter({ result }: { result: BenchmarkResult }) {
  const env = result.environment;
  return (
    <p className="text-[0.8125rem] text-fg-subtle">
      {[
        env.distribution ?? env.platform,
        env.cpuModel,
        `${env.logicalCores} threads`,
        env.powerSource === "ac" ? "AC power" : env.powerSource === "battery" ? "battery" : null,
        env.cpuGovernor ? `governor ${env.cpuGovernor}` : null,
      ]
        .filter(Boolean)
        .join(" · ")}
      <br />
      Method {result.method.methodId}, scoring {result.method.scoreVersion}.{" "}
      {result.method.comparability === "crossPlatform"
        ? "Comparable across Windows and Linux."
        : "Comparable only on the same platform."}{" "}
      {result.affinity === "not pinned" ? "" : `${result.affinity}. `}Outliers are flagged and kept,
      never removed.
    </p>
  );
}

export function Score({
  label,
  value,
  change,
}: {
  label: string;
  value: number | undefined;
  change: number | null | undefined;
}) {
  const delta = formatDelta(change ?? null);
  return (
    <div>
      <div className="text-fg-muted">{label}</div>
      <div className="flex items-baseline gap-2">
        <span className="num text-4xl font-medium leading-none tracking-tight">{value ?? "—"}</span>
        {delta && (
          <span
            className={cn(
              "num font-medium",
              (change ?? 0) > 0.05
                ? "text-ok"
                : (change ?? 0) < -0.05
                  ? "text-crit"
                  : "text-fg-muted",
            )}
          >
            {delta}
          </span>
        )}
      </div>
    </div>
  );
}
