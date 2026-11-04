import { Check } from "lucide-react";
import { useEffect, useState } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { backend, toAppError } from "@/services/ipc";
import { loadDiskTargets, runBenchmark } from "@/services/benchmarks";
import { isActive, useBenchmarkStore } from "@/stores/benchmarkStore";
import type {
  BenchmarkProfile,
  BenchmarkResult,
  DiskPhaseResult,
  DiskPlan,
  DiskResult,
} from "@/types";
import { compareScores, previousComparable } from "./compare";
import { formatBytes, formatMicros, formatSpread, formatThroughput } from "./format";
import {
  EnvironmentFooter,
  Outcome,
  ProfilePicker,
  Progress,
  ResultNotes,
  RunRow,
  Score,
  StabilityLabel,
} from "./parts";

const DISK_PROFILES: readonly { id: BenchmarkProfile; label: string; detail: string }[] = [
  { id: "quick", label: "Quick", detail: "A few seconds on a fast drive. Writes about 0.3 GB." },
  { id: "standard", label: "Standard", detail: "Under a minute. The default." },
  { id: "extended", label: "Extended", detail: "Longer and larger. Writes several GB." },
];

export function DiskCard({
  result,
  history,
}: {
  result: BenchmarkResult | null;
  history: readonly BenchmarkResult[];
}) {
  const status = useBenchmarkStore((s) => s.status);
  const targets = useBenchmarkStore((s) => s.diskTargets);
  const targetsError = useBenchmarkStore((s) => s.diskTargetsError);
  const [profile, setProfile] = useState<BenchmarkProfile>("standard");
  const [chosen, setChosen] = useState<string | null>(null);
  const [plan, setPlan] = useState<DiskPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const active = isActive(status);
  const mine = active && status.kind === "disk";

  useEffect(() => {
    void loadDiskTargets();
  }, []);

  const recommended = targets?.find((t) => t.recommended) ?? targets?.[0];
  const targetId = chosen ?? recommended?.id;
  const target = targets?.find((t) => t.id === targetId);
  const none = targets !== null && targets.length === 0;

  // The plan states exactly what a run would write here, before it starts.
  useEffect(() => {
    if (!targetId) return;
    let stale = false;
    backend
      .diskPlan(targetId, profile)
      .then((p) => {
        if (stale) return;
        setPlan(p);
        setPlanError(null);
      })
      .catch((e) => {
        if (stale) return;
        setPlan(null);
        setPlanError(toAppError(e).message);
      });
    return () => {
      stale = true;
    };
  }, [targetId, profile]);

  const blocked = plan?.blocked ?? planError;

  return (
    <Card>
      <CardHeader title="Disk" />
      <div className="space-y-4 px-4 pb-4 pt-3">
        {targets === null && <p className="text-fg-muted">Looking for drives…</p>}
        {none && (
          <p role="alert" className="text-fg-muted">
            {targetsError ??
              "No writable drive location was found. The disk benchmark needs a folder you can write to."}
          </p>
        )}
        {targets !== null && targets.length > 0 && (
          <>
            <div className="flex flex-wrap items-center gap-3">
              <label htmlFor="disk-target" className="text-fg-muted">
                Location
              </label>
              <select
                id="disk-target"
                value={targetId ?? ""}
                disabled={active}
                onChange={(e) => setChosen(e.target.value)}
                className="h-8 max-w-full rounded-md border border-border bg-surface px-2 disabled:opacity-60"
              >
                {targets.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.label} · {t.fileSystem} · {formatBytes(t.freeBytes)} free
                  </option>
                ))}
              </select>
            </div>
            {target && target.notes.length > 0 && (
              <p className="text-warn">{target.notes.join(" ")}</p>
            )}
            <ProfilePicker
              profiles={DISK_PROFILES}
              value={profile}
              onChange={setProfile}
              disabled={active}
            />
            {plan && !plan.blocked && (
              <p className="text-fg-muted" data-testid="disk-plan">
                Creates a temporary {formatBytes(plan.fileBytes)} file in {plan.directory} and
                writes about {formatBytes(plan.estimatedBytesWritten)} to the drive, then deletes
                it. Roughly {plan.estimatedSeconds} seconds on a fast drive; slower drives take
                longer. Frequent runs add wear to flash storage.
              </p>
            )}
            {blocked && (
              <p role="alert" className="text-warn">
                {blocked}
              </p>
            )}
            <RunRow
              active={active || !!blocked || !plan}
              mine={mine}
              detail={DISK_PROFILES.find((p) => p.id === profile)?.detail}
              label="Run disk benchmark"
              onRun={() => void runBenchmark("disk", profile, { diskTarget: targetId })}
            />
          </>
        )}
        {mine && <Progress kind="disk" />}
        <Outcome kind="disk" />
        {!mine && result?.disk && (
          <DiskResultPanel result={result} disk={result.disk} history={history} />
        )}
        {!mine && !result && !none && targets !== null && status.state !== "failed" && (
          <p className="text-fg-muted">
            Run the benchmark to measure sequential and random speed and response time. Close
            programs that use the drive heavily first.
          </p>
        )}
      </div>
    </Card>
  );
}

