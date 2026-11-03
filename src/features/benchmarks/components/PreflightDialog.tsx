import { TriangleAlert } from "lucide-react";
import { Dialog } from "radix-ui";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/cn";
import { buildRequest, preflight, startRun } from "@/services/benchmarks";
import { toAppError } from "@/services/ipc";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { BenchmarkKind, BenchmarkProfile, PreflightReport } from "@/types";
import { formatBytes, formatDuration } from "@/utils/format";
import { KIND_LABELS, PROFILE_LABELS } from "../format";
import { primaryButton, secondaryButton, selectClass } from "./styles";

const PROFILES: BenchmarkProfile[] = ["quick", "standard", "extended"];
const PROFILE_HINTS: Record<BenchmarkProfile, string> = {
  quick: "Rapid check with fewer samples.",
  standard: "Balances accuracy and duration.",
  extended: "More samples and sustained load; detects throttling and cache exhaustion.",
};
/** How long "Wait for system to become idle" keeps checking. */
const IDLE_WAIT_MS = 120_000;
const IDLE_POLL_MS = 3_000;

/**
 * Confirmation before every run: profile, estimated duration and data use,
 * and pre-run warnings (background load, battery). Nothing runs until the
 * user presses Start.
 */
export function PreflightDialog() {
  const launch = useBenchmarkStore((s) => s.launch);
  // Keyed so every launch starts with fresh dialog state.
  return launch ? <PreflightContent key={launch.join(",")} kinds={launch} /> : null;
}

