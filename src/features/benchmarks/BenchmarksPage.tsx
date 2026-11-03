import { Check, Play, Square, Trash2 } from "lucide-react";
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
  loadGpuAdapters,
  runBenchmark,
} from "@/services/benchmarks";
import { isActive, shownResultFor, useBenchmarkStore } from "@/stores/benchmarkStore";
import type {
  BenchmarkKind,
  BenchmarkProfile,
  BenchmarkResult,
  GpuDeviceType,
  GpuResult,
  Stability,
  WorkloadId,
} from "@/types";
import { formatDuration, formatTimestamp } from "@/utils/format";
import { compareScores, DIFFERENT_METHOD_NOTE, previousComparable } from "./compare";
import { formatDelta, formatSpread, formatThroughput } from "./format";

const CPU_PROFILES: readonly { id: BenchmarkProfile; label: string; detail: string }[] = [
  { id: "quick", label: "Quick", detail: "About 10 seconds. A fast check." },
  { id: "standard", label: "Standard", detail: "About 30 seconds. The default." },
  {
    id: "extended",
    label: "Extended",
    detail: "About 70 seconds, including a 30-second sustained-load test.",
  },
];

const GPU_PROFILES: readonly { id: BenchmarkProfile; label: string; detail: string }[] = [
  { id: "quick", label: "Quick", detail: "About 5 seconds. A fast check." },
  { id: "standard", label: "Standard", detail: "About 10 seconds. The default." },
  {
    id: "extended",
    label: "Extended",
    detail: "About 40 seconds, including a 30-second sustained-load test.",
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

const DEVICE_TYPE: Record<GpuDeviceType, string> = {
  discrete: "Discrete GPU",
  integrated: "Integrated GPU",
  virtual: "Virtual GPU",
  software: "Software renderer (runs on the CPU)",
  other: "Graphics device",
};

export function BenchmarksPage() {
  const status = useBenchmarkStore((s) => s.status);
  const history = useBenchmarkStore((s) => s.history);
  const cpuResult = useBenchmarkStore((s) => shownResultFor(s, "cpu"));
  const gpuResult = useBenchmarkStore((s) => shownResultFor(s, "gpu"));
  const select = useBenchmarkStore((s) => s.select);
  const selectedId = useBenchmarkStore((s) => s.selectedId);

  useEffect(() => {
    void initBenchmarks();
    void loadGpuAdapters();
  }, []);

  const development =
    status.developmentBuild ||
    cpuResult?.environment.developmentBuild === true ||
    gpuResult?.environment.developmentBuild === true;
  const openId = selectedId ?? status.runId ?? null;

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
          <CpuCard result={cpuResult} history={history} />
          <GpuCard result={gpuResult} history={history} />
          <div className="grid gap-4 md:grid-cols-2">
            {["Storage", "Internet"].map((name) => (
              <Card key={name}>
                <CardHeader title={name} />
                <p className="px-4 pb-4 pt-2 text-fg-muted">Not available yet.</p>
              </Card>
            ))}
          </div>
          <HistoryCard history={history} openId={openId} onOpen={select} />
        </div>
      )}
    </>
  );
}

// ------------------------------------------------------------------ CPU

function CpuCard({
  result,
  history,
}: {
  result: BenchmarkResult | null;
  history: readonly BenchmarkResult[];
}) {
  const status = useBenchmarkStore((s) => s.status);
  const [profile, setProfile] = useState<BenchmarkProfile>("standard");
  const active = isActive(status);
  const mine = active && status.kind === "cpu";
  return (
    <Card>
      <CardHeader title="CPU" />
      <div className="space-y-4 px-4 pb-4 pt-3">
        <ProfilePicker
          profiles={CPU_PROFILES}
          value={profile}
          onChange={setProfile}
          disabled={active}
        />
        <RunRow
          active={active}
          mine={mine}
          detail={CPU_PROFILES.find((p) => p.id === profile)?.detail}
          label="Run CPU benchmark"
          onRun={() => void runBenchmark("cpu", profile)}
        />
        {mine && <Progress kind="cpu" />}
        <Outcome kind="cpu" />
        {!mine && result && <CpuResultPanel result={result} history={history} />}
        {!mine && !result && status.state !== "failed" && (
          <p className="text-fg-muted">
            Run the benchmark to measure single-core and multi-core speed. Close other applications
            first for the most repeatable result.
          </p>
        )}
      </div>
    </Card>
  );
}

function CpuResultPanel({
  result,
  history,
}: {
  result: BenchmarkResult;
  history: readonly BenchmarkResult[];
}) {
  const previous = previousComparable(history, result);
  const delta = previous ? compareScores(result, previous) : null;
  const byKey = (id: WorkloadId, mode: "singleCore" | "multiCore") =>
    result.workloads.find((w) => w.id === id && w.mode === mode);

  return (
    <section aria-label="Latest CPU result" className="space-y-4 border-t border-border pt-4">
      <div className="flex flex-wrap items-end gap-x-10 gap-y-3">
        <Score label="Single-core" value={result.scores?.singleCore} change={delta?.singleCore} />
        <Score label="Multi-core" value={result.scores?.multiCore} change={delta?.multiCore} />
        <StabilityLabel stability={result.stability} />
      </div>
      <ResultNotes result={result} history={history} previous={previous} scored={!!result.scores} />

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
        <Sustained
          title="Sustained multi-core load"
          peak={formatThroughput(result.sustained.peakMops, "Mops/s")}
          sustained={formatThroughput(result.sustained.sustainedMops, "Mops/s")}
          decline={result.sustained.declinePercent}
          note={result.sustained.note}
        />
      )}
      <EnvironmentFooter result={result} />
    </section>
  );
}

