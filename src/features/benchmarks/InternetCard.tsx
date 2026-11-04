import { useEffect, useState } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { backend } from "@/services/ipc";
import { runBenchmark } from "@/services/benchmarks";
import { isActive, useBenchmarkStore } from "@/stores/benchmarkStore";
import type {
  BenchmarkProfile,
  BenchmarkResult,
  InterfaceKind,
  InternetPlan,
  InternetResult,
  LatencyStats,
  TransferResult,
} from "@/types";
import { formatBytes, formatThroughput } from "./format";
import {
  EnvironmentFooter,
  Outcome,
  ProfilePicker,
  Progress,
  ResultNotes,
  RunRow,
  StabilityLabel,
} from "./parts";

const INTERNET_PROFILES: readonly { id: BenchmarkProfile; label: string; detail: string }[] = [
  { id: "quick", label: "Quick", detail: "About 15 seconds. Uses the least data." },
  { id: "standard", label: "Standard", detail: "About 30 seconds. The default." },
  { id: "extended", label: "Extended", detail: "About a minute. Uses the most data." },
];

const KIND_LABEL: Record<InterfaceKind, string> = {
  ethernet: "Ethernet",
  wifi: "Wi-Fi",
  vpn: "VPN",
  virtual: "Virtual",
  loopback: "Loopback",
  unknown: "Unknown connection type",
};

export function InternetCard({
  result,
  history,
}: {
  result: BenchmarkResult | null;
  history: readonly BenchmarkResult[];
}) {
  const status = useBenchmarkStore((s) => s.status);
  const [profile, setProfile] = useState<BenchmarkProfile>("standard");
  const [plan, setPlan] = useState<InternetPlan | null>(null);
  const active = isActive(status);
  const mine = active && status.kind === "internet";

  useEffect(() => {
    let stale = false;
    backend
      .internetPlan(profile)
      .then((p) => !stale && setPlan(p))
      .catch(() => !stale && setPlan(null));
    return () => {
      stale = true;
    };
  }, [profile]);

  return (
    <Card>
      <CardHeader title="Internet" />
      <div className="space-y-4 px-4 pb-4 pt-3">
        <ProfilePicker
          profiles={INTERNET_PROFILES}
          value={profile}
          onChange={setProfile}
          disabled={active}
        />
        {plan && (
          <p className="text-fg-muted" data-testid="internet-plan">
            Connects to {plan.host} ({plan.provider}) and sends and receives generated test data: up
            to {formatBytes(plan.maxDownloadBytes)} down and {formatBytes(plan.maxUploadBytes)} up.
            It counts against any data cap or mobile plan. No files or information about this
            computer are sent, but {plan.host} sees your public IP address, the timing of the
            requests and that this app made them. The server limits repeated large transfers, so run
            it sparingly. You are asked to confirm before it starts.
          </p>
        )}
        <RunRow
          active={active || !plan}
          mine={mine}
          detail={INTERNET_PROFILES.find((p) => p.id === profile)?.detail}
          label="Run internet test"
          onRun={() => void runBenchmark("internet", profile)}
        />
        {mine && <Progress kind="internet" />}
        <Outcome kind="internet" />
        {!mine && result?.internet && (
          <InternetResultPanel result={result} net={result.internet} history={history} />
        )}
        {!mine && !result && status.state !== "failed" && (
          <p className="text-fg-muted">
            Run the test to measure latency, download and upload speed. Pause large downloads and
            streaming first: they share the connection.
          </p>
        )}
      </div>
    </Card>
  );
}

