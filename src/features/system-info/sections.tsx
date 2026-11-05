/** Contents of each System Information section (the right-hand pane). */
import type { ReactNode } from "react";
import { Unavailable } from "@/components/ui/KeyValueList";
import { useMetricsStore } from "@/stores/metricsStore";
import { usePerfStore } from "@/stores/perfStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";
import type { HardwareInfo, StaticSystemInfo } from "@/types";
import {
  formatBytes,
  formatCount,
  formatDuration,
  formatPercent,
  formatTimestamp,
} from "@/utils/format";
import { INTERFACE_KIND_LABEL, uptimeSeconds, visibleInterfaces } from "@/utils/system";
import { DetailGroup, DetailRows, type DetailItem } from "./DetailRows";
import { DISK_KIND_LABEL, batteryState, cacheLabel, describeScreen, formatMhz } from "./format";
import { usedLabel } from "./summaries";

const Muted = ({ children }: { children: ReactNode }) => (
  <p className="text-fg-muted">{children}</p>
);
const notReported = <span className="font-normal text-fg-subtle">Not reported by the driver</span>;
const known = (value: string | null | undefined) => value || <Unavailable />;

export function OsSection({ info }: { info: StaticSystemInfo }) {
  const named: DetailItem[] = info.distribution
    ? [{ label: "Distribution", value: info.distribution }]
    : [
        { label: "Name", value: known(info.osName) },
        { label: "Version", value: known(info.longOsVersion ?? info.osVersion) },
      ];
  const optional: [string, string | null][] = [
    ["Desktop", info.desktopEnvironment],
    ["Display server", info.displayServer],
    ["Init system", info.initSystem],
    ["Container", info.container],
  ];
  return (
    <DetailRows
      items={[
        ...named,
        { label: "Kernel", value: known(info.kernelVersion) },
        { label: "Hostname", value: known(info.hostname) },
        { label: "Architecture", value: info.architecture },
        // Shown only where the concept exists and was detected.
        ...optional.flatMap(([label, value]) => (value ? [{ label, value }] : [])),
      ]}
    />
  );
}

export function MachineSection({ hardware }: { hardware: HardwareInfo | null }) {
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
  const present = rows.flatMap(([label, value]) => (value ? [{ label, value }] : []));
  if (!hardware) return <Muted>Loading…</Muted>;
  if (present.length === 0) {
    return (
      <Muted>
        This account cannot read machine details from the firmware, or the platform does not expose
        them. Serial numbers are never collected.
      </Muted>
    );
  }
  return <DetailRows items={present} />;
}

export function SessionSection({ info, now }: { info: StaticSystemInfo; now: number }) {
  const load = useMetricsStore((s) => s.tick?.cpu.loadAverage);
  return (
    <DetailRows
      items={[
        { label: "Uptime", value: formatDuration(uptimeSeconds(info.bootTimeSecs, now)) },
        {
          label: "Boot time",
          value:
            info.bootTimeSecs != null ? formatTimestamp(info.bootTimeSecs * 1000) : <Unavailable />,
        },
        ...(load
          ? [
              {
                label: "Load average",
                value: `${load.one.toFixed(2)} / ${load.five.toFixed(2)} / ${load.fifteen.toFixed(2)}`,
                note: "1, 5 and 15 minutes. The average number of tasks running or waiting to run, not a percentage: compare it with the number of logical processors.",
              },
            ]
          : []),
      ]}
    />
  );
}

/** What the web view knows about the screen and the user's regional settings. */
export function DisplaySection() {
  const theme = useUiStore((s) => s.resolvedTheme);
  const format = Intl.DateTimeFormat().resolvedOptions();
  return (
    <DetailRows
      items={[
        {
          label: "Screen",
          value: describeScreen({
            width: window.screen.width,
            height: window.screen.height,
            pixelRatio: window.devicePixelRatio || 1,
          }),
          note: "The display this window is on, in physical pixels.",
        },
        { label: "Color scheme", value: theme === "dark" ? "Dark" : "Light" },
        { label: "Time zone", value: format.timeZone },
        { label: "Language and region", value: format.locale },
      ]}
    />
  );
}