function PreflightContent({ kinds: requested }: { kinds: BenchmarkKind[] }) {
  const close = useBenchmarkStore((s) => s.closeLaunch);
  const privacyAccepted = useSettingsStore((s) => s.benchmark.networkPrivacyAccepted);
  const updateSettings = useSettingsStore((s) => s.update);
  const isFull = requested.length > 1;
  // The internet test never runs as part of a full benchmark unless ticked.
  const [includeNetwork, setIncludeNetwork] = useState(false);
  const [consent, setConsent] = useState(privacyAccepted);
  const kinds: BenchmarkKind[] =
    isFull && includeNetwork && !requested.includes("network")
      ? [...requested, "network"]
      : requested;
  const hasNetwork = kinds.includes("network");
  const capabilities = useBenchmarkStore((s) => s.capabilities);
  const defaultProfile = useSettingsStore((s) => s.benchmark.profile);
  const [profile, setProfile] = useState<BenchmarkProfile>(defaultProfile);
  const [simulate, setSimulate] = useState(false);
  const [report, setReport] = useState<PreflightReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [waiting, setWaiting] = useState(false);
  const [starting, setStarting] = useState(false);
  const waitTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const request = buildRequest(kinds, { profile, simulate });
  const requestKey = JSON.stringify(request);

  useEffect(() => {
    let cancelled = false;
    preflight(request)
      .then((r) => {
        if (cancelled) return;
        setReport(r);
        setError(null);
      })
      .catch((e: unknown) => !cancelled && setError(toAppError(e).message));
    return () => {
      cancelled = true;
    };
    // requestKey captures every field of the request.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestKey]);

  useEffect(() => () => clearTimeout(waitTimer.current), []);

  const start = async () => {
    if (hasNetwork && !consent) return;
    if (hasNetwork && !privacyAccepted) {
      const benchmark = useSettingsStore.getState().benchmark;
      updateSettings({ benchmark: { ...benchmark, networkPrivacyAccepted: true } });
    }
    setStarting(true);
    const ok = await startRun(request);
    setStarting(false);
    if (ok) close();
  };

  const waitForIdle = () => {
    setWaiting(true);
    const deadline = Date.now() + IDLE_WAIT_MS;
    const poll = async () => {
      try {
        const r = await preflight(request);
        setReport(r);
        if (r.loadLevel === "idle") {
          setWaiting(false);
          await start();
          return;
        }
      } catch (e) {
        setError(toAppError(e).message);
        setWaiting(false);
        return;
      }
      if (Date.now() < deadline) {
        waitTimer.current = setTimeout(() => void poll(), IDLE_POLL_MS);
      } else {
        setWaiting(false);
        setError("The system did not become idle within 2 minutes.");
      }
    };
    void poll();
  };

  const cancel = () => {
    clearTimeout(waitTimer.current);
    setWaiting(false);
    close();
  };

  const busy = report?.loadLevel !== undefined && report.loadLevel !== "idle";
  const title =
    kinds.length > 1 ? "Run full benchmark" : `Run ${KIND_LABELS[kinds[0] ?? "cpu"]} benchmark`;

  return (
    <Dialog.Root open onOpenChange={(open) => !open && cancel()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/40" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 max-h-[calc(100vh-2rem)] w-[480px] max-w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 overflow-auto rounded-xl border border-border bg-surface p-5 shadow-xl">
          <Dialog.Title className="text-base font-semibold">{title}</Dialog.Title>
          <Dialog.Description className="mt-1 text-fg-muted">
            {kinds.map((k) => KIND_LABELS[k]).join(", ")}. Close other applications for the most
            accurate results.
          </Dialog.Description>

          <div className="mt-4 space-y-3">
            <div className="flex items-center justify-between gap-3">
              <label htmlFor="bench-profile" className="font-medium">
                Profile
              </label>
              <select
                id="bench-profile"
                className={selectClass}
                value={profile}
                onChange={(e) => setProfile(e.target.value as BenchmarkProfile)}
              >
                {PROFILES.map((p) => (
                  <option key={p} value={p}>
                    {PROFILE_LABELS[p]}
                  </option>
                ))}
              </select>
            </div>
            <p className="text-fg-muted">{PROFILE_HINTS[profile]}</p>

            {capabilities?.simulationAvailable && (
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={simulate}
                  onChange={(e) => setSimulate(e.target.checked)}
                />
                Simulated run (development: no hardware load, values are not measurements)
              </label>
            )}

            {report && (
              <dl className="num grid grid-cols-2 gap-y-1 rounded-md bg-surface-2 p-3">
                <dt className="text-fg-muted">Estimated duration</dt>
                <dd>
                  {report.estimatedSecs > 0 ? `≈ ${formatDuration(report.estimatedSecs)}` : "—"}
                </dd>
                {report.estimatedDiskWriteBytes > 0 && (
                  <>
                    <dt className="text-fg-muted">Temporary data written</dt>
                    <dd>up to {formatBytes(report.estimatedDiskWriteBytes)}</dd>
                  </>
                )}
                {report.estimatedNetworkBytes > 0 && (
                  <>
                    <dt className="text-fg-muted">Internet data</dt>
                    <dd>up to ≈ {Math.round(report.estimatedNetworkBytes / 1e6)} MB</dd>
                  </>
                )}
              </dl>
            )}

            {isFull && (
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={includeNetwork}
                  onChange={(e) => setIncludeNetwork(e.target.checked)}
                />
                Include the internet test (uses internet data)
              </label>
            )}

            {hasNetwork && (
              <div className="space-y-2 rounded-md border border-border p-3">
                <p className="font-medium">About the internet test</p>
                <ul className="list-disc space-y-1 pl-5 text-fg-muted">
                  <li>
                    It connects to Cloudflare&apos;s speed test servers, which necessarily see this
                    device&apos;s public IP address.
                  </li>
                  <li>
                    It uses internet data (amount shown above) and is never run automatically.
                  </li>
                  <li>
                    Cloudflare can keep anonymized measurements; this is off unless you enable it in
                    Settings → Benchmarks.
                  </li>
                  <li>The test is optional. Results stay on this computer.</li>
                </ul>
                <label className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={consent}
                    onChange={(e) => setConsent(e.target.checked)}
                  />
                  I understand and want to run the internet test
                </label>
              </div>
            )}

            {report?.warnings.map((w, i) => (
              <p
                key={`${w.code}-${i}`}
                role="alert"
                className={cn("flex gap-2 rounded-md bg-warn-soft p-3 text-warn")}
              >
                <TriangleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
                <span>{w.message}</span>
              </p>
            ))}
            {waiting && (
              <p role="status" className="text-fg-muted">
                Waiting for the system to become idle…
              </p>
            )}
            {error && (
              <p role="alert" className="rounded-md bg-crit-soft p-3 text-crit">
                {error}
              </p>
            )}
          </div>

          <div className="mt-5 flex flex-wrap justify-end gap-2">
            <button type="button" className={secondaryButton} onClick={cancel}>
              Cancel
            </button>
            {busy && !waiting && (
              <button type="button" className={secondaryButton} onClick={waitForIdle}>
                Wait for system to become idle
              </button>
            )}
            <button
              type="button"
              className={primaryButton}
              disabled={!report || starting || waiting || (hasNetwork && !consent)}
              onClick={() => void start()}
            >
              {busy ? "Run anyway" : "Start benchmark"}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
