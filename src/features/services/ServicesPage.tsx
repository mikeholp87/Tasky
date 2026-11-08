import { CircleAlert, CircleCheck, CircleMinus, Lock, RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { SearchBox } from "@/components/SearchBox";
import { cn } from "@/lib/cn";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import { useUiStore } from "@/stores/uiStore";
import { useMetricsStore } from "@/stores/metricsStore";
import type { ServiceAction, ServiceInfo, ServiceScope } from "@/types";
import { type ServiceFilter, filterServices, stateLabel } from "./logic";

const POLL_MS = 3000;
const btn =
  "flex h-7 items-center rounded-md border border-border px-2 text-fg-muted hover:text-fg disabled:opacity-50";

const SCOPES: readonly { id: ServiceScope; label: string }[] = [
  { id: "system", label: "System" },
  { id: "user", label: "Your account" },
];
const FILTERS: readonly { id: ServiceFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "running", label: "Running" },
  { id: "failed", label: "Failed" },
  { id: "stopped", label: "Stopped" },
];

const DONE: Record<ServiceAction, string> = {
  start: "Starting",
  stop: "Stopping",
  restart: "Restarting",
};

export function ServicesPage() {
  const windows = useMetricsStore((s) => s.staticInfo?.platform === "windows");
  const [scope, setScope] = useState<ServiceScope>("system");
  const [filter, setFilter] = useState<ServiceFilter>("all");
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const openProcess = useUiStore((s) => s.openProcess);
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.listServices(scope),
    scope,
    POLL_MS,
  );
  const shown = useMemo(
    () => (data ? filterServices(data, query, filter) : []),
    [data, query, filter],
  );
  const failed = data?.filter((s) => s.activeState === "failed").length ?? 0;

  async function act(s: ServiceInfo, action: ServiceAction) {
    setPending(s.unit);
    try {
      await backend.controlService(scope, s.unit, action);
      toast.success(`${DONE[action]} ${s.unit}…`);
    } catch (e) {
      const err = toAppError(e);
      // Declining the confirmation is a choice, not an error.
      if (err.kind !== "operationCancelled") toast.error(err.message);
    } finally {
      setPending(null);
      refresh();
    }
  }

  return (
    <>
      <PageHeader
        title="Services"
        description={
          failed > 0
            ? `${failed} failed. Start, stop or restart background services.`
            : "Start, stop or restart background services."
        }
        actions={
          <button type="button" className={cn(btn, "h-8 gap-1.5 px-2.5")} onClick={refresh}>
            <RefreshCw size="0.875rem" aria-hidden /> Refresh
          </button>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <div
          role="group"
          aria-label="Services of"
          className="flex overflow-hidden rounded-md border border-border"
        >
          {SCOPES.filter((o) => !windows || o.id === "system").map((o) => (
            <button
              key={o.id}
              type="button"
              aria-pressed={scope === o.id}
              onClick={() => setScope(o.id)}
              className={cn(
                "h-8 px-3 text-fg-muted hover:text-fg",
                scope === o.id && "bg-surface-2 font-medium text-fg",
              )}
            >
              {o.label}
            </button>
          ))}
        </div>
        <SearchBox
          value={query}
          onChange={setQuery}
          placeholder="Search name, description, PID…"
          label="Search services"
        />
        <div
          role="group"
          aria-label="Show"
          className="flex overflow-hidden rounded-md border border-border"
        >
          {FILTERS.map((o) => (
            <button
              key={o.id}
              type="button"
              aria-pressed={filter === o.id}
              onClick={() => setFilter(o.id)}
              className={cn(
                "h-8 px-3 text-fg-muted hover:text-fg",
                filter === o.id && "bg-surface-2 font-medium text-fg",
              )}
            >
              {o.label}
            </button>
          ))}
        </div>
        {data && <span className="text-fg-muted">{shown.length} services</span>}
      </div>

      {error && !data ? (
        <EmptyState title="Services unavailable">{error.message}</EmptyState>
      ) : loading ? (
        <EmptyState title="Loading…" />
      ) : shown.length === 0 ? (
        <EmptyState title="No services match" />
      ) : (
        <div className="overflow-hidden rounded-lg border border-border bg-surface">
          <table className="w-full text-left">
            <thead className="bg-surface-2 text-[0.8125rem] font-medium text-fg-muted">
              <tr>
                <th className="px-3 py-2">Service</th>
                <th className="px-3 py-2">State</th>
                <th className="px-3 py-2">At boot</th>
                <th className="px-3 py-2 text-right">PID</th>
                <th className="px-3 py-2 text-right">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((s) => (
                <Row
                  key={s.unit}
                  service={s}
                  busy={
                    pending !== null ||
                    ["activating", "deactivating", "reloading"].includes(s.activeState)
                  }
                  onAct={(a) => void act(s, a)}
                  onOpen={() => s.mainPid != null && openProcess(s.mainPid)}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

function StateBadge({ service }: { service: ServiceInfo }) {
  const label = stateLabel(service);
  const failed = service.activeState === "failed";
  const on = service.activeState === "active" || service.activeState === "activating";
  // A shape as well as a colour: check, cross-circle or dash.
  const Icon = failed ? CircleAlert : on ? CircleCheck : CircleMinus;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5",
        failed && "text-crit",
        on && !failed && "text-ok",
        !on && !failed && "text-fg-muted",
      )}
    >
      <Icon size="0.875rem" aria-hidden />
      {label}
    </span>
  );
}

function Row({
  service: s,
  busy,
  onAct,
  onOpen,
}: {
  service: ServiceInfo;
  busy: boolean;
  onAct: (action: ServiceAction) => void;
  onOpen: () => void;
}) {
  const running = s.activeState === "active" || s.activeState === "activating";
  return (
    <tr className="border-t border-border/60 hover:bg-surface-2">
      <td className="min-w-0 px-3 py-2">
        <div className="font-medium">{s.unit}</div>
        {s.description && (
          <div className="truncate text-[0.8125rem] text-fg-muted">{s.description}</div>
        )}
      </td>
      <td className="px-3 py-2">
        <StateBadge service={s} />
      </td>
      <td className="px-3 py-2 text-fg-muted">{s.startup ?? "—"}</td>
      <td className="num px-3 py-2 text-right">
        {s.mainPid != null ? (
          <button
            type="button"
            onClick={onOpen}
            aria-label={`Open process ${s.mainPid} of ${s.unit}`}
            className="text-accent underline"
          >
            {s.mainPid}
          </button>
        ) : (
          "—"
        )}
      </td>
      <td className="px-3 py-2">
        <div className="flex justify-end gap-1.5">
          {s.protected ? (
            <span
              className="flex items-center gap-1 text-[0.8125rem] text-fg-subtle"
              title="System-critical: can't be changed from Tasky."
            >
              <Lock size="0.8125rem" aria-hidden /> Protected
            </span>
          ) : (
            <>
              {!running && (
                <button
                  type="button"
                  className={btn}
                  disabled={busy}
                  onClick={() => onAct("start")}
                  aria-label={`Start ${s.unit}`}
                >
                  Start
                </button>
              )}
              {running && (
                <>
                  <button
                    type="button"
                    className={btn}
                    disabled={busy}
                    onClick={() => onAct("restart")}
                    aria-label={`Restart ${s.unit}`}
                  >
                    Restart
                  </button>
                  <button
                    type="button"
                    className={btn}
                    disabled={busy}
                    onClick={() => onAct("stop")}
                    aria-label={`Stop ${s.unit}`}
                  >
                    Stop
                  </button>
                </>
              )}
            </>
          )}
        </div>
      </td>
    </tr>
  );
}
