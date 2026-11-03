import { useMemo, useState } from "react";
import { EmptyState } from "@/components/EmptyState";
import { PageHeader } from "@/components/PageHeader";
import { SearchBox } from "@/components/SearchBox";
import { Card } from "@/components/ui/Card";
import { cn } from "@/lib/cn";
import { useActivityStore } from "@/stores/activityStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { useUiStore } from "@/stores/uiStore";
import { FILTER_LABEL, clock, describe, filterEvents, type ActivityFilter } from "./logic";

const FILTERS = Object.keys(FILTER_LABEL) as ActivityFilter[];

export function ActivityPage() {
  const events = useActivityStore((s) => s.events);
  const cpuSpike = useSettingsStore((s) => s.cpuSpikePercent);
  const memoryJump = useSettingsStore((s) => s.memoryJumpMb);
  const openProcess = useUiStore((s) => s.openProcess);
  const navigate = useUiStore((s) => s.navigate);
  const [filter, setFilter] = useState<ActivityFilter>("all");
  const [query, setQuery] = useState("");
  const shown = useMemo(() => filterEvents(events, filter, query), [events, filter, query]);

  return (
    <>
      <PageHeader
        title="Activity"
        description={`Process starts and exits, sustained CPU above ${cpuSpike}%, memory growth over ${memoryJump} MB, alerts and actions. Newest first; the last 1,000 events are kept while the app runs.`}
        actions={
          <button
            type="button"
            onClick={() => navigate("settings")}
            className="flex h-8 items-center rounded-md border border-border px-2.5 text-fg-muted hover:text-fg"
          >
            Thresholds…
          </button>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <SearchBox
          value={query}
          onChange={setQuery}
          label="Search activity"
          placeholder="Process, PID, detail…"
        />
        <div
          role="radiogroup"
          aria-label="Event type"
          className="flex rounded-md border border-border p-0.5"
        >
          {FILTERS.map((f) => (
            <button
              key={f}
              type="button"
              role="radio"
              aria-checked={filter === f}
              onClick={() => setFilter(f)}
              className={cn(
                "h-7 rounded px-2.5 text-fg-muted",
                filter === f && "bg-surface-2 text-fg",
              )}
            >
              {FILTER_LABEL[f]}
            </button>
          ))}
        </div>
        <span className="text-fg-muted">{shown.length} shown</span>
      </div>

      {shown.length === 0 ? (
        <EmptyState title={events.length === 0 ? "Nothing has happened yet" : "No matching events"}>
          {events.length === 0
            ? "Events appear here as processes start and exit and as usage changes."
            : "Try a different filter or search."}
        </EmptyState>
      ) : (
        <Card>
          <ol className="num divide-y divide-border/60">
            {shown.map((e) => (
              <li
                key={e.seq}
                className="grid grid-cols-[72px_1fr_auto] items-baseline gap-3 px-4 py-2"
              >
                <time
                  className="font-mono text-fg-muted"
                  dateTime={new Date(e.timestampMs).toISOString()}
                >
                  {clock(e.timestampMs)}
                </time>
                <span>
                  {describe(e)}
                  {e.detail && <span className="text-fg-muted"> · {e.detail}</span>}
                </span>
                {e.pid != null && e.kind !== "processExited" ? (
                  <button
                    type="button"
                    className="font-mono text-accent hover:underline"
                    onClick={() => openProcess(e.pid as number)}
                  >
                    PID {e.pid}
                  </button>
                ) : (
                  <span className="font-mono text-fg-subtle">
                    {e.pid != null ? `PID ${e.pid}` : ""}
                  </span>
                )}
              </li>
            ))}
          </ol>
        </Card>
      )}
    </>
  );
}
