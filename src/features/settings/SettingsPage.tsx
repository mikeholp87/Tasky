import { Monitor, Moon, Sun } from "lucide-react";
import type { ReactNode } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { PageHeader } from "@/components/PageHeader";
import { cn } from "@/lib/cn";
import { setRefreshInterval, togglePaused } from "@/services/monitoring";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore, type ThemePreference } from "@/stores/settingsStore";

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

export function SettingsPage() {
  const settings = useSettingsStore();
  const monitoring = useMetricsStore((s) => s.monitoring);
  const connected = useMetricsStore((s) => s.status === "live");

  return (
    <>
      <PageHeader title="Settings" />
      <div className="max-w-2xl space-y-4">
        <Section title="Monitoring">
          <Field label="Refresh interval" htmlFor="interval">
            <select
              id="interval"
              disabled={!connected}
              value={monitoring.paused ? "paused" : String(monitoring.intervalMs)}
              onChange={(e) => {
                if (e.target.value === "paused") {
                  if (!monitoring.paused) void togglePaused();
                  return;
                }
                const ms = Number(e.target.value);
                void (async () => {
                  await setRefreshInterval(ms);
                  if (useMetricsStore.getState().monitoring.paused) await togglePaused();
                })();
              }}
              className="h-8 rounded-md border border-border bg-surface px-2 outline-none focus:border-accent disabled:opacity-50"
            >
              {INTERVALS.map((i) => (
                <option key={i.ms} value={i.ms}>
                  {i.label}
                </option>
              ))}
              <option value="paused">Paused</option>
            </select>
          </Field>
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
                  aria-checked={settings.theme === id}
                  onClick={() => settings.update({ theme: id })}
                  className={cn(
                    "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-fg-muted",
                    settings.theme === id && "bg-surface text-fg shadow-sm",
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
            checked={settings.compactTable}
            onChange={(v) => settings.update({ compactTable: v })}
          />
          <Toggle
            label="Process heat mapping"
            description="Tint CPU and memory cells by usage."
            checked={settings.heatMapping}
            onChange={(v) => settings.update({ heatMapping: v })}
          />
        </Section>
      </div>
    </>
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
  htmlFor,
  id,
  children,
}: {
  label: string;
  htmlFor?: string;
  id?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 py-3">
      <label id={id} htmlFor={htmlFor} className="font-medium">
        {label}
      </label>
      {children}
    </div>
  );
}

export function Toggle({
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
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative h-5 w-9 shrink-0 rounded-full transition-colors",
          checked ? "bg-accent" : "bg-border-strong",
        )}
      >
        <span
          className={cn(
            "absolute top-0.5 size-4 rounded-full bg-white shadow transition-transform",
            checked ? "translate-x-4" : "translate-x-0.5",
          )}
        />
      </button>
    </div>
  );
}
