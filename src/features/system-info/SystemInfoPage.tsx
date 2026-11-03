import { Download } from "lucide-react";
import type { ReactNode } from "react";
import { DiagnosticsCard } from "./DiagnosticsCard";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList, Unavailable } from "@/components/ui/KeyValueList";
import { useRemoteData } from "@/hooks/useRemoteData";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";
import type { KeyValueItem } from "@/components/ui/KeyValueList";
import type { HardwareInfo, StaticSystemInfo } from "@/types";
import { formatBytes, formatDuration, formatPercent, formatTimestamp } from "@/utils/format";
import { INTERFACE_KIND_LABEL, uptimeSeconds, visibleInterfaces } from "@/utils/system";
import { DISK_KIND_LABEL, batteryState, cacheLabel, describeScreen, formatMhz } from "./format";

/** OS rows. Linux shows the os-release name instead of sysinfo's name/version. */
function osItems(info: StaticSystemInfo): KeyValueItem[] {
  const named: KeyValueItem[] = info.distribution
    ? [{ label: "Distribution", value: info.distribution }]
    : [
        { label: "Name", value: info.osName ?? <Unavailable /> },
        { label: "Version", value: info.longOsVersion ?? info.osVersion ?? <Unavailable /> },
      ];
  const optional: [string, string | null][] = [
    ["Desktop", info.desktopEnvironment],
    ["Display server", info.displayServer],
    ["Init system", info.initSystem],
    ["Container", info.container],
  ];
  return [
    ...named,
    { label: "Kernel", value: info.kernelVersion ?? <Unavailable /> },
    { label: "Hostname", value: info.hostname ?? <Unavailable /> },
    { label: "Architecture", value: info.architecture },
    // Shown only where the concept exists and was detected.
    ...optional.flatMap(([label, value]) => (value ? [{ label, value }] : [])),
  ];
}

export function SystemInfoPage() {
  const info = useMetricsStore((s) => s.staticInfo);
  const now = useMetricsStore((s) => s.tick?.timestampMs);
  const load = useMetricsStore((s) => s.tick?.cpu.loadAverage);
  // Firmware, caches and batteries are read on demand; the battery changes, so
  // refresh slowly while this page is open.
  const hardware = useRemoteData(() => backend.getHardwareInfo(), "hardware", 15_000).data;

  return (
    <>
      <PageHeader
        title="System Information"
        description="Operating system, hardware and environment."
        actions={
          <button
            type="button"
            disabled={!isBackendAvailable()}
            onClick={() => void exportSummary()}
            className="flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg disabled:opacity-50"
          >
            <Download size={14} aria-hidden /> Export summary…
          </button>
        }
      />
      {!info || now == null ? (
        <ConnectionNotice />
      ) : (
        <div className="max-w-5xl space-y-8">
          <Group title="System">
            <Card>
              <CardHeader title="Operating system" />
              <div className="px-4 pb-4 pt-3">
                <KeyValueList items={osItems(info)} />
              </div>
            </Card>
            <MachineCard hardware={hardware} />
            <Card>
              <CardHeader title="Session" />
              <div className="px-4 pb-4 pt-3">
                <KeyValueList
                  items={[
                    {
                      label: "Uptime",
                      value: formatDuration(uptimeSeconds(info.bootTimeSecs, now)),
                    },
                    {
                      label: "Boot time",
                      value:
                        info.bootTimeSecs != null ? (
                          formatTimestamp(info.bootTimeSecs * 1000)
                        ) : (
                          <Unavailable />
                        ),
                    },
                    ...(load
                      ? [
                          {
                            label: "Load average (1/5/15 min)",
                            hint: "Average number of tasks running or waiting to run. Not a percentage: compare it with the number of logical processors.",
                            value: `${load.one.toFixed(2)} / ${load.five.toFixed(2)} / ${load.fifteen.toFixed(2)}`,
                          },
                        ]
                      : []),
                  ]}
                />
              </div>
            </Card>
            <LocaleCard />
          </Group>

          <Group title="Hardware">
            <ProcessorCard info={info} hardware={hardware} />
            <MemoryCard />
            <GraphicsCard />
            <StorageCard />
            <NetworkCard />
            <PowerCard hardware={hardware} />
            <TemperaturesCard />
          </Group>

          <Group title="Task Manager">
            <Card>
              <CardHeader title="Task Manager" />
              <div className="px-4 pb-4 pt-3">
                <KeyValueList
                  items={[
                    { label: "Version", value: info.appVersion },
                    { label: "Process ID", value: info.selfPid },
                  ]}
                />
              </div>
            </Card>
            <DiagnosticsCard />
          </Group>
        </div>
      )}
    </>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-label={title}>
      <h2 className="mb-3 text-[11px] font-semibold uppercase tracking-wider text-fg-subtle">
        {title}
      </h2>
      <div className="grid items-start gap-4 lg:grid-cols-2">{children}</div>
    </section>
  );
}

