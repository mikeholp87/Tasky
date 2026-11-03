import { useMemo, useState } from "react";
import { Copy, FileSearch, RefreshCw, Square } from "lucide-react";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { SearchBox } from "@/components/SearchBox";
import { Card } from "@/components/ui/Card";
import { cn } from "@/lib/cn";
import { copyText, requestAction } from "@/features/processes/actions";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend, isBackendAvailable } from "@/services/ipc";
import { useMetricsStore } from "@/stores/metricsStore";
import { useUiStore } from "@/stores/uiStore";
import {
  PROTOCOL_LABEL,
  STATE_LABEL,
  asPort,
  endpoint,
  filterRows,
  portOwners,
  toRows,
  type PortRow,
  type PortView,
} from "./logic";

/** Resolving owners scans every process's descriptors, so poll gently. */
const POLL_MS = 3_000;

const btn =
  "flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg disabled:opacity-50";

export function PortsPage() {
  const [query, setQuery] = useState("");
  const [view, setView] = useState<PortView>("listening");
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.listSockets(),
    "sockets",
    POLL_MS,
  );
  const rows = useMemo(() => (data ? toRows(data.sockets) : []), [data]);
  const shown = useMemo(() => filterRows(rows, view, query), [rows, view, query]);
  const port = asPort(query);
  const owners = useMemo(() => (port == null ? [] : portOwners(rows, port)), [rows, port]);

  return (
    <>
      <PageHeader
        title="Ports"
        description="Which program is listening on, or connected through, each port"
        actions={
          <button type="button" className={btn} onClick={refresh}>
            <RefreshCw size={14} aria-hidden /> Refresh
          </button>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <SearchBox
          value={query}
          onChange={setQuery}
          label="Search ports"
          placeholder="Port, process, PID, address…"
        />
        <div
          role="radiogroup"
          aria-label="Which sockets"
          className="flex rounded-md border border-border p-0.5"
        >
          {(["listening", "all"] as const).map((v) => (
            <button
              key={v}
              type="button"
              role="radio"
              aria-checked={view === v}
              onClick={() => setView(v)}
              className={cn(
                "h-7 rounded px-2.5 text-fg-muted",
                view === v && "bg-surface-2 text-fg",
              )}
            >
              {v === "listening" ? "Listening" : "All connections"}
            </button>
          ))}
        </div>
        <span className="text-fg-muted">{shown.length} shown</span>
      </div>

      {port != null && <ConflictBanner port={port} owners={owners} scanned={data != null} />}

      {data && data.inaccessibleProcesses > 0 && (
        <p role="note" className="mb-3 text-fg-muted">
          {data.inaccessibleProcesses} processes could not be inspected, so their sockets may show
          no owner. Run as administrator/root to see them.
        </p>
      )}
      {data?.truncated && (
        <p role="note" className="mb-3 text-fg-muted">
          Too many sockets to show; the list is cut off. Narrow it with the search.
        </p>
      )}

      {!isBackendAvailable() ? (
        <EmptyState title="Backend not available">Open the desktop app to see ports.</EmptyState>
      ) : error && !data ? (
        <EmptyState title="Unable to list ports">{error.message}</EmptyState>
      ) : loading ? (
        <EmptyState title="Loading…" />
      ) : shown.length === 0 ? (
        <EmptyState title="No matching sockets">
          {view === "listening"
            ? "Nothing listening matches. Try “All connections”."
            : "No sockets match your search."}
        </EmptyState>
      ) : (
        <Card className="overflow-auto">
          <table className="w-full text-left text-[12px]">
            <thead className="sticky top-0 bg-surface-2 text-fg-muted">
              <tr>
                <th className="px-3 py-2 font-medium">Port</th>
                <th className="px-3 py-2 font-medium">Protocol</th>
                <th className="px-3 py-2 font-medium">Local address</th>
                <th className="px-3 py-2 font-medium">Remote</th>
                <th className="px-3 py-2 font-medium">State</th>
                <th className="px-3 py-2 font-medium">Process</th>
                <th className="px-3 py-2 font-medium">PID</th>
                <th className="px-3 py-2 font-medium">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody className="num selectable">
              {shown.map((r) => (
                <PortRowView key={r.key} row={r} />
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </>
  );
}

function PortRowView({ row }: { row: PortRow }) {
  const openProcess = useUiStore((s) => s.openProcess);
  return (
    <tr className="border-t border-border/60">
      <td className="px-3 py-1.5 font-mono font-medium">{row.port}</td>
      <td className="px-3 py-1.5">{PROTOCOL_LABEL[row.protocol]}</td>
      <td className="px-3 py-1.5 font-mono">{endpoint(row.address, null)}</td>
      <td className="px-3 py-1.5 font-mono">{row.remote ?? "—"}</td>
      <td className="px-3 py-1.5">{row.state ? STATE_LABEL[row.state] : "Bound"}</td>
      <td className="px-3 py-1.5">
        {row.pid != null ? (
          <button
            type="button"
            className="text-accent hover:underline"
            onClick={() => openProcess(row.pid as number)}
          >
            {row.processName ?? "Unknown"}
          </button>
        ) : (
          <span
            className="text-fg-subtle"
            title="The system does not say which process owns this socket"
          >
            Unknown
          </span>
        )}
      </td>
      <td className="px-3 py-1.5 font-mono">{row.pid ?? "—"}</td>
      <td className="px-3 py-1.5 text-right">
        {row.pid != null && (
          <button
            type="button"
            aria-label={`Copy PID ${row.pid}`}
            title="Copy PID"
            className="rounded p-1 text-fg-subtle hover:text-fg"
            onClick={() => void copyText("PID", String(row.pid))}
          >
            <Copy size={13} aria-hidden />
          </button>
        )}
      </td>
    </tr>
  );
}

/**
 * Shown when the search is exactly a port number. Describes who holds the
 * port and offers actions; nothing is ever ended automatically.
 */
function ConflictBanner({
  port,
  owners,
  scanned,
}: {
  port: number;
  owners: ReturnType<typeof portOwners>;
  scanned: boolean;
}) {
  const processes = useMetricsStore((s) => s.tick?.processes);
  const openProcess = useUiStore((s) => s.openProcess);
  if (!scanned) return null;
  if (owners.length === 0) {
    return (
      <div role="status" className="mb-4 rounded-md border border-border bg-surface-2 px-4 py-3">
        Port {port} is free: nothing is listening on it.
      </div>
    );
  }
  return (
    <div
      role="status"
      className="mb-4 space-y-3 rounded-md border border-warn/40 bg-warn-soft px-4 py-3"
    >
      <p className="font-medium text-warn">Port {port} is already in use.</p>
      {owners.map((o) => {
        const row = o.pid != null ? processes?.find((p) => p.pid === o.pid) : undefined;
        return (
          <div key={o.pid ?? "unknown"} className="space-y-1.5">
            <p>
              <span className="font-medium">{o.name ?? "Unknown process"}</span>
              {o.pid != null && <span className="num text-fg-muted"> · PID {o.pid}</span>}
              <span className="text-fg-muted"> · {o.listeners.join(", ")}</span>
            </p>
            {row?.exe && <p className="break-all font-mono text-[12px] text-fg-muted">{row.exe}</p>}
            {o.pid == null && (
              <p className="text-fg-muted">
                The owner is hidden from this account. Run as administrator/root to identify it.
              </p>
            )}
            {o.pid != null && (
              <div className="flex flex-wrap gap-2">
                <button type="button" className={btn} onClick={() => openProcess(o.pid as number)}>
                  <FileSearch size={14} aria-hidden /> Open process details
                </button>
                <button
                  type="button"
                  className={btn}
                  onClick={() => void copyText("PID", String(o.pid))}
                >
                  <Copy size={14} aria-hidden /> Copy PID
                </button>
                <button
                  type="button"
                  className={cn(btn, "text-crit hover:text-crit")}
                  disabled={!row}
                  title={row ? undefined : "This process is no longer listed"}
                  onClick={() => row && void requestAction(row, { type: "terminate" })}
                >
                  <Square size={13} aria-hidden /> Terminate…
                </button>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
