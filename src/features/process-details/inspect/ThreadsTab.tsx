import { useRemoteData } from "@/hooks/useRemoteData";
import { STATUS_LABEL } from "@/features/processes/cells";
import { backend } from "@/services/ipc";
import { formatDuration, formatPercent, formatTimestamp } from "@/utils/format";
import { InspectBody } from "./InspectBody";

/** CPU per thread needs two readings, so this is the one tab that polls: only while visible. */
const POLL_MS = 1_500;

export function ThreadsTab({ pid, linux }: { pid: number; linux: boolean }) {
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.getThreads(pid),
    `threads:${pid}`,
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
      <div className="max-h-[28rem] overflow-auto">
        <table className="w-full text-left text-[12px]">
          <thead className="sticky top-0 bg-surface text-fg-muted">
            <tr>
              <th className="py-1 pr-3 font-medium">TID</th>
              <th className="py-1 pr-3 font-medium">Name</th>
              <th className="py-1 pr-3 font-medium">State</th>
              <th className="py-1 pr-3 text-right font-medium">CPU</th>
              <th className="py-1 pr-3 text-right font-medium">CPU time</th>
              <th className="py-1 pr-3 text-right font-medium">Priority</th>
              <th className="py-1 pr-3 text-right font-medium">Core</th>
              <th className="py-1 pr-3 font-medium">Started</th>
              <th className="py-1 font-medium">Waiting in</th>
            </tr>
          </thead>
          <tbody className="num selectable">
            {threads.map((t) => (
              <tr key={t.tid} className="border-t border-border/60">
                <td className="py-1 pr-3 font-mono">{t.tid}</td>
                <td className="py-1 pr-3">{t.name ?? "—"}</td>
                <td className="py-1 pr-3">{t.state ? STATUS_LABEL[t.state] : "—"}</td>
                <td className="py-1 pr-3 text-right">{formatPercent(t.cpuPercent)}</td>
                <td className="py-1 pr-3 text-right">{formatDuration(t.cpuTimeMs / 1000)}</td>
                <td className="py-1 pr-3 text-right">{t.priority ?? "—"}</td>
                <td className="py-1 pr-3 text-right">{t.processor ?? "—"}</td>
                <td className="py-1 pr-3">
                  {t.startTime != null ? formatTimestamp(t.startTime * 1000) : "—"}
                </td>
                <td className="py-1 font-mono">{t.waitChannel ?? "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </InspectBody>
  );
}
