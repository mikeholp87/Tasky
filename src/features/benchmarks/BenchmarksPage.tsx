import { Play, Square, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { Card, CardHeader } from "@/components/ui/Card";
import { Meter } from "@/components/ui/Meter";
import { cn } from "@/lib/cn";
import { isBackendAvailable } from "@/services/ipc";
import {
  cancelBenchmark,
  deleteBenchmarkResult,
  initBenchmarks,
  runCpuBenchmark,
} from "@/services/benchmarks";
import { isActive, shownResult, useBenchmarkStore } from "@/stores/benchmarkStore";
import type { BenchmarkProfile, BenchmarkResult, Stability, WorkloadId } from "@/types";
import { formatDuration, formatTimestamp } from "@/utils/format";
import { compareScores, DIFFERENT_METHOD_NOTE, previousComparable } from "./compare";
import { formatDelta, formatSpread, formatThroughput } from "./format";

const PROFILES: readonly { id: BenchmarkProfile; label: string; detail: string }[] = [
  { id: "quick", label: "Quick", detail: "About 10 seconds. A fast check." },
  { id: "standard", label: "Standard", detail: "About 30 seconds. The default." },
  {
    id: "extended",
    label: "Extended",
    detail: "About 70 seconds, including a 30-second sustained-load test.",
  },
];

const WORKLOADS: readonly { id: WorkloadId; label: string }[] = [
  { id: "integer", label: "Integer" },
  { id: "floatingPoint", label: "Floating point" },
  { id: "hash", label: "SHA-256 hashing" },
  { id: "compress", label: "Compression" },
  { id: "decompress", label: "Decompression" },
];

const STABILITY: Record<Stability, { label: string; tone: string }> = {
  high: { label: "High stability", tone: "text-ok" },
  medium: { label: "Medium stability", tone: "text-warn" },
  low: { label: "Low stability", tone: "text-crit" },
};

export function BenchmarksPage() {
  const status = useBenchmarkStore((s) => s.status);
  const history = useBenchmarkStore((s) => s.history);
  const result = useBenchmarkStore(shownResult);
  const select = useBenchmarkStore((s) => s.select);
  const [profile, setProfile] = useState<BenchmarkProfile>("standard");
  const active = isActive(status);

  useEffect(() => {
    void initBenchmarks();
  }, []);

  const development = status.developmentBuild || result?.environment.developmentBuild === true;

  return (
    <>
      <PageHeader
        title="Benchmarks"
        description="Measure how fast this computer is and compare results over time."
      />
      {!isBackendAvailable() ? (
        <ConnectionNotice />
      ) : (
        <div className="max-w-5xl space-y-4">
          {development && (
            <p
              role="note"
              className="rounded-xl border border-border bg-warn-soft px-4 py-3 text-warn"
            >
              Development build: benchmark scores are not representative, so no score is calculated
              and results are not saved. Use the installed release build to measure.
            </p>
          )}
          <Card>
            <CardHeader title="CPU" />
            <div className="space-y-4 px-4 pb-4 pt-3">
              <ProfilePicker value={profile} onChange={setProfile} disabled={active} />
              <div className="flex items-center gap-3">
                {active ? (
                  <button
                    type="button"
                    onClick={() => void cancelBenchmark()}
                    className="flex h-9 items-center gap-2 rounded-lg border border-border px-4 font-medium hover:bg-surface-2"
                  >
                    <Square size={14} aria-hidden /> Cancel
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => void runCpuBenchmark(profile)}
                    className="flex h-9 items-center gap-2 rounded-lg bg-accent px-4 font-medium text-accent-fg hover:opacity-90"
                  >
                    <Play size={14} aria-hidden /> Run CPU benchmark
                  </button>
                )}
                <span className="text-fg-muted">
                  {PROFILES.find((p) => p.id === profile)?.detail}
                </span>
              </div>
              {active && <Progress />}
              {!active && status.state === "cancelled" && status.message && (
                <p className="text-fg-muted">{status.message}</p>
              )}
              {!active && status.state === "failed" && (
                <p role="alert" className="text-crit">
                  {status.message ?? "The benchmark failed."}
                </p>
              )}
              {!active && result && <ResultPanel result={result} history={history} />}
              {!active && !result && status.state !== "failed" && (
                <p className="text-fg-muted">
                  Run the benchmark to measure single-core and multi-core speed. Close other
                  applications first for the most repeatable result.
                </p>
              )}
            </div>
          </Card>

          <div className="grid gap-4 md:grid-cols-3">
            {["GPU", "Storage", "Internet"].map((name) => (
              <Card key={name}>
                <CardHeader title={name} />
                <p className="px-4 pb-4 pt-2 text-fg-muted">Not available yet.</p>
              </Card>
            ))}
          </div>

          <HistoryCard history={history} openId={result?.id ?? null} onOpen={select} />
        </div>
      )}
    </>
  );
}