const fieldOrNote = (value: string | null | undefined) => value ?? <Unavailable />;

function MachineCard({ hardware }: { hardware: HardwareInfo | null }) {
  const m = hardware?.machine;
  const rows: [string, string | null | undefined][] = [
    ["Manufacturer", m?.vendor],
    ["Model", m?.model],
    ["Type", m?.chassis],
    ["Motherboard", m?.board],
    ["Firmware vendor", m?.biosVendor],
    ["Firmware version", m?.biosVersion],
    ["Firmware date", m?.biosDate],
  ];
  const known = rows.filter(([, v]) => v);
  return (
    <Card>
      <CardHeader title="Machine" />
      <div className="px-4 pb-4 pt-3">
        {known.length === 0 ? (
          <p className="text-fg-muted">
            {hardware
              ? "The firmware does not report machine details to this account, or this platform does not expose them."
              : "Loading…"}
          </p>
        ) : (
          <KeyValueList items={known.map(([label, value]) => ({ label, value }))} />
        )}
      </div>
    </Card>
  );
}

function ProcessorCard({
  info,
  hardware,
}: {
  info: StaticSystemInfo;
  hardware: HardwareInfo | null;
}) {
  const frequency = useMetricsStore((s) => s.tick?.cpu.frequencyMhz);
  const cpu = hardware?.cpu;
  return (
    <Card>
      <CardHeader title="Processor" />
      <div className="px-4 pb-4 pt-3">
        <KeyValueList
          items={[
            { label: "Model", value: info.cpuBrand || <Unavailable /> },
            { label: "Vendor", value: info.cpuVendor || <Unavailable /> },
            { label: "Architecture", value: info.architecture },
            { label: "Physical cores", value: info.physicalCoreCount ?? <Unavailable /> },
            { label: "Logical processors", value: info.logicalCoreCount },
            {
              label: "Current frequency",
              hint: "Windows reports the nominal frequency, not the live boost clock.",
              value: fieldOrNote(formatMhz(frequency)),
            },
            ...(formatMhz(cpu?.maxMhz)
              ? [{ label: "Maximum frequency", value: formatMhz(cpu?.maxMhz) }]
              : []),
            ...(formatMhz(cpu?.minMhz)
              ? [{ label: "Minimum frequency", value: formatMhz(cpu?.minMhz) }]
              : []),
            ...(cpu?.governor ? [{ label: "Frequency governor", value: cpu.governor }] : []),
            ...(cpu?.caches ?? []).map((c) => ({
              label: `${cacheLabel(c)} cache`,
              hint: c.level < 3 ? "Per core" : "Shared between cores",
              value: formatBytes(c.sizeBytes),
            })),
            ...(cpu
              ? [
                  {
                    label: "Hardware virtualization",
                    value: cpu.virtualization ?? "Not offered or not reported",
                  },
                  { label: "Running in a virtual machine", value: cpu.virtualized ? "Yes" : "No" },
                ]
              : []),
          ]}
        />
      </div>
    </Card>
  );
}

