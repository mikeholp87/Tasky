import { ArrowLeft } from "lucide-react";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/EmptyState";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import type { BenchmarkKind } from "@/types";
import { formatDuration, formatTimestamp } from "@/utils/format";
import { KIND_LABELS, PROFILE_LABELS, formatScore } from "../format";
import { linkButton, secondaryButton } from "./styles";
import { TrendCharts } from "./TrendCharts";

/** Stored runs, newest first. `kind` limits the list to one category. */
export function HistoryList({ kind }: { kind?: BenchmarkKind }) {
  const history = useBenchmarkStore((s) => s.history);
  const setView = useBenchmarkStore((s) => s.setView);
  const rows = kind ? history.filter((h) => h.categories.some((c) => c.kind === kind)) : history;
  if (rows.length === 0) {
    return (
      <EmptyState title="No benchmark history yet">
        Results are stored on this computer only and are never uploaded.
      </EmptyState>
    );
  }
  return (
    <Card className="overflow-x-auto">
      <table className="num w-full text-left">
        <caption className="sr-only">Benchmark history</caption>
        <thead className="text-[12px] text-fg-muted">
          <tr className="border-b border-border">
            <th scope="col" className="px-4 py-2 font-medium">
              Date
            </th>
            <th scope="col" className="px-2 py-2 font-medium">
              Tests
            </th>
            <th scope="col" className="px-2 py-2 font-medium">
              Profile
            </th>
            <th scope="col" className="px-2 py-2 text-right font-medium">
              {kind ? `${KIND_LABELS[kind]} score` : "Overall"}
            </th>
            <th scope="col" className="px-2 py-2 text-right font-medium">
              Duration
            </th>
            <th scope="col" className="px-4 py-2">
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((h) => {
            const score = kind
              ? h.categories.find((c) => c.kind === kind)?.score
              : h.systemScore?.value;
            return (
              <tr key={h.id} className="border-b border-border last:border-0">
                <td className="px-4 py-2">
                  {formatTimestamp(h.startedAtMs)}
                  {h.simulated && <span className="ml-2 text-fg-muted">(simulated)</span>}
                </td>
                <td className="px-2 py-2">
                  {h.categories.map((c) => KIND_LABELS[c.kind]).join(", ")}
                </td>
                <td className="px-2 py-2">{PROFILE_LABELS[h.profile]}</td>
                <td className="px-2 py-2 text-right">
                  {formatScore(score)}
                  {!kind && h.systemScore?.partial && " (partial)"}
                </td>
                <td className="px-2 py-2 text-right">{formatDuration(h.durationMs / 1000)}</td>
                <td className="px-4 py-2 text-right">
                  <button
                    type="button"
                    className={linkButton}
                    onClick={() => setView({ name: "result", id: h.id })}
                  >
                    View
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </Card>
  );
}

export function HistoryView() {
  const setView = useBenchmarkStore((s) => s.setView);
  return (
    <div className="space-y-4">
      <button type="button" className={secondaryButton} onClick={() => setView({ name: "home" })}>
        <ArrowLeft size={14} aria-hidden />
        Benchmarks
      </button>
      <TrendCharts />
      <HistoryList />
    </div>
  );
}