function InternetResultPanel({
  result,
  net,
  history,
}: {
  result: BenchmarkResult;
  net: InternetResult;
  history: readonly BenchmarkResult[];
}) {
  const previous = history
    .filter(
      (h) =>
        h.id !== result.id &&
        h.kind === "internet" &&
        h.internet?.host === net.host &&
        h.startedAtMs < result.startedAtMs,
    )
    .sort((a, b) => b.startedAtMs - a.startedAtMs)[0];
  const prevDown = previous?.internet?.download?.medianMbps;
  const prevUp = previous?.internet?.upload?.medianMbps;
  return (
    <section aria-label="Latest internet result" className="space-y-4 border-t border-border pt-4">
      <div className="flex flex-wrap items-end gap-x-10 gap-y-3">
        <Speed label="Download" t={net.download} previous={prevDown} />
        <Speed label="Upload" t={net.upload} previous={prevUp} />
        <Latency label="Latency" l={net.idleLatency} />
        <StabilityLabel stability={result.stability} />
      </div>
      <ResultNotes result={result} history={history} previous={null} scored />
      <table className="w-full text-left">
        <caption className="sr-only">Internet measurements</caption>
        <thead>
          <tr className="text-fg-muted">
            <th scope="col" className="py-1.5 pr-4 font-medium">
              Measure
            </th>
            <th scope="col" className="py-1.5 pr-4 text-right font-medium">
              Median
            </th>
            <th scope="col" className="py-1.5 pr-4 text-right font-medium">
              Peak
            </th>
            <th scope="col" className="py-1.5 text-right font-medium">
              Response while busy
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {net.download && <TransferRow name="Download" t={net.download} />}
          {net.upload && <TransferRow name="Upload" t={net.upload} />}
        </tbody>
      </table>
      {net.idleLatency && (
        <p className="num text-fg-muted">
          Idle latency: median {net.idleLatency.medianMs.toFixed(0)} ms, fastest{" "}
          {net.idleLatency.minMs.toFixed(0)} ms, 95th percentile {net.idleLatency.p95Ms.toFixed(0)}{" "}
          ms. Jitter {net.idleLatency.jitterMs.toFixed(1)} ms (the average change between
          consecutive round trips).
        </p>
      )}
      <p className="num text-fg-muted">
        This run moved {formatBytes(net.bytesDownloaded)} down and {formatBytes(net.bytesUploaded)}{" "}
        up.
      </p>
      <ul className="space-y-1 text-fg-muted">
        {net.notes.map((n) => (
          <li key={n}>{n}</li>
        ))}
      </ul>
      <p className="text-[0.8125rem] text-fg-subtle">
        {net.provider} · {net.host}
        {net.serverLocation ? ` · server ${net.serverLocation}` : ""}
        {net.path.interface
          ? ` · via ${net.path.interface} (${KIND_LABEL[net.path.kind ?? "unknown"]})`
          : ""}
        <br />
        There is no internet score: the route to the server dominates the result. Compare runs only
        against the same server.
      </p>
      <EnvironmentFooter result={result} />
    </section>
  );
}

function Speed({
  label,
  t,
  previous,
}: {
  label: string;
  t: TransferResult | null;
  previous: number | undefined;
}) {
  const change = t && previous ? ((t.medianMbps - previous) / previous) * 100 : null;
  return (
    <div>
      <div className="text-fg-muted">{label}</div>
      <div className="flex items-baseline gap-2">
        <span className="num text-4xl font-medium leading-none tracking-tight">
          {t ? formatThroughput(t.medianMbps, "Mbit/s") : "—"}
        </span>
        {change != null && Math.abs(change) >= 0.05 && (
          <span className="num text-fg-muted">
            {change > 0 ? "+" : "−"}
            {Math.abs(change).toFixed(1)}%
          </span>
        )}
      </div>
    </div>
  );
}

function Latency({ label, l }: { label: string; l: LatencyStats | null }) {
  return (
    <div>
      <div className="text-fg-muted">{label}</div>
      <span className="num text-4xl font-medium leading-none tracking-tight">
        {l ? `${l.medianMs.toFixed(0)} ms` : "—"}
      </span>
    </div>
  );
}

function TransferRow({ name, t }: { name: string; t: TransferResult }) {
  return (
    <tr>
      <th scope="row" className="py-1.5 pr-4 font-normal">
        {name} ({t.connections} connections)
      </th>
      <td className="num py-1.5 pr-4 text-right font-medium">
        {formatThroughput(t.medianMbps, "Mbit/s")}
      </td>
      <td className="num py-1.5 pr-4 text-right">{formatThroughput(t.peakMbps, "Mbit/s")}</td>
      <td className="num py-1.5 text-right">
        {t.loadedLatency ? `${t.loadedLatency.medianMs.toFixed(0)} ms` : "—"}
      </td>
    </tr>
  );
}