function MemoryCard() {
  const m = useMetricsStore((s) => s.tick?.memory);
  if (!m) return null;
  return (
    <Card>
      <CardHeader title="Memory" />
      <div className="px-4 pb-4 pt-3">
        <KeyValueList
          items={[
            { label: "Installed", value: formatBytes(m.totalBytes) },
            {
              label: "In use",
              value: `${formatBytes(m.usedBytes)} (${formatPercent(m.usedPercent, 0)})`,
            },
            { label: "Available", value: formatBytes(m.availableBytes) },
            ...(m.cachedBytes != null
              ? [{ label: "Cached", value: formatBytes(m.cachedBytes) }]
              : []),
            ...(m.buffersBytes != null
              ? [{ label: "Buffers", value: formatBytes(m.buffersBytes) }]
              : []),
            {
              label: "Swap / page file",
              value:
                m.swapTotalBytes > 0
                  ? `${formatBytes(m.swapUsedBytes)} of ${formatBytes(m.swapTotalBytes)}`
                  : "None configured",
            },
            ...(m.commitLimitBytes != null
              ? [
                  {
                    label: "Commit charge",
                    hint: "Memory promised to programs, including the page file.",
                    value: `${formatBytes(m.commitUsedBytes)} of ${formatBytes(m.commitLimitBytes)}`,
                  },
                ]
              : []),
          ]}
        />
        <p className="mt-3 text-fg-muted">
          Memory type, speed and slot layout need administrator access to firmware tables and are
          not shown.
        </p>
      </div>
    </Card>
  );
}

function StorageCard() {
  const disks = useMetricsStore((s) => s.tick?.disks);
  if (!disks) return null;
  return (
    <Card>
      <CardHeader title="Storage" />
      <div className="divide-y divide-border px-4 pb-1 pt-3">
        {disks.length === 0 && <p className="pb-3 text-fg-muted">No volumes detected.</p>}
        {disks.map((d) => (
          <div key={`${d.name}:${d.mountPoint}`} className="py-3 first:pt-0">
            <KeyValueList
              items={[
                { label: "Volume", value: <span title={d.name}>{d.name || d.mountPoint}</span> },
                { label: "Mounted at", value: d.mountPoint },
                { label: "File system", value: d.fileSystem || <Unavailable /> },
                { label: "Type", value: DISK_KIND_LABEL[d.kind] },
                { label: "Capacity", value: formatBytes(d.totalBytes) },
                {
                  label: "Used",
                  value: `${formatBytes(d.usedBytes)} (${formatPercent(d.usedPercent, 0)})`,
                },
                { label: "Free", value: formatBytes(d.availableBytes) },
                ...(d.removable ? [{ label: "Removable", value: "Yes" }] : []),
                ...(d.readOnly ? [{ label: "Access", value: "Read-only" }] : []),
              ]}
            />
          </div>
        ))}
      </div>
    </Card>
  );
}

function NetworkCard() {
  const all = useMetricsStore((s) => s.tick?.networks);
  const showVirtual = useSettingsStore((s) => s.showVirtualAdapters);
  if (!all) return null;
  const networks = visibleInterfaces(all, showVirtual);
  return (
    <Card>
      <CardHeader title="Network adapters" />
      <div className="divide-y divide-border px-4 pb-1 pt-3">
        {networks.length === 0 && (
          <p className="pb-3 text-fg-muted">No network adapters detected.</p>
        )}
        {networks.map((n) => (
          <div key={n.name} className="py-3 first:pt-0">
            <KeyValueList
              items={[
                { label: "Adapter", value: n.name },
                ...(INTERFACE_KIND_LABEL[n.kind]
                  ? [{ label: "Type", value: INTERFACE_KIND_LABEL[n.kind] }]
                  : []),
                { label: "IPv4", value: n.ipv4.join(", ") || "None" },
                { label: "IPv6", value: n.ipv6.join(", ") || "None" },
                { label: "MAC address", value: n.macAddress ?? <Unavailable /> },
                ...(n.isPrimary ? [{ label: "Primary", value: "Yes" }] : []),
              ]}
            />
          </div>
        ))}
      </div>
    </Card>
  );
}