// ------------------------------------------------------------------ GPU

function GpuCard({
  result,
  history,
}: {
  result: BenchmarkResult | null;
  history: readonly BenchmarkResult[];
}) {
  const status = useBenchmarkStore((s) => s.status);
  const adapters = useBenchmarkStore((s) => s.adapters);
  const adaptersError = useBenchmarkStore((s) => s.adaptersError);
  const [profile, setProfile] = useState<BenchmarkProfile>("standard");
  const [chosen, setChosen] = useState<string | null>(null);
  const active = isActive(status);
  const mine = active && status.kind === "gpu";

  const recommended = adapters?.find((a) => a.recommended) ?? adapters?.[0];
  const adapterId = chosen ?? recommended?.id;
  const adapter = adapters?.find((a) => a.id === adapterId);
  const noAdapter = adapters !== null && adapters.length === 0;

  return (
    <Card>
      <CardHeader title="GPU" />
      <div className="space-y-4 px-4 pb-4 pt-3">
        {adapters === null && <p className="text-fg-muted">Looking for graphics adapters…</p>}
        {noAdapter && (
          <p role="alert" className="text-fg-muted">
            {adaptersError ??
              "No graphics adapter was found. The GPU benchmark needs a working Vulkan driver."}
          </p>
        )}
        {adapters !== null && adapters.length > 0 && (
          <>
            <div className="flex flex-wrap items-center gap-3">
              <label htmlFor="gpu-adapter" className="text-fg-muted">
                Adapter
              </label>
              <select
                id="gpu-adapter"
                value={adapterId ?? ""}
                disabled={active}
                onChange={(e) => setChosen(e.target.value)}
                className="h-8 max-w-full rounded-md border border-border bg-surface px-2 disabled:opacity-60"
              >
                {adapters.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name} · {DEVICE_TYPE[a.deviceType]}
                    {a.recommended ? " · recommended" : ""}
                  </option>
                ))}
              </select>
            </div>
            {adapter?.deviceType === "software" && (
              <p className="text-warn">
                This adapter is a software renderer that runs on the CPU. It can be tested, but the
                result is not a GPU score and is not scored.
              </p>
            )}
            <ProfilePicker
              profiles={GPU_PROFILES}
              value={profile}
              onChange={setProfile}
              disabled={active}
            />
            <RunRow
              active={active}
              mine={mine}
              detail={GPU_PROFILES.find((p) => p.id === profile)?.detail}
              label="Run GPU benchmark"
              onRun={() => void runBenchmark("gpu", profile, adapterId)}
            />
          </>
        )}
        {mine && <Progress kind="gpu" />}
        <Outcome kind="gpu" />
        {!mine && result?.gpu && (
          <GpuResultPanel result={result} gpu={result.gpu} history={history} />
        )}
        {!mine && !result && !noAdapter && adapters !== null && status.state !== "failed" && (
          <p className="text-fg-muted">
            Run the benchmark to measure compute and memory speed. Close games and video playback
            first: other programs share the GPU.
          </p>
        )}
      </div>
    </Card>
  );
}