export function ProcessorSection({
  info,
  hardware,
}: {
  info: StaticSystemInfo;
  hardware: HardwareInfo | null;
}) {
  const frequency = useMetricsStore((s) => s.tick?.cpu.frequencyMhz);
  const cpu = hardware?.cpu;
  const optional = (label: string, value: string | null | undefined): DetailItem[] =>
    value ? [{ label, value }] : [];
  return (
    <>
      <DetailRows
        items={[
          { label: "Model", value: known(info.cpuBrand) },
          { label: "Vendor", value: known(info.cpuVendor) },
          { label: "Architecture", value: info.architecture },
          { label: "Physical cores", value: info.physicalCoreCount ?? <Unavailable /> },
          { label: "Logical processors", value: info.logicalCoreCount },
          {
            label: "Current frequency",
            value: formatMhz(frequency) ?? <Unavailable />,
            note: "Windows reports the nominal frequency, not the live boost clock.",
          },
          ...optional("Maximum frequency", formatMhz(cpu?.maxMhz)),
          ...optional("Minimum frequency", formatMhz(cpu?.minMhz)),
          ...optional("Frequency governor", cpu?.governor),
        ]}
      />
      {cpu && (
        <>
          {cpu.caches.length > 0 && (
            <DetailGroup title="Caches">
              <DetailRows
                items={cpu.caches.map((c) => ({
                  label: cacheLabel(c),
                  value: formatBytes(c.sizeBytes),
                  note: c.level < 3 ? "Per core" : "Shared between cores",
                }))}
              />
            </DetailGroup>
          )}
          <DetailGroup title="Virtualization">
            <DetailRows
              items={[
                {
                  label: "Hardware support",
                  value: cpu.virtualization ?? "Not offered or not reported",
                },
                { label: "Running in a virtual machine", value: cpu.virtualized ? "Yes" : "No" },
              ]}
            />
          </DetailGroup>
        </>
      )}
    </>
  );
}

export function MemorySection() {
  const m = useMetricsStore((s) => s.tick?.memory);
  if (!m) return null;
  return (
    <>
      <DetailRows
        items={[
          { label: "Installed", value: formatBytes(m.totalBytes) },
          { label: "In use", value: usedLabel(m) },
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
                  value: `${formatBytes(m.commitUsedBytes)} of ${formatBytes(m.commitLimitBytes)}`,
                  note: "Memory promised to programs, including the page file.",
                },
              ]
            : []),
        ]}
      />
      <p className="mt-4 text-fg-muted">
        Memory type, speed and slot layout need administrator access to firmware tables and are not
        shown.
      </p>
    </>
  );
}

export function GraphicsSection() {
  const gpus = useMetricsStore((s) => s.tick?.gpus);
  if (!gpus) return null;
  if (gpus.length === 0) return <Muted>No graphics adapter detected.</Muted>;
  return (
    <>
      {gpus.map((g) => (
        <DetailGroup key={g.id} title={gpus.length > 1 ? g.name : undefined}>
          <DetailRows
            items={[
              ...(gpus.length === 1 ? [{ label: "Adapter", value: g.name }] : []),
              { label: "Vendor", value: known(g.vendor) },
              {
                label: "Utilization",
                value: g.usagePercent != null ? formatPercent(g.usagePercent, 0) : notReported,
              },
              {
                label: "Dedicated memory",
                value:
                  g.memoryTotalBytes != null
                    ? `${g.memoryUsedBytes != null ? `${formatBytes(g.memoryUsedBytes)} of ` : ""}${formatBytes(g.memoryTotalBytes)}`
                    : notReported,
                note: "Integrated GPUs share system memory and have none of their own to report.",
              },
            ]}
          />
        </DetailGroup>
      ))}
    </>
  );
}