function PowerCard({ hardware }: { hardware: HardwareInfo | null }) {
  const source = hardware?.powerSource;
  const sourceLabel =
    source === "ac" ? "Mains power" : source === "battery" ? "Battery" : "Not reported";
  return (
    <Card>
      <CardHeader title="Power and battery" />
      <div className="divide-y divide-border px-4 pb-1 pt-3">
        <div className="pb-3">
          <KeyValueList
            items={[{ label: "Power source", value: hardware ? sourceLabel : <Unavailable /> }]}
          />
        </div>
        {hardware && hardware.batteries.length === 0 && (
          <p className="py-3 text-fg-muted">No battery detected.</p>
        )}
        {(hardware?.batteries ?? []).map((b) => (
          <div key={b.name} className="py-3">
            <KeyValueList
              items={[
                { label: "Battery", value: b.name },
                ...(batteryState(b) ? [{ label: "Charge", value: batteryState(b) }] : []),
                ...(b.healthPercent != null
                  ? [
                      {
                        label: "Health",
                        hint: "Full-charge capacity compared with the capacity when new.",
                        value: `${b.healthPercent}%`,
                      },
                    ]
                  : []),
                ...(b.cycleCount != null ? [{ label: "Charge cycles", value: b.cycleCount }] : []),
                ...(b.technology ? [{ label: "Chemistry", value: b.technology }] : []),
              ]}
            />
          </div>
        ))}
      </div>
    </Card>
  );
}

/** Facts the web view knows about the screen and the user's regional settings. */
function LocaleCard() {
  const theme = useUiStore((s) => s.resolvedTheme);
  const format = Intl.DateTimeFormat().resolvedOptions();
  return (
    <Card>
      <CardHeader title="Display and locale" />
      <div className="px-4 pb-4 pt-3">
        <KeyValueList
          items={[
            {
              label: "Screen",
              hint: "Size of the display the window is on, in physical pixels.",
              value: describeScreen({
                width: window.screen.width,
                height: window.screen.height,
                pixelRatio: window.devicePixelRatio || 1,
              }),
            },
            { label: "Color scheme", value: theme === "dark" ? "Dark" : "Light" },
            { label: "Time zone", value: format.timeZone },
            { label: "Language and region", value: format.locale },
          ]}
        />
      </div>
    </Card>
  );
}

function GraphicsCard() {
  const gpus = useMetricsStore((s) => s.tick?.gpus);
  if (!gpus) return null;
  const notReported = (
    <span className="font-normal text-fg-subtle">Not reported by the driver</span>
  );
  return (
    <Card>
      <CardHeader title="Graphics" />
      <div className="space-y-4 px-4 pb-4 pt-3">
        {gpus.length === 0 && <p className="text-fg-muted">No graphics adapter detected.</p>}
        {gpus.map((g) => (
          <KeyValueList
            key={g.id}
            items={[
              { label: "Adapter", value: <span title={g.name}>{g.name}</span> },
              { label: "Vendor", value: g.vendor ?? <Unavailable /> },
              {
                label: "Utilization",
                value: g.usagePercent != null ? formatPercent(g.usagePercent, 0) : notReported,
              },
              {
                label: "Dedicated memory",
                hint: "Integrated GPUs share system memory and have no dedicated memory to report.",
                value:
                  g.memoryTotalBytes != null
                    ? `${g.memoryUsedBytes != null ? `${formatBytes(g.memoryUsedBytes)} of ` : ""}${formatBytes(g.memoryTotalBytes)}`
                    : notReported,
              },
            ]}
          />
        ))}
      </div>
    </Card>
  );
}

function TemperaturesCard() {
  const temps = useMetricsStore((s) => s.tick?.temperatures);
  if (!temps) return null;
  const deg = (v: number) => `${v.toFixed(0)} °C`;
  return (
    <Card>
      <CardHeader title="Temperatures" />
      <div className="px-4 pb-4 pt-3">
        {temps.length === 0 ? (
          <p className="text-fg-muted">
            No temperature sensors are exposed by the system (common in virtual machines).
          </p>
        ) : (
          <KeyValueList
            items={temps.map((t) => ({
              label: t.label,
              hint: [
                t.maxCelsius != null ? `Highest seen ${deg(t.maxCelsius)}` : null,
                t.criticalCelsius != null ? `Critical at ${deg(t.criticalCelsius)}` : null,
              ]
                .filter(Boolean)
                .join(" · "),
              value: (
                <span
                  className={
                    t.criticalCelsius != null && t.celsius >= t.criticalCelsius - 5
                      ? "text-crit"
                      : undefined
                  }
                >
                  {deg(t.celsius)}
                </span>
              ),
            }))}
          />
        )}
      </div>
    </Card>
  );
}

async function exportSummary() {
  try {
    const path = await backend.exportSystemSummary();
    if (path) toast.success(`System summary saved to ${path}`);
  } catch (e) {
    toast.error(toAppError(e).message);
  }
}