function GpuResultPanel({
  result,
  gpu,
  history,
}: {
  result: BenchmarkResult;
  gpu: GpuResult;
  history: readonly BenchmarkResult[];
}) {
  const previous = previousComparable(history, result);
  const delta = previous ? compareScores(result, previous) : null;
  const a = gpu.adapter;
  return (
    <section aria-label="Latest GPU result" className="space-y-4 border-t border-border pt-4">
      <div className="flex flex-wrap items-end gap-x-10 gap-y-3">
        <Score label="GPU score" value={result.gpuScore ?? undefined} change={delta?.gpu} />
        <StabilityLabel stability={result.stability} />
      </div>
      <ResultNotes
        result={result}
        history={history}
        previous={previous}
        scored={result.gpuScore != null}
      />

      <table className="w-full text-left">
        <caption className="sr-only">GPU measurements (median of all iterations)</caption>
        <thead>
          <tr className="text-fg-muted">
            <th scope="col" className="py-1.5 pr-4 font-medium">
              Workload
            </th>
            <th scope="col" className="py-1.5 pr-4 text-right font-medium">
              Result
            </th>
            <th scope="col" className="py-1.5 text-right font-medium">
              Checked
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {gpu.workloads.map((w) => (
            <tr key={w.id}>
              <th scope="row" className="py-1.5 pr-4 font-normal">
                {w.label}
              </th>
              <td className="num py-1.5 pr-4 text-right">
                <span className="font-medium">{formatThroughput(w.median, w.unit)}</span>{" "}
                <span className="text-fg-subtle">{formatSpread(w)}</span>
                {w.outlierIndices.length > 0 && (
                  <span className="text-fg-subtle"> · {w.outlierIndices.length} outlier</span>
                )}
              </td>
              <td className="py-1.5 text-right">
                {w.validated ? (
                  <span
                    className="inline-flex items-center gap-1 text-ok"
                    title="The output matched a CPU reference calculation."
                  >
                    <Check size={14} aria-hidden /> Verified
                  </span>
                ) : (
                  <span className="text-fg-subtle">Not checked</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      {gpu.sustained && (
        <Sustained
          title="Sustained floating-point load"
          peak={formatThroughput(gpu.sustained.peakMops / 1000, "GFLOPS")}
          sustained={formatThroughput(gpu.sustained.sustainedMops / 1000, "GFLOPS")}
          decline={gpu.sustained.declinePercent}
          note={gpu.sustained.note}
        />
      )}
      {gpu.notes.length > 0 && (
        <ul className="space-y-1 text-fg-muted">
          {gpu.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
      <p className="text-[12px] text-fg-subtle">
        {a.name} · {DEVICE_TYPE[a.deviceType]} · {a.backend} · {a.driver} {a.driverInfo}
        <br />
        Method {result.method.methodId}, scoring {result.method.scoreVersion}. Timed with{" "}
        {gpu.timingMethod === "gpuTimestamps" ? "GPU timestamps" : "the wall clock"}. Outliers are
        flagged and kept, never removed.
      </p>
    </section>
  );
}

// --------------------------------------------------------------- shared

function ProfilePicker({
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
    <div
      role="radiogroup"
      aria-label="Benchmark profile"
      className="inline-flex gap-1 rounded-lg bg-surface-2 p-1"
    >
      {profiles.map((p) => (
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

/** Run, or Cancel when this card's benchmark is the one running. */
function RunRow({
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
  return (
    <div className="flex flex-wrap items-center gap-3">
      {mine ? (
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
          disabled={active}
          onClick={onRun}
          title={active ? "Another benchmark is running." : undefined}
          className="flex h-9 items-center gap-2 rounded-lg bg-accent px-4 font-medium text-accent-fg hover:opacity-90 disabled:opacity-50"
        >
          <Play size={14} aria-hidden /> {label}
        </button>
      )}
      <span className="text-fg-muted">{detail}</span>
    </div>
  );
}

/** Cancelled or failed outcome of the latest run of this kind. */
function Outcome({ kind }: { kind: BenchmarkKind }) {
  const status = useBenchmarkStore((s) => s.status);
  if (isActive(status) || status.kind !== kind) return null;
  if (status.state === "cancelled" && status.message) {
    return <p className="text-fg-muted">{status.message}</p>;
  }
  if (status.state === "failed") {
    return (
      <p role="alert" className="text-crit">
        {status.message ?? "The benchmark failed."}
      </p>
    );
  }
  return null;
}

function Progress({ kind }: { kind: BenchmarkKind }) {
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
      <Meter
        percent={pct}
        label="Benchmark progress"
        color={kind === "gpu" ? "--res-net" : "--res-cpu"}
      />
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
          <span className="text-fg-muted">
            GPU utilization, clock speed and temperature are not measured.
          </span>
        )}
      </div>
    </div>
  );
}

function StabilityLabel({ stability }: { stability: Stability }) {
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
function ResultNotes({
  result,
  history,
  previous,
  scored,
}: {
  result: BenchmarkResult;
  history: readonly BenchmarkResult[];
  previous: BenchmarkResult | null;
  scored: boolean;
}) {
  const others = history.filter((h) => h.id !== result.id && h.kind === result.kind);
  const comparable = previous ? compareScores(result, previous) : null;
  return (
    <>
      {!scored && (
        <p className="text-fg-muted">
          Not scored: development builds and software renderers are not representative, or a
          measurement was unavailable.
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
          {result.kind === "gpu" ? " on this adapter" : ""} to compare with.
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

function Sustained({
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

function EnvironmentFooter({ result }: { result: BenchmarkResult }) {
  const env = result.environment;
  return (
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
      {result.affinity === "not pinned" ? "" : `${result.affinity}. `}Outliers are flagged and kept,
      never removed.
    </p>
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

function scoreText(r: BenchmarkResult): string {
  if (r.kind === "gpu") return r.gpuScore != null ? `GPU ${r.gpuScore}` : "Not scored";
  return r.scores ? `${r.scores.singleCore} / ${r.scores.multiCore}` : "Not scored";
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
                  <span className="w-10 shrink-0 font-medium">
                    {r.kind === "gpu" ? "GPU" : "CPU"}
                  </span>
                  <span className="w-20 shrink-0 capitalize">{r.profile}</span>
                  <span className="num">{scoreText(r)}</span>
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
        <p className="pt-2 text-[12px] text-fg-subtle">CPU scores are single-core / multi-core.</p>
      </div>
    </Card>
  );
}