function ProfilePicker({
  value,
  onChange,
  disabled,
}: {
  value: BenchmarkProfile;
  onChange: (p: BenchmarkProfile) => void;
  disabled: boolean;
}) {
  return (
    <div
      role="radiogroup"
      aria-label="Benchmark profile"
      className="inline-flex gap-1 rounded-lg bg-surface-2 p-1"
    >
      {PROFILES.map((p) => (
        <button
          key={p.id}
          type="button"
          role="radio"
          aria-checked={value === p.id}
          disabled={disabled}
          onClick={() => onChange(p.id)}
          className={cn(
            "rounded-md px-3 py-1.5 text-fg-muted disabled:opacity-60",
            value === p.id && "bg-surface text-fg shadow-sm",
          )}
        >
          {p.label}
        </button>
      ))}
    </div>
  );
}

function Progress() {
  const s = useBenchmarkStore((st) => st.status);
  const pct = Math.round(s.fraction * 100);
  const dash = <span className="text-fg-subtle">—</span>;
  return (
    <div className="space-y-3 rounded-xl bg-surface-2 p-4">
      <div className="flex items-baseline justify-between gap-3">
        <span role="status" className="font-medium">
          {s.stage || "Starting"}
        </span>
        <span className="num text-fg-muted">{pct}%</span>
      </div>
      <Meter percent={pct} label="Benchmark progress" color="--res-cpu" />
      <div className="flex flex-wrap items-baseline gap-x-8 gap-y-1">
        <span className="num text-2xl font-medium">
          {s.live ? formatThroughput(s.live.value, s.live.unit) : dash}
        </span>
        <span className="num text-fg-muted">Elapsed {formatDuration(s.elapsedMs / 1000)}</span>
        <span className="num text-fg-muted">
          CPU {s.cpuPercent != null ? `${s.cpuPercent.toFixed(0)}%` : "—"}
        </span>
        <span className="num text-fg-muted">
          {s.frequencyMhz != null ? `${(s.frequencyMhz / 1000).toFixed(2)} GHz` : "Frequency —"}
        </span>
        <span className="num text-fg-muted">
          {s.temperatureC != null ? `${s.temperatureC.toFixed(0)} °C` : "Temperature —"}
        </span>
      </div>
    </div>
  );
}