function DiskResultPanel({
  result,
  disk,
  history,
}: {
  result: BenchmarkResult;
  disk: DiskResult;
  history: readonly BenchmarkResult[];
}) {
  const previous = previousComparable(history, result);
  const delta = previous ? compareScores(result, previous) : null;
  const direct = disk.ioMode === "direct";
  const d = disk.device;
  return (
    <section aria-label="Latest disk result" className="space-y-4 border-t border-border pt-4">
      <div className="flex flex-wrap items-end gap-x-10 gap-y-3">
        <Score label="Disk score" value={result.diskScore ?? undefined} change={delta?.disk} />
        <StabilityLabel stability={result.stability} />
      </div>
      <ResultNotes
        result={result}
        history={history}
        previous={previous}
        scored={result.diskScore != null}
        unscoredNote={
          direct
            ? undefined
            : "Not scored: only direct I/O, which measures the drive itself, is scored."
        }
      />
      <p className="text-fg-muted">
        {direct
          ? "Direct I/O: the operating system's cache was bypassed, so this measures the drive."
          : "Buffered I/O: the operating system's cache is part of what was measured."}
        {disk.ioModeNote ? ` ${disk.ioModeNote}` : ""}
      </p>

      <div className="overflow-x-auto">
        <table className="w-full min-w-[40rem] text-left">
          <caption className="sr-only">Disk measurements (median of all rounds)</caption>
          <thead>
            <tr className="text-fg-muted">
              <th scope="col" className="py-1.5 pr-4 font-medium">
                Test
              </th>
              <th scope="col" className="py-1.5 pr-4 text-right font-medium">
                Speed
              </th>
              <th scope="col" className="py-1.5 pr-4 text-right font-medium">
                Typical response
              </th>
              <th scope="col" className="py-1.5 pr-4 text-right font-medium">
                Slowest 1%
              </th>
              <th scope="col" className="py-1.5 text-right font-medium">
                Checked
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {disk.phases.map((p) => (
              <PhaseRow key={p.label} phase={p} />
            ))}
          </tbody>
        </table>
      </div>

      <p className="num text-fg-muted">
        This run wrote {formatBytes(disk.bytesWritten)} to the drive and read{" "}
        {formatBytes(disk.bytesRead)} from it, using a {formatBytes(disk.fileBytes)} temporary file
        that has been deleted.
      </p>
      {disk.notes.length > 0 && (
        <ul className="space-y-1 text-fg-muted">
          {disk.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
      <p className="text-[0.8125rem] text-fg-subtle">
        {disk.target.label} · {disk.target.fileSystem}
        {d
          ? ` · ${[d.model ?? d.name, d.transport, d.rotational ? "hard disk" : "solid state"].filter(Boolean).join(" · ")}`
          : ""}
        <br />
        Queue depth is shown only where each outstanding request really reaches the drive (direct
        I/O). Response times are exact percentiles of every operation.
      </p>
      <EnvironmentFooter result={result} />
    </section>
  );
}

function PhaseRow({ phase: p }: { phase: DiskPhaseResult }) {
  const sequential = p.blockBytes >= 1 << 20;
  const speed = sequential
    ? formatThroughput(p.medianMbps, "MB/s")
    : formatThroughput(p.medianIops, "IOPS");
  return (
    <tr>
      <th scope="row" className="py-1.5 pr-4 font-normal">
        {p.label}
      </th>
      <td className="num py-1.5 pr-4 text-right">
        <span className="font-medium">{speed}</span>{" "}
        <span className="text-fg-subtle">{formatSpread({ cvPercent: p.cvPercent })}</span>
        {!sequential && (
          <span className="text-fg-subtle"> · {formatThroughput(p.medianMbps, "MB/s")}</span>
        )}
      </td>
      <td className="num py-1.5 pr-4 text-right">
        {p.latency ? formatMicros(p.latency.p50Us) : "—"}
      </td>
      <td className="num py-1.5 pr-4 text-right">
        {p.latency ? formatMicros(p.latency.p99Us) : "—"}
      </td>
      <td className="py-1.5 text-right">
        {p.validated ? (
          <span
            className="inline-flex items-center gap-1 text-ok"
            title="The data read back matched what was written."
          >
            <Check size="0.875rem" aria-hidden /> Verified
          </span>
        ) : (
          <span className="text-fg-subtle">Not checked</span>
        )}
      </td>
    </tr>
  );
}
