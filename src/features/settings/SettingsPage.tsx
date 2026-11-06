import { Download, Monitor, Moon, RotateCcw, Sun } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { PageHeader } from "@/components/PageHeader";
import { Switch } from "@/components/ui/Switch";
import { cn } from "@/lib/cn";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { togglePaused } from "@/services/monitoring";
import { loadBenchmarkData } from "@/services/benchmarks";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore, type ThemePreference } from "@/stores/settingsStore";
import { toast } from "@/stores/toastStore";
import type {
  BenchmarkProfile,
  BenchmarkSettings,
  DiskLocation,
  DiskWriteLimit,
  GpuAdapterInfo,
  NetworkDataProfile,
} from "@/types";
import { useUiStore } from "@/stores/uiStore";
import { formatBytes } from "@/utils/format";

const THEMES: { id: ThemePreference; label: string; icon: typeof Sun }[] = [
  { id: "system", label: "System", icon: Monitor },
  { id: "light", label: "Light", icon: Sun },
  { id: "dark", label: "Dark", icon: Moon },
];

const INTERVALS = [
  { ms: 500, label: "500 ms" },
  { ms: 1000, label: "1 second" },
  { ms: 2000, label: "2 seconds" },
  { ms: 5000, label: "5 seconds" },
];

const HISTORY = [
  { minutes: 15, label: "15 minutes" },
  { minutes: 30, label: "30 minutes" },
  { minutes: 60, label: "1 hour" },
];

const selectCls =
  "h-8 rounded-md border border-border bg-surface px-2 outline-none focus:border-accent disabled:opacity-50";

