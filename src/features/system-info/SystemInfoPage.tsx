import { Download } from "lucide-react";
import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { create } from "zustand";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { Card } from "@/components/ui/Card";
import { cn } from "@/lib/cn";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { toast } from "@/stores/toastStore";
import {
  AppSection,
  DisplaySection,
  GraphicsSection,
  MachineSection,
  MemorySection,
  NetworkSection,
  OsSection,
  PowerSection,
  ProcessorSection,
  SensorsSection,
  SessionSection,
  StorageSection,
} from "./sections";
import {
  appSummary,
  displaySummary,
  graphicsSummary,
  machineSummary,
  memorySummary,
  networkSummary,
  osSummary,
  powerSummary,
  processorSummary,
  sensorsSummary,
  sessionSummary,
  storageSummary,
} from "./summaries";

type SectionId =
  | "os"
  | "machine"
  | "session"
  | "display"
  | "processor"
  | "memory"
  | "graphics"
  | "storage"
  | "network"
  | "power"
  | "sensors"
  | "app";

interface Section {
  id: SectionId;
  label: string;
  group: "System" | "Hardware" | "Task Manager";
  summary: string;
  content: ReactNode;
}

const GROUPS: readonly Section["group"][] = ["System", "Hardware", "Task Manager"];

/** Kept for the session so coming back to the page reopens the same section. */
const useSelectedSection = create<{ id: SectionId; set: (id: SectionId) => void }>((set) => ({
  id: "os",
  set: (id) => set({ id }),
}));

