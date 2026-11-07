import { useMemo, useState } from "react";
import { RefreshCw } from "lucide-react";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { SearchBox } from "@/components/SearchBox";
import { Card } from "@/components/ui/Card";
import { Switch } from "@/components/ui/Switch";
import { cn } from "@/lib/cn";
import { useRemoteData } from "@/hooks/useRemoteData";
import { backend, isBackendAvailable, toAppError } from "@/services/ipc";
import { toast } from "@/stores/toastStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { filterItems } from "./logic";

const btn =
  "flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-fg-muted hover:text-fg disabled:opacity-50";

export function StartupPage() {
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const { data, error, loading, refresh } = useRemoteData(
    () => backend.listStartupItems(),
    "startup",
  );
  const shown = useMemo(() => (data ? filterItems(data, query) : []), [data, query]);
  const willStart = data?.filter((i) => i.enabled && i.runsHere).length ?? 0;

  async function toggle(id: string, name: string, enabled: boolean) {
    setPending((p) => new Set(p).add(id));
    try {
      await backend.setStartupEnabled(id, enabled);
      const settings = await backend.getSettings();
      if (settings.startWithOs !== useSettingsStore.getState().startWithOs) {
        useSettingsStore.getState().update({ startWithOs: settings.startWithOs });
      }
      toast.success(`${name} will ${enabled ? "" : "no longer "}start at login.`);
    } catch (e) {
      toast.error(toAppError(e).message);
    } finally {
      setPending((p) => {
        const next = new Set(p);
        next.delete(id);
        return next;
      });
      refresh();
    }
  }

  return (
    <>
      <PageHeader
        title="Startup"
        description="Programs that start when you log in. Turning one off hides it; nothing is deleted."
        actions={
          <button type="button" className={btn} onClick={refresh}>
            <RefreshCw size="0.875rem" aria-hidden /> Refresh
          </button>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <SearchBox
          value={query}
          onChange={setQuery}
          label="Search startup items"
          placeholder="Name or command…"
        />
        {data && (
          <span className="text-fg-muted">
            {willStart} of {data.length} start at login
          </span>
        )}
      </div>

      {!isBackendAvailable() ? (
        <EmptyState title="Backend not available">
          Open the desktop app to see startup items.
        </EmptyState>
      ) : error && !data ? (
        <EmptyState title="Unable to list startup items">{error.message}</EmptyState>
      ) : loading ? (
        <EmptyState title="Loading…" />
      ) : shown.length === 0 ? (
        <EmptyState title="No startup items">
          {query ? "Nothing matches your search." : "No programs are set to start at login."}
        </EmptyState>
      ) : (
        <Card className="overflow-auto">
          <table className="w-full text-left text-[0.8125rem]">
            <thead className="sticky top-0 bg-surface-2 text-fg-muted">
              <tr>
                <th className="px-3 py-2 font-medium">Name</th>
                <th className="px-3 py-2 font-medium">Command</th>
                <th className="px-3 py-2 font-medium">Source</th>
                <th className="px-3 py-2 font-medium">Status</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((i) => (
                <tr key={i.id} className="border-t border-border/60">
                  <td className={cn("px-3 py-1.5", !(i.enabled && i.runsHere) && "text-fg-subtle")}>
                    <div className="font-medium">{i.name}</div>
                    {i.comment && <div className="text-fg-muted">{i.comment}</div>}
                    {i.onlyIn && <div className="text-fg-muted">Only in {i.onlyIn}</div>}
                  </td>
                  <td className="selectable break-all px-3 py-1.5 font-mono text-fg-muted">
                    {i.command}
                  </td>
                  <td className="px-3 py-1.5">{i.scope === "user" ? "You" : "System"}</td>
                  <td className="px-3 py-1.5">
                    <div
                      className={cn("flex items-center gap-2", pending.has(i.id) && "opacity-50")}
                    >
                      <Switch
                        label={`Start ${i.name} at login`}
                        checked={i.enabled}
                        onChange={(v) => {
                          if (!pending.has(i.id)) void toggle(i.id, i.name, v);
                        }}
                      />
                      <span
                        className="text-fg-muted"
                        title={
                          i.enabled && !i.runsHere
                            ? `This desktop does not start it${i.onlyIn ? ` (only in ${i.onlyIn})` : ""}.`
                            : undefined
                        }
                      >
                        {!i.enabled ? "Disabled" : i.runsHere ? "Enabled" : "Not used here"}
                      </span>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
    </>
  );
}