export function SettingsPage() {
  const s = useSettingsStore();
  const monitoring = useMetricsStore((m) => m.monitoring);
  const setMonitoring = useMetricsStore((m) => m.setMonitoring);
  const navigate = useUiStore((u) => u.navigate);
  const platform = useMetricsStore((m) => m.staticInfo?.platform);
  const desktop = useMetricsStore((m) => m.staticInfo?.desktopEnvironment);
  // Stock GNOME has no tray: a hidden window would have no icon to come back from.
  const noTrayHost = platform === "linux" && /gnome/i.test(desktop ?? "");
  const backendReady = isBackendAvailable();

  const onInterval = (value: string) => {
    if (value === "paused") {
      if (!monitoring.paused) void togglePaused();
      return;
    }
    const ms = Number(value);
    s.update({ refreshIntervalMs: ms });
    setMonitoring({ ...monitoring, intervalMs: ms });
    if (monitoring.paused) void togglePaused();
  };

  return (
    <>
      <PageHeader title="Settings" description="Changes are saved automatically." />
      <div className="max-w-2xl space-y-4">
        <Section title="General">
          <Toggle
            label="Start with the operating system"
            checked={s.startWithOs}
            onChange={(v) => s.update({ startWithOs: v })}
          />
          <Toggle
            label="Minimize to tray"
            description={
              "Closing the window keeps Task Manager running in the system tray, so alerts still work." +
              (noTrayHost
                ? " GNOME shows no tray by default; without the AppIndicator extension, closing the window quits Task Manager."
                : "")
            }
            checked={s.minimizeToTray}
            onChange={(v) => s.update({ minimizeToTray: v })}
          />
          <Toggle
            label="Launch minimized"
            description="Start hidden (in the tray, or minimized if the tray is off)."
            checked={s.launchMinimized}
            onChange={(v) => s.update({ launchMinimized: v })}
          />
          <Toggle
            label="Remember window position"
            checked={s.rememberWindow}
            onChange={(v) => s.update({ rememberWindow: v })}
          />
        </Section>

        <Section title="Monitoring">
          <Field label="Refresh interval" htmlFor="interval">
            <select
              id="interval"
              disabled={!backendReady}
              value={monitoring.paused ? "paused" : String(s.refreshIntervalMs)}
              onChange={(e) => onInterval(e.target.value)}
              className={selectCls}
            >
              {INTERVALS.map((i) => (
                <option key={i.ms} value={i.ms}>
                  {i.label}
                </option>
              ))}
              <option value="paused">Paused</option>
            </select>
          </Field>
          <Field
            label="History length"
            htmlFor="history"
            description="Kept in memory only; nothing is written to disk."
          >
            <select
              id="history"
              value={s.historyMinutes}
              onChange={(e) => s.update({ historyMinutes: Number(e.target.value) })}
              className={selectCls}
            >
              {HISTORY.map((h) => (
                <option key={h.minutes} value={h.minutes}>
                  {h.label}
                </option>
              ))}
            </select>
          </Field>
          <Field
            label="Highlight new processes"
            htmlFor="new-process"
            description="Marks processes that started within this time in the process table."
          >
            <select
              id="new-process"
              value={s.newProcessSeconds}
              onChange={(e) => s.update({ newProcessSeconds: Number(e.target.value) })}
              className={selectCls}
            >
              {withCurrent([0, 5, 10, 30, 60], s.newProcessSeconds).map((v) => (
                <option key={v} value={v}>
                  {v === 0 ? "Off" : `${v} seconds`}
                </option>
              ))}
            </select>
          </Field>
          <Field
            label="CPU spike threshold"
            htmlFor="cpu-spike"
            description="Activity logs a process that stays above this share of the whole machine's CPU for a few seconds."
          >
            <select
              id="cpu-spike"
              value={s.cpuSpikePercent}
              onChange={(e) => s.update({ cpuSpikePercent: Number(e.target.value) })}
              className={selectCls}
            >
              {withCurrent([25, 50, 70, 80, 90], s.cpuSpikePercent).map((v) => (
                <option key={v} value={v}>
                  {v}%
                </option>
              ))}
            </select>
          </Field>
          <Field
            label="Memory jump threshold"
            htmlFor="memory-jump"
            description="Activity logs a process whose memory grows by this much within half a minute."
          >
            <select
              id="memory-jump"
              value={s.memoryJumpMb}
              onChange={(e) => s.update({ memoryJumpMb: Number(e.target.value) })}
              className={selectCls}
            >
              {withCurrent([128, 256, 512, 1024, 2048], s.memoryJumpMb).map((v) => (
                <option key={v} value={v}>
                  {v >= 1024 ? `${v / 1024} GB` : `${v} MB`}
                </option>
              ))}
            </select>
          </Field>
          {platform === "linux" && (
            <>
              <Toggle
                label="Show kernel threads"
                description="List kernel workers (kworker, ksoftirqd, …). They cannot be ended and usually outnumber normal processes."
                checked={s.showKernelThreads}
                onChange={(v) => s.update({ showKernelThreads: v })}
              />
              <Toggle
                label="Show virtual network adapters"
                description="Bridges, veth pairs and container networks. They are never included in network totals."
                checked={s.showVirtualAdapters}
                onChange={(v) => s.update({ showVirtualAdapters: v })}
              />
            </>
          )}
        </Section>

        <Section title="Appearance">
          <Field label="Theme" id="theme-label">
            <div
              role="radiogroup"
              aria-labelledby="theme-label"
              className="inline-flex gap-1 rounded-lg bg-surface-2 p-1"
            >
              {THEMES.map(({ id, label, icon: Icon }) => (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={s.theme === id}
                  onClick={() => s.update({ theme: id })}
                  className={cn(
                    "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-fg-muted",
                    s.theme === id && "bg-surface text-fg shadow-sm",
                  )}
                >
                  <Icon size="0.875rem" aria-hidden />
                  {label}
                </button>
              ))}
            </div>
          </Field>
          <Toggle
            label="Compact table"
            description="Denser rows in the process table."
            checked={s.compactTable}
            onChange={(v) => s.update({ compactTable: v })}
          />
          <Toggle
            label="Process heat mapping"
            description="Tint CPU and memory cells by usage."
            checked={s.heatMapping}
            onChange={(v) => s.update({ heatMapping: v })}
          />
        </Section>

        <Section title="Alerts">
          <Toggle
            label="Desktop notifications"
            description="Show a system notification when an alert fires."
            checked={s.notificationsEnabled}
            onChange={(v) => s.update({ notificationsEnabled: v })}
          />
          <Field label={`${s.alertRules.length} alert rules`}>
            <button
              type="button"
              onClick={() => navigate("alerts")}
              className="h-8 rounded-md border border-border px-3 hover:bg-surface-2"
            >
              Configure alerts
            </button>
          </Field>
        </Section>

        <BenchmarkSection />

        <Section title="Advanced">
          <Toggle
            label="Debug logging"
            description="Write detailed logs. Leave off unless you are troubleshooting."
            checked={s.debugLogging}
            onChange={(v) => s.update({ debugLogging: v })}
          />
          <DiagnosticsExport disabled={!backendReady} />
          <Field label="Reset all settings" description="Restores defaults, including alert rules.">
            <button
              type="button"
              onClick={() => {
                if (window.confirm("Reset all settings to their defaults?")) void s.reset();
              }}
              className="flex h-8 items-center gap-1.5 rounded-md border border-border px-3 text-crit hover:bg-surface-2"
            >
              <RotateCcw size="0.875rem" aria-hidden /> Reset
            </button>
          </Field>
        </Section>
      </div>
    </>
  );
}