export function SystemInfoPage() {
  const info = useMetricsStore((s) => s.staticInfo);
  const now = useMetricsStore((s) => s.tick?.timestampMs);
  const memory = useMetricsStore((s) => s.tick?.memory);
  const gpus = useMetricsStore((s) => s.tick?.gpus);
  const disks = useMetricsStore((s) => s.tick?.disks);
  const networks = useMetricsStore((s) => s.tick?.networks);
  const temperatures = useMetricsStore((s) => s.tick?.temperatures);
  const showVirtual = useSettingsStore((s) => s.showVirtualAdapters);
  // Firmware, caches and batteries are read on demand; the battery changes, so
  // refresh slowly while this page is open.
  const hardware = useRemoteData(() => backend.getHardwareInfo(), "hardware", 15_000).data;
  const selected = useSelectedSection((s) => s.id);
  const choose = useSelectedSection((s) => s.set);
  const tabs = useRef<Record<string, HTMLButtonElement | null>>({});

  const header = (
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
          <Download size="0.875rem" aria-hidden /> Export summary…
        </button>
      }
    />
  );
  if (!info || now == null) {
    return (
      <>
        {header}
        <ConnectionNotice />
      </>
    );
  }

  const screen = {
    width: window.screen.width,
    height: window.screen.height,
    pixelRatio: window.devicePixelRatio || 1,
  };
  const sections: Section[] = [
    {
      id: "os",
      label: "Operating system",
      group: "System",
      summary: osSummary(info),
      content: <OsSection info={info} />,
    },
    {
      id: "machine",
      label: "Machine",
      group: "System",
      summary: machineSummary(hardware),
      content: <MachineSection hardware={hardware} />,
    },
    {
      id: "session",
      label: "Session",
      group: "System",
      summary: sessionSummary(info, now),
      content: <SessionSection info={info} now={now} />,
    },
    {
      id: "display",
      label: "Display and locale",
      group: "System",
      summary: displaySummary(screen),
      content: <DisplaySection />,
    },
    {
      id: "processor",
      label: "Processor",
      group: "Hardware",
      summary: processorSummary(info),
      content: <ProcessorSection info={info} hardware={hardware} />,
    },
    {
      id: "memory",
      label: "Memory",
      group: "Hardware",
      summary: memorySummary(memory),
      content: <MemorySection />,
    },
    {
      id: "graphics",
      label: "Graphics",
      group: "Hardware",
      summary: graphicsSummary(gpus),
      content: <GraphicsSection />,
    },
    {
      id: "storage",
      label: "Storage",
      group: "Hardware",
      summary: storageSummary(disks),
      content: <StorageSection />,
    },
    {
      id: "network",
      label: "Network adapters",
      group: "Hardware",
      summary: networkSummary(networks, showVirtual),
      content: <NetworkSection />,
    },
    {
      id: "power",
      label: "Power and battery",
      group: "Hardware",
      summary: powerSummary(hardware),
      content: <PowerSection hardware={hardware} />,
    },
    {
      id: "sensors",
      label: "Temperatures",
      group: "Hardware",
      summary: sensorsSummary(temperatures),
      content: <SensorsSection />,
    },
    {
      id: "app",
      label: "Task Manager",
      group: "Task Manager",
      summary: appSummary(info),
      content: <AppSection info={info} />,
    },
  ];
  const current = sections.find((s) => s.id === selected) ?? sections[0];
  if (!current) return null;

  // Arrow keys move through the list like the tabs they are.
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const at = sections.findIndex((s) => s.id === current.id);
    const next =
      e.key === "ArrowDown" || e.key === "ArrowRight"
        ? at + 1
        : e.key === "ArrowUp" || e.key === "ArrowLeft"
          ? at - 1
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? sections.length - 1
              : null;
    const target = next == null ? undefined : sections[(next + sections.length) % sections.length];
    if (!target) return;
    e.preventDefault();
    choose(target.id);
    tabs.current[target.id]?.focus();
  };

  return (
    <>
      {header}
      <div className="flex max-w-6xl flex-col gap-4 lg:flex-row lg:items-start lg:gap-6">
        <div
          role="tablist"
          aria-label="System information sections"
          aria-orientation="vertical"
          onKeyDown={onKeyDown}
          className="flex shrink-0 gap-1 overflow-x-auto lg:sticky lg:top-0 lg:max-h-[calc(100vh-8rem)] lg:w-64 lg:flex-col lg:overflow-y-auto"
        >
          {GROUPS.map((group) => (
            <div key={group} className="flex gap-1 lg:mb-3 lg:flex-col">
              <div className="hidden px-2.5 pb-1 text-[0.8125rem] font-medium text-fg-subtle lg:block">
                {group}
              </div>
              {sections
                .filter((s) => s.group === group)
                .map((s) => {
                  const active = s.id === current.id;
                  return (
                    <button
                      key={s.id}
                      ref={(el) => {
                        tabs.current[s.id] = el;
                      }}
                      type="button"
                      role="tab"
                      id={`sysinfo-tab-${s.id}`}
                      aria-selected={active}
                      aria-controls="sysinfo-panel"
                      tabIndex={active ? 0 : -1}
                      onClick={() => choose(s.id)}
                      className={cn(
                        "relative shrink-0 rounded-lg px-2.5 py-1.5 text-left transition-colors lg:w-full",
                        "text-fg-muted hover:bg-surface-2 hover:text-fg",
                        active &&
                          "bg-surface-2 text-fg lg:before:absolute lg:before:inset-y-1.5 lg:before:left-0 lg:before:w-[3px] lg:before:rounded-full lg:before:bg-accent",
                      )}
                    >
                      <span className={cn("block whitespace-nowrap", active && "font-medium")}>
                        {s.label}
                      </span>
                      <span className="hidden truncate text-[0.8125rem] font-normal text-fg-subtle lg:block">
                        {s.summary}
                      </span>
                    </button>
                  );
                })}
            </div>
          ))}
        </div>

        <Card
          role="tabpanel"
          id="sysinfo-panel"
          aria-labelledby={`sysinfo-tab-${current.id}`}
          className="min-w-0 flex-1 p-5"
        >
          <h2 className="mb-3 text-lg font-semibold tracking-tight">{current.label}</h2>
          {current.content}
        </Card>
      </div>
    </>
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
