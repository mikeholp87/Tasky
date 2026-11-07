import { useMemo } from "react";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend } from "@/services/ipc";
import type { ActivityEvent, Connection, ProcessPoint, ProcessRow } from "@/types";
import { cn } from "@/lib/cn";
import { diagnose } from "./diagnose";
import { InspectBody } from "./inspect/InspectBody";

/**
 * Reads the thread list and open descriptors once when opened (Refresh reads
 * again), and combines them with data the page already has.
 */
export function DiagnoseTab({
  row,
  history,
  connections,
  childCount,
  events,
}: {
  row: ProcessRow;
  history: readonly ProcessPoint[];
  connections: readonly Connection[] | null | undefined;
  childCount: number;
  events: readonly ActivityEvent[];
}) {
  const threads = useRemoteData(
    () => backend.getThreads(row.pid, row.startTime),
    `diag-threads:${row.pid}:${row.startTime}`,
  );
  // Open files are unsupported on Windows; that is not an error for diagnosis.
  const files = useRemoteData(
    () => backend.getOpenFiles(row.pid, row.startTime),
    `diag-files:${row.pid}:${row.startTime}`,
  );
  const findings = useMemo(
    () =>
      diagnose({
        row,
        history,
        threads: threads.data,
        openFiles: files.data,
        connections,
        childCount,
        events,
      }),
    [row, history, threads.data, files.data, connections, childCount, events],
  );
  const refresh = () => {
    threads.refresh();
    files.refresh();
  };
  return (
    <InspectBody
      loading={threads.loading && files.loading}
      error={threads.error && files.error ? threads.error : null}
      hasData={threads.data != null || files.data != null}
      onRefresh={refresh}
      note="Observations from this process's recent activity. They describe what is measurable now; they do not prove a cause."
    >
      <ul className="space-y-2">
        {findings.map((f, i) => (
          <li
            key={i}
            className={cn(
              "rounded-md border px-3 py-2",
              f.level === "note" ? "border-warn/40 bg-warn-soft" : "border-border bg-surface-2",
            )}
          >
            {f.text}
          </li>
        ))}
      </ul>
    </InspectBody>
  );
}
