import { Download, Monitor, Moon, RotateCcw, Sun } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { PageHeader } from "@/components/PageHeader";
import { Switch } from "@/components/ui/Switch";
import { cn } from "@/lib/cn";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { togglePaused } from "@/services/monitoring";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore, type ThemePreference } from "@/stores/settingsStore";
import { toast } from "@/stores/toastStore";
import { useUiStore } from "@/stores/uiStore";

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
            description="Closing the window keeps Task Manager running in the system tray, so alerts still work."
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
                  <Icon size={14} aria-hidden />
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
              <RotateCcw size={14} aria-hidden /> Reset
            </button>
          </Field>
        </Section>
      </div>
    </>
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
            App version, system details, settings, performance and recent errors.
          </div>
        </div>
        <button
          type="button"
          disabled={disabled}
          onClick={() => void run()}
          className="flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-border px-3 hover:bg-surface-2 disabled:opacity-50"
        >
          <Download size={14} aria-hidden /> Export…
        </button>
      </div>
      <label className="flex items-center gap-2 text-fg-muted">
        <input
          type="checkbox"
          checked={processes}
          onChange={(e) => setProcesses(e.target.checked)}
        />
        Include the process list
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
