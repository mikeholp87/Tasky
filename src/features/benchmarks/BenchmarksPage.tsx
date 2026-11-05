import { Check, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { Card, CardHeader } from "@/components/ui/Card";
import { cn } from "@/lib/cn";
import { isBackendAvailable } from "@/services/ipc";
import {
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
  WorkloadId,
} from "@/types";
import { formatTimestamp } from "@/utils/format";
import { compareScores, previousComparable } from "./compare";
import { DiskCard } from "./DiskCard";
import { formatSpread, formatThroughput } from "./format";
import { InternetCard } from "./InternetCard";
import { STABILITY } from "./stability";
import {
  EnvironmentFooter,
  Outcome,
  ProfilePicker,
  Progress,
  ResultNotes,
  RunRow,
  Score,
  StabilityLabel,
  Sustained,
} from "./parts";

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
  const diskResult = useBenchmarkStore((s) => shownResultFor(s, "disk"));
  const internetResult = useBenchmarkStore((s) => shownResultFor(s, "internet"));
  const select = useBenchmarkStore((s) => s.select);
  const selectedId = useBenchmarkStore((s) => s.selectedId);

  useEffect(() => {
    void initBenchmarks();
    void loadGpuAdapters();
  }, []);

  const development =
    status.developmentBuild ||
    cpuResult?.environment.developmentBuild === true ||
    gpuResult?.environment.developmentBuild === true ||
    diskResult?.environment.developmentBuild === true;
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
          <DiskCard result={diskResult} history={history} />
          <InternetCard result={internetResult} history={history} />
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
              onRun={() => void runBenchmark("gpu", profile, { adapter: adapterId })}
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
                    <Check size="0.875rem" aria-hidden /> Verified
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
      <p className="text-[0.8125rem] text-fg-subtle">
        {a.name} · {DEVICE_TYPE[a.deviceType]} · {a.backend} · {a.driver} {a.driverInfo}
        <br />
        Method {result.method.methodId}, scoring {result.method.scoreVersion}. Timed with{" "}
        {gpu.timingMethod === "gpuTimestamps" ? "GPU timestamps" : "the wall clock"}. Outliers are
        flagged and kept, never removed.
      </p>
    </section>
  );
}

const KIND_NAME: Record<BenchmarkKind, string> = {
  cpu: "CPU",
  gpu: "GPU",
  disk: "Disk",
  internet: "Internet",
};

function scoreText(r: BenchmarkResult): string {
  if (r.kind === "disk") return r.diskScore != null ? `Disk ${r.diskScore}` : "Not scored";
  if (r.kind === "internet") {
    const d = r.internet?.download?.medianMbps;
    const u = r.internet?.upload?.medianMbps;
    return d != null && u != null
      ? `↓ ${formatThroughput(d, "Mbit/s")} ↑ ${formatThroughput(u, "Mbit/s")}`
      : "No speed";
  }
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
                  <span className="w-16 shrink-0 font-medium">{KIND_NAME[r.kind]}</span>
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
                  <Trash2 size="0.875rem" aria-hidden />
                </button>
              </li>
            ))}
          </ul>
        )}
        <p className="pt-2 text-[0.8125rem] text-fg-subtle">
          CPU scores are single-core / multi-core.
        </p>
      </div>
    </Card>
  );
}
