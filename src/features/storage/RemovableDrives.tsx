import { Usb } from "lucide-react";
import { useState } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend, toAppError } from "@/services/ipc";
import { useMetricsStore } from "@/stores/metricsStore";
import { toast } from "@/stores/toastStore";
import type { Volume } from "@/types";
import { formatBytes } from "@/utils/format";

/** Drives plugged in but not mounted do not change the mount table, so poll slowly. */
const POLL_MS = 5000;

const btn =
  "h-8 rounded-md border border-border px-3 text-fg-muted hover:text-fg disabled:opacity-50";

const nameOf = (v: Volume) => v.label ?? v.device;

/**
 * Mount and unmount external drives. The backend lists only drives the
 * desktop treats as user-managed, so system and internal partitions never
 * appear here, and it refuses them if asked.
 */
export function RemovableDrives() {
  const [busy, setBusy] = useState<string | null>(null);
  // A drive being mounted or unmounted changes the mount table within a tick.
  const mounts = useMetricsStore((s) => s.tick?.disks.map((d) => d.mountPoint).join("\n"));
  // Drive management is optional (e.g. UDisks missing): an error shows nothing.
  const { data, refresh } = useRemoteData(() => backend.listVolumes(), mounts ?? "", POLL_MS);
  const volumes = data ?? [];

  const act = async (v: Volume, what: "mount" | "unmount" | "powerOff") => {
    setBusy(v.device);
    try {
      if (what === "unmount") {
        await backend.unmountVolume(v.device);
        toast.success(`Unmounted ${nameOf(v)}. Eject the drive before unplugging it.`);
      } else if (what === "powerOff") {
        await backend.powerOffVolume(v.device);
        toast.success(`Ejected ${nameOf(v)}. You can unplug it.`);
      } else {
        const at = await backend.mountVolume(v.device);
        toast.success(at ? `Mounted ${nameOf(v)} at ${at}` : `Mounted ${nameOf(v)}`);
      }
    } catch (e) {
      const error = toAppError(e);
      if (error.kind !== "operationCancelled") toast.error(error.message);
    } finally {
      setBusy(null);
      refresh();
    }
  };

  if (volumes.length === 0) return null;
  return (
    <Card className="mb-4">
      <CardHeader title="External drives" icon={<Usb size="0.875rem" aria-hidden />} />
      <ul className="divide-y divide-border px-4 pb-1 pt-2">
        {volumes.map((v) => (
          <li key={v.device} className="flex items-center justify-between gap-4 py-2.5">
            <div className="min-w-0">
              <div className="truncate font-medium">{nameOf(v)}</div>
              <div className="num truncate text-[0.8125rem] text-fg-muted">
                {v.device} · {v.fileSystem} · {formatBytes(v.sizeBytes)}
                {v.mountPoint ? " · " : ""}
                {v.mountPoint && <span className="selectable">{v.mountPoint}</span>}
              </div>
            </div>
            <div className="flex shrink-0 gap-2">
              {!v.mountPoint && v.canPowerOff && (
                <button
                  type="button"
                  disabled={busy !== null}
                  onClick={() => void act(v, "powerOff")}
                  aria-label={`Eject ${nameOf(v)}`}
                  className={btn}
                >
                  {busy === v.device ? "Working…" : "Eject"}
                </button>
              )}
              <button
                type="button"
                disabled={busy !== null}
                onClick={() => void act(v, v.mountPoint ? "unmount" : "mount")}
                aria-label={`${v.mountPoint ? "Unmount" : "Mount"} ${nameOf(v)}`}
                className={btn}
              >
                {busy === v.device
                  ? v.mountPoint
                    ? "Unmounting…"
                    : "Mounting…"
                  : v.mountPoint
                    ? "Unmount"
                    : "Mount"}
              </button>
            </div>
          </li>
        ))}
      </ul>
      <p className="px-4 pb-3 pt-1 text-[0.8125rem] text-fg-subtle">
        Only external drives are listed. System and internal partitions can&apos;t be mounted or
        unmounted here.
      </p>
    </Card>
  );
}