const BENCH_PROFILES: { id: BenchmarkProfile; label: string }[] = [
  { id: "quick", label: "Quick" },
  { id: "standard", label: "Standard" },
  { id: "extended", label: "Extended" },
];
const WRITE_LIMITS: { id: DiskWriteLimit; label: string }[] = [
  { id: "conservative", label: "Conservative (1 GiB)" },
  { id: "standard", label: "Standard (4 GiB)" },
  { id: "extended", label: "Extended (16 GiB)" },
];
const DATA_PROFILES: { id: NetworkDataProfile; label: string }[] = [
  { id: "dataSaver", label: "Data Saver (≈25 MB)" },
  { id: "standard", label: "Standard (≈200 MB)" },
  { id: "maximum", label: "Maximum throughput (≈1 GB)" },
];

// Stable fallback: a fresh `[]` in a store selector would re-render forever.
const NO_ADAPTERS: GpuAdapterInfo[] = [];
const NO_LOCATIONS: DiskLocation[] = [];

function BenchmarkSection() {
  const b = useSettingsStore((s) => s.benchmark);
  const update = useSettingsStore((s) => s.update);
  const logical = useMetricsStore((m) => m.staticInfo?.logicalCoreCount ?? 0);
  const set = (patch: Partial<BenchmarkSettings>) => update({ benchmark: { ...b, ...patch } });
  const adapters = useBenchmarkStore((s) => s.capabilities?.gpuAdapters ?? NO_ADAPTERS);
  const locations = useBenchmarkStore((s) => s.capabilities?.diskLocations ?? NO_LOCATIONS);
  const capabilitiesLoaded = useBenchmarkStore((s) => s.capabilities !== null);
  useEffect(() => {
    if (!capabilitiesLoaded) void loadBenchmarkData();
  }, [capabilitiesLoaded]);
  const workerOptions = Array.from({ length: Math.min(logical, 256) }, (_, i) => i + 1);

  return (
    <Section title="Benchmarks">
      <Field label="Default profile" htmlFor="bench-default-profile">
        <select
          id="bench-default-profile"
          value={b.profile}
          onChange={(e) => set({ profile: e.target.value as BenchmarkProfile })}
          className={selectCls}
        >
          {BENCH_PROFILES.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </Field>
      <Field
        label="CPU worker threads"
        htmlFor="bench-workers"
        description="Automatic uses one worker per logical processor."
      >
        <select
          id="bench-workers"
          value={b.cpuWorkers ?? "auto"}
          onChange={(e) =>
            set({ cpuWorkers: e.target.value === "auto" ? null : Number(e.target.value) })
          }
          className={selectCls}
        >
          <option value="auto">Automatic</option>
          {workerOptions.map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
      </Field>
      <Field
        label="GPU to benchmark"
        htmlFor="bench-gpu"
        description="Automatic picks the GPU the system reports as high-performance."
      >
        <select
          id="bench-gpu"
          value={b.gpuAdapterId ?? "auto"}
          onChange={(e) => set({ gpuAdapterId: e.target.value === "auto" ? null : e.target.value })}
          className={cn(selectCls, "max-w-64")}
        >
          <option value="auto">Automatic</option>
          {adapters.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name} · {a.backend}
              {a.deviceType === "software" ? " (software)" : ""}
            </option>
          ))}
        </select>
      </Field>
      <Toggle
        label="Cooldown between heavy tests"
        description="Pause briefly between CPU and GPU tests so heat from one does not affect the next."
        checked={b.cooldown}
        onChange={(v) => set({ cooldown: v })}
      />
      <Field
        label="Storage benchmark location"
        htmlFor="bench-disk"
        description="A temporary file is created in a dedicated benchmark folder on this drive and deleted afterwards."
      >
        <select
          id="bench-disk"
          value={b.diskMountPoint ?? "auto"}
          onChange={(e) =>
            set({ diskMountPoint: e.target.value === "auto" ? null : e.target.value })
          }
          className={cn(selectCls, "max-w-64")}
        >
          <option value="auto">Automatic (app drive)</option>
          {locations.map((l) => (
            <option key={l.mountPoint} value={l.mountPoint}>
              {l.mountPoint} · {l.fileSystem} · {formatBytes(l.availableBytes)} free
              {l.removable ? " (removable)" : ""}
            </option>
          ))}
        </select>
      </Field>
      <Toggle
        label="Allow disk write benchmark"
        description="Write tests create a temporary file on the selected drive and delete it afterwards."
        checked={b.diskAllowWrite}
        onChange={(v) => set({ diskAllowWrite: v })}
      />
      <Field label="Disk benchmark write limit" htmlFor="bench-write-limit">
        <select
          id="bench-write-limit"
          value={b.diskWriteLimit}
          onChange={(e) => set({ diskWriteLimit: e.target.value as DiskWriteLimit })}
          className={selectCls}
        >
          {WRITE_LIMITS.map((l) => (
            <option key={l.id} value={l.id}>
              {l.label}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Internet data usage" htmlFor="bench-data">
        <select
          id="bench-data"
          value={b.networkDataProfile}
          onChange={(e) => set({ networkDataProfile: e.target.value as NetworkDataProfile })}
          className={selectCls}
        >
          {DATA_PROFILES.map((d) => (
            <option key={d.id} value={d.id}>
              {d.label}
            </option>
          ))}
        </select>
      </Field>
      <Toggle
        label="Share internet test results with the provider"
        description="Off by default. When on, the test provider (Cloudflare) may keep anonymized measurements."
        checked={b.networkShareResults}
        onChange={(v) => set({ networkShareResults: v })}
      />
      <Toggle
        label="Save benchmark history"
        description="Results are stored on this computer only."
        checked={b.saveHistory}
        onChange={(v) => set({ saveHistory: v })}
      />
      <Toggle
        label="Show raw measurements"
        checked={b.showRawMeasurements}
        onChange={(v) => set({ showRawMeasurements: v })}
      />
      <Toggle
        label="Show methodology"
        checked={b.showMethodology}
        onChange={(v) => set({ showMethodology: v })}
      />
      <Toggle
        label="Debug benchmark logging"
        checked={b.debugLogging}
        onChange={(v) => set({ debugLogging: v })}
      />
    </Section>
  );
}

function DiagnosticsExport({ disabled }: { disabled: boolean }) {
  const [processes, setProcesses] = useState(false);
  const [commandLines, setCommandLines] = useState(false);
  const run = async () => {
    try {
      const path = await backend.exportDiagnostics(processes, processes && commandLines);
      if (path) toast.success(`Diagnostics saved to ${path}`);
    } catch (e) {
      toast.error(toAppError(e).message);
    }
  };
  return (
    <div className="space-y-2 py-3">
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="font-medium">Export diagnostics</div>
          <div className="text-fg-muted">
            App version, system details, preferences and performance. Review before sharing.
          </div>
        </div>
        <button
          type="button"
          disabled={disabled}
          onClick={() => void run()}
          className="flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-border px-3 hover:bg-surface-2 disabled:opacity-50"
        >
          <Download size="0.875rem" aria-hidden /> Export…
        </button>
      </div>
      <label className="flex items-center gap-2 text-fg-muted">
        <input
          type="checkbox"
          checked={processes}
          onChange={(e) => setProcesses(e.target.checked)}
        />
        Include process list, hostname, alert rules and recent errors (may be sensitive)
      </label>
      <label className={cn("flex items-center gap-2 text-fg-muted", !processes && "opacity-50")}>
        <input
          type="checkbox"
          disabled={!processes}
          checked={commandLines}
          onChange={(e) => setCommandLines(e.target.checked)}
        />
        Include command lines (may contain passwords or tokens)
      </label>
    </div>
  );
}

/** The preset choices plus the stored value, if a hand-edited file set something else. */
function withCurrent(options: number[], current: number): number[] {
  return options.includes(current) ? options : [...options, current].sort((a, b) => a - b);
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card>
      <CardHeader title={title} />
      <div className="divide-y divide-border px-4 pb-2 pt-1">{children}</div>
    </Card>
  );
}

function Field({
  label,
  description,
  htmlFor,
  id,
  children,
}: {
  label: string;
  description?: string;
  htmlFor?: string;
  id?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 py-3">
      <div>
        <label id={id} htmlFor={htmlFor} className="font-medium">
          {label}
        </label>
        {description && <div className="text-fg-muted">{description}</div>}
      </div>
      {children}
    </div>
  );
}

function Toggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 py-3">
      <div>
        <div className="font-medium">{label}</div>
        {description && <div className="text-fg-muted">{description}</div>}
      </div>
      <Switch label={label} checked={checked} onChange={onChange} />
    </div>
  );
}
