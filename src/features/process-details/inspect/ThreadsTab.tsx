import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useRemoteData } from "@/hooks/useRemoteData";
import { scaled, useRootPx } from "@/hooks/useRootPx";
import { STATUS_LABEL } from "@/features/processes/cells";
import { backend } from "@/services/ipc";
import type { ThreadInfo } from "@/types";
import { formatDuration, formatPercent, formatTimestamp } from "@/utils/format";
import { InspectBody } from "./InspectBody";

/** CPU per thread needs two readings, so this is the one tab that polls: only while visible. */
const POLL_MS = 1_500;
const ROW_HEIGHT = 24;
const COLUMNS =
  "grid-cols-[4.5rem_minmax(8rem,1fr)_6rem_4.5rem_6rem_4.5rem_3.5rem_10rem_minmax(8rem,1fr)]";

export function ThreadsTab({
  pid,
  startTime,
  linux,
}: {
  pid: number;
  startTime: number;
  linux: boolean;
}) {
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.getThreads(pid, startTime),
    `threads:${pid}:${startTime}`,
    POLL_MS,
  );
  const threads = data?.threads ?? [];
  return (
    <InspectBody
      loading={loading}
      error={error}
      hasData={data != null}
      onRefresh={refresh}
      toolbar={
        data && (
          <span className="text-fg-muted">
            {threads.length} threads · updates every 1.5 s while this tab is open
          </span>
        )
      }
      note={
        linux
          ? "CPU is a share of the whole machine, like the Processes page. The wait channel is the kernel function a sleeping thread is blocked in, when the system lets this account read it."
          : "Windows does not report thread state or wait reasons through the interfaces this app uses."
      }
    >
      <ThreadTable threads={threads} />
    </InspectBody>
  );
}

/** Virtualized: some processes run thousands of threads, re-read every poll. */
function ThreadTable({ threads }: { threads: readonly ThreadInfo[] }) {
  const scroller = useRef<HTMLDivElement>(null);
  const rowHeight = scaled(ROW_HEIGHT, useRootPx());
  const virtualizer = useVirtualizer({
    count: threads.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 12,
    getItemKey: (index) => threads[index]?.tid ?? index,
  });
  return (
    <div
      ref={scroller}
      role="table"
      aria-rowcount={threads.length + 1}
      className="max-h-[28rem] overflow-auto text-[0.8125rem]"
    >
      <div
        role="row"
        className={`sticky top-0 z-[1] grid ${COLUMNS} gap-3 bg-surface py-1 font-medium text-fg-muted`}
      >
        <span role="columnheader">TID</span>
        <span role="columnheader">Name</span>
        <span role="columnheader">State</span>
        <span role="columnheader" className="text-right">
          CPU
        </span>
        <span role="columnheader" className="text-right">
          CPU time
        </span>
        <span role="columnheader" className="text-right">
          Priority
        </span>
        <span role="columnheader" className="text-right">
          Core
        </span>
        <span role="columnheader">Started</span>
        <span role="columnheader">Waiting in</span>
      </div>
      <div
        className="num selectable"
        style={{ height: virtualizer.getTotalSize(), position: "relative" }}
      >
        {virtualizer.getVirtualItems().map((v) => {
          const t = threads[v.index];
          if (!t) return null;
          return (
            <div
              key={v.key}
              role="row"
              aria-rowindex={v.index + 2}
              className={`absolute left-0 grid w-full ${COLUMNS} items-center gap-3 border-t border-border/60`}
              style={{ top: v.start, height: rowHeight }}
            >
              <span role="cell" className="font-mono">
                {t.tid}
              </span>
              <span role="cell" className="truncate">
                {t.name ?? "—"}
              </span>
              <span role="cell">{t.state ? STATUS_LABEL[t.state] : "—"}</span>
              <span role="cell" className="text-right">
                {formatPercent(t.cpuPercent)}
              </span>
              <span role="cell" className="text-right">
                {t.cpuTimeMs == null ? "—" : formatDuration(t.cpuTimeMs / 1000)}
              </span>
              <span role="cell" className="text-right">
                {t.priority ?? "—"}
              </span>
              <span role="cell" className="text-right">
                {t.processor ?? "—"}
              </span>
              <span role="cell" className="truncate">
                {t.startTime != null ? formatTimestamp(t.startTime * 1000) : "—"}
              </span>
              <span role="cell" className="truncate font-mono" title={t.waitChannel ?? undefined}>
                {t.waitChannel ?? "—"}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