export function StorageSection() {
  const disks = useMetricsStore((s) => s.tick?.disks);
  if (!disks) return null;
  if (disks.length === 0) return <Muted>No volumes detected.</Muted>;
  return (
    <>
      {disks.map((d) => (
        <DetailGroup key={`${d.name}:${d.mountPoint}`} title={d.name || d.mountPoint}>
          <DetailRows
            items={[
              { label: "Mounted at", value: d.mountPoint },
              { label: "File system", value: known(d.fileSystem) },
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
        </DetailGroup>
      ))}
    </>
  );
}

export function NetworkSection() {
  const all = useMetricsStore((s) => s.tick?.networks);
  const showVirtual = useSettingsStore((s) => s.showVirtualAdapters);
  if (!all) return null;
  const networks = visibleInterfaces(all, showVirtual);
  if (networks.length === 0) return <Muted>No network adapters detected.</Muted>;
  return (
    <>
      {networks.map((n) => (
        <DetailGroup key={n.name} title={n.name}>
          <DetailRows
            items={[
              ...(INTERFACE_KIND_LABEL[n.kind]
                ? [{ label: "Type", value: INTERFACE_KIND_LABEL[n.kind] }]
                : []),
              { label: "IPv4", value: n.ipv4.join(", ") || "None" },
              { label: "IPv6", value: n.ipv6.join(", ") || "None" },
              { label: "MAC address", value: n.macAddress ?? <Unavailable /> },
              ...(n.isPrimary ? [{ label: "Primary", value: "Yes" }] : []),
            ]}
          />
        </DetailGroup>
      ))}
    </>
  );
}

export function PowerSection({ hardware }: { hardware: HardwareInfo | null }) {
  if (!hardware) return <Muted>Loading…</Muted>;
  const source = hardware.powerSource;
  return (
    <>
      <DetailRows
        items={[
          {
            label: "Power source",
            value:
              source === "ac" ? "Mains power" : source === "battery" ? "Battery" : "Not reported",
          },
        ]}
      />
      {hardware.batteries.length === 0 && (
        <p className="mt-4 text-fg-muted">No battery detected.</p>
      )}
      {hardware.batteries.map((b) => (
        <DetailGroup key={b.name} title={`Battery ${b.name}`}>
          <DetailRows
            items={[
              ...(batteryState(b) ? [{ label: "Charge", value: batteryState(b) }] : []),
              ...(b.healthPercent != null
                ? [
                    {
                      label: "Health",
                      value: `${b.healthPercent}%`,
                      note: "Full-charge capacity compared with the capacity when new.",
                    },
                  ]
                : []),
              ...(b.cycleCount != null ? [{ label: "Charge cycles", value: b.cycleCount }] : []),
              ...(b.technology ? [{ label: "Chemistry", value: b.technology }] : []),
            ]}
          />
        </DetailGroup>
      ))}
    </>
  );
}

export function SensorsSection() {
  const temps = useMetricsStore((s) => s.tick?.temperatures);
  if (!temps) return null;
  if (temps.length === 0) {
    return (
      <Muted>No temperature sensors are exposed by the system (common in virtual machines).</Muted>
    );
  }
  const deg = (v: number) => `${v.toFixed(0)} °C`;
  return (
    <DetailRows
      items={temps.map((t) => ({
        label: t.label,
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
        note:
          [
            t.maxCelsius != null ? `Highest seen ${deg(t.maxCelsius)}` : null,
            t.criticalCelsius != null ? `Critical at ${deg(t.criticalCelsius)}` : null,
          ]
            .filter(Boolean)
            .join(" · ") || undefined,
      }))}
    />
  );
}

const MOCK_COUNTS = [0, 100, 500, 1000, 5000];
const ms = (v: number) => `${v.toFixed(1)} ms`;

export function AppSection({ info }: { info: StaticSystemInfo }) {
  const engine = useMetricsStore((s) => s.tick?.engine);
  const ingest = usePerfStore((s) => s.ingestMs);
  const frame = usePerfStore((s) => s.frameMs);
  const mock = usePerfStore((s) => s.mockProcessCount);
  const setMock = usePerfStore((s) => s.setMockProcessCount);
  return (
    <>
      <DetailRows
        items={[
          { label: "Version", value: info.appVersion },
          { label: "Process ID", value: info.selfPid },
        ]}
      />
      {engine && (
        <DetailGroup title="Monitoring cost">
          <DetailRows
            items={[
              {
                label: "Backend poll time",
                value: ms(engine.pollDurationMs),
                note: "Time to sample the OS for one tick.",
              },
              { label: "Refresh interval", value: `${engine.intervalMs} ms` },
              { label: "Processes", value: formatCount(engine.processCount) },
              {
                label: "Tick payload",
                value: formatBytes(engine.lastPayloadBytes),
                note: "Serialized size of the previous tick sent to this window.",
              },
              {
                label: "Frontend ingest",
                value: ms(ingest),
                note: "Store and history updates for one tick.",
              },
              {
                label: "Tick to next frame",
                value: ms(frame),
                note: "Ingest plus React rendering and chart drawing.",
              },
            ]}
          />
        </DetailGroup>
      )}
      {import.meta.env.DEV && (
        <label className="mt-6 flex items-center justify-between gap-3 border-t border-border pt-3">
          <span>
            <span className="font-medium">Mock processes</span>
            <span className="block text-fg-muted">
              Development builds only. Replaces the real list.
            </span>
          </span>
          <select
            value={mock}
            onChange={(e) => setMock(Number(e.target.value))}
            className="h-8 rounded-md border border-border bg-surface px-2"
          >
            {MOCK_COUNTS.map((n) => (
              <option key={n} value={n}>
                {n === 0 ? "Off" : formatCount(n)}
              </option>
            ))}
          </select>
        </label>
      )}
    </>
  );
}