function ResultPanel({
  result,
  history,
}: {
  result: BenchmarkResult;
  history: readonly BenchmarkResult[];
}) {
  const previous = previousComparable(history, result);
  const delta = previous ? compareScores(result, previous) : null;
  const stability = STABILITY[result.stability];
  const env = result.environment;
  const byKey = (id: WorkloadId, mode: "singleCore" | "multiCore") =>
    result.workloads.find((w) => w.id === id && w.mode === mode);

  return (
    <section aria-label="Latest CPU result" className="space-y-4 border-t border-border pt-4">
      <div className="flex flex-wrap items-end gap-x-10 gap-y-3">
        <Score label="Single-core" value={result.scores?.singleCore} change={delta?.singleCore} />
        <Score label="Multi-core" value={result.scores?.multiCore} change={delta?.multiCore} />
        <span
          className={cn("font-medium", stability.tone)}
          title="How repeatable this run looked, from the spread between iterations and any warnings. It is not a statistical confidence interval."
        >
          {stability.label}
        </span>
      </div>
      {!result.scores && (
        <p className="text-fg-muted">
          Not scored: development builds are not representative, or a measurement was unavailable.
        </p>
      )}
      {delta && previous && (
        <p className="text-fg-muted">
          Compared with your previous {result.profile} run on{" "}
          {formatTimestamp(previous.startedAtMs)}. One run is not proof of a real difference.
        </p>
      )}
      {previous === null && history.some((h) => h.id !== result.id) && (
        <p className="text-fg-muted">
          No earlier {result.profile} result measured the same way to compare with.
        </p>
      )}
      {history.some((h) => h.id !== result.id && h.method.methodId !== result.method.methodId) && (
        <p className="text-fg-subtle">{DIFFERENT_METHOD_NOTE}</p>
      )}
      {result.warnings.length > 0 && (
        <ul className="space-y-1 rounded-lg bg-warn-soft px-3 py-2 text-warn">
          {result.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}

      <table className="w-full text-left">
        <caption className="sr-only">Measurements by workload (median of all iterations)</caption>
        <thead>
          <tr className="text-fg-muted">
            <th scope="col" className="py-1.5 pr-4 font-medium">
              Workload
            </th>
            <th scope="col" className="py-1.5 pr-4 text-right font-medium">
              Single-core
            </th>
            <th scope="col" className="py-1.5 text-right font-medium">
              Multi-core
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {WORKLOADS.map(({ id, label }) => (
            <tr key={id}>
              <th scope="row" className="py-1.5 pr-4 font-normal">
                {label}
              </th>
              {(["singleCore", "multiCore"] as const).map((mode) => {
                const w = byKey(id, mode);
                return (
                  <td key={mode} className="num py-1.5 text-right">
                    {w ? (
                      <>
                        <span className="font-medium">{formatThroughput(w.median, w.unit)}</span>{" "}
                        <span className="text-fg-subtle">{formatSpread(w)}</span>
                        {w.outlierIndices.length > 0 && (
                          <span className="text-fg-subtle">
                            {" "}
                            · {w.outlierIndices.length} outlier
                          </span>
                        )}
                      </>
                    ) : (
                      "—"
                    )}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>

      {result.sustained && (
        <div className="space-y-1">
          <h3 className="font-medium">Sustained multi-core load</h3>
          <p className="num text-fg-muted">
            Peak {formatThroughput(result.sustained.peakMops, "Mops/s")}, sustained{" "}
            {formatThroughput(result.sustained.sustainedMops, "Mops/s")}
            {result.sustained.declinePercent >= 0.5 &&
              ` (${result.sustained.declinePercent.toFixed(0)}% lower than at the start)`}
            .
          </p>
          {result.sustained.note && <p className="text-fg-muted">{result.sustained.note}</p>}
        </div>
      )}

      <p className="text-[12px] text-fg-subtle">
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
        {result.affinity === "not pinned" ? "" : `${result.affinity}. `}Outliers are flagged and
        kept, never removed.
      </p>
    </section>
  );
}

function Score({
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

function HistoryCard({
  history,
  openId,
  onOpen,
}: {
  history: readonly BenchmarkResult[];
  openId: string | null;
  onOpen: (id: string) => void;
}) {
  return (
    <Card>
      <CardHeader title="History" />
      <div className="px-4 pb-4 pt-2">
        {history.length === 0 ? (
          <p className="text-fg-muted">Saved results appear here after a run finishes.</p>
        ) : (
          <ul className="divide-y divide-border">
            {history.slice(0, 20).map((r) => (
              <li key={r.id} className="flex items-center gap-3 py-1.5">
                <button
                  type="button"
                  onClick={() => onOpen(r.id)}
                  aria-current={r.id === openId ? "true" : undefined}
                  className={cn(
                    "flex min-w-0 flex-1 items-center gap-4 rounded-md px-2 py-1 text-left hover:bg-surface-2",
                    r.id === openId && "bg-surface-2",
                  )}
                >
                  <span className="num w-40 shrink-0 text-fg-muted">
                    {formatTimestamp(r.startedAtMs)}
                  </span>
                  <span className="w-20 shrink-0 capitalize">{r.profile}</span>
                  <span className="num">
                    {r.scores ? `${r.scores.singleCore} / ${r.scores.multiCore}` : "Not scored"}
                  </span>
                  <span className={cn("ml-auto", STABILITY[r.stability].tone)}>
                    {STABILITY[r.stability].label}
                  </span>
                </button>
                <button
                  type="button"
                  onClick={() => void deleteBenchmarkResult(r.id)}
                  aria-label={`Delete the result from ${formatTimestamp(r.startedAtMs)}`}
                  className="rounded-md p-1.5 text-fg-muted hover:bg-surface-2 hover:text-crit"
                >
                  <Trash2 size={14} aria-hidden />
                </button>
              </li>
            ))}
          </ul>
        )}
        <p className="pt-2 text-[12px] text-fg-subtle">Scores are single-core / multi-core.</p>
      </div>
    </Card>
  );
}
