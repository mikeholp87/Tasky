import { ArrowLeft, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { Card } from "@/components/ui/Card";
import { EmptyState } from "@/components/EmptyState";
import { cn } from "@/lib/cn";
import { backend, toAppError } from "@/services/ipc";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import type { BenchmarkComparison } from "@/types";
import { formatTimestamp } from "@/utils/format";
import { PROFILE_LABELS, formatChange, formatMetricText } from "../format";
import { secondaryButton, selectClass } from "./styles";

interface Props {
  baselineId?: string;
  currentId?: string;
}

interface Loaded {
  key: string;
  comparison: BenchmarkComparison | null;
  error: string | null;
}

export function CompareView({ baselineId, currentId }: Props) {
  const history = useBenchmarkStore((s) => s.history);
  const setView = useBenchmarkStore((s) => s.setView);
  const current = currentId ?? history[0]?.id;
  const baseline = baselineId ?? history[1]?.id;
  const key = `${baseline}|${current}`;
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  useEffect(() => {
    if (!baseline || !current || baseline === current) return;
    let cancelled = false;
    backend
      .compareBenchmarks(baseline, current)
      .then((comparison) => !cancelled && setLoaded({ key, comparison, error: null }))
      .catch(
        (e: unknown) =>
          !cancelled && setLoaded({ key, comparison: null, error: toAppError(e).message }),
      );
    return () => {
      cancelled = true;
    };
  }, [baseline, current, key]);

  const back = (
    <button type="button" className={secondaryButton} onClick={() => setView({ name: "home" })}>
      <ArrowLeft size={14} aria-hidden />
      Benchmarks
    </button>
  );
  if (history.length < 2) {
    return (
      <div className="space-y-4">
        {back}
        <EmptyState title="Nothing to compare yet">
          Run at least two benchmarks to compare results over time.
        </EmptyState>
      </div>
    );
  }

  const option = (id: string) => {
    const h = history.find((x) => x.id === id);
    return h
      ? `${formatTimestamp(h.startedAtMs)} · ${PROFILE_LABELS[h.profile]}${h.simulated ? " · simulated" : ""}`
      : id;
  };
  const select = (label: string, value: string | undefined, onChange: (id: string) => void) => (
    <label className="flex flex-col gap-1">
      <span className="text-fg-muted">{label}</span>
      <select
        className={selectClass}
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value)}
      >
        {history.map((h) => (
          <option key={h.id} value={h.id}>
            {option(h.id)}
          </option>
        ))}
      </select>
    </label>
  );
  const comparison = loaded?.key === key ? loaded.comparison : null;
  const error = loaded?.key === key ? loaded.error : null;

  return (
    <div className="space-y-4">
      {back}
      <Card className="flex flex-wrap gap-4 p-4">
        {select("Previous", baseline, (id) =>
          setView({ name: "compare", baselineId: id, currentId: current }),
        )}
        {select("Current", current, (id) =>
          setView({ name: "compare", baselineId: baseline, currentId: id }),
        )}
      </Card>
      {baseline === current && <p className="text-fg-muted">Choose two different runs.</p>}
      {error && (
        <p role="alert" className="text-crit">
          {error}
        </p>
      )}
      {comparison && !comparison.compatibleMethod && (
        <p role="alert" className="flex gap-2 rounded-md bg-warn-soft p-3 text-warn">
          <TriangleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
          These runs used different benchmark methodology or score versions. Their values are listed
          for reference only and are not compared.
        </p>
      )}
      {comparison?.hardwareChanged && (
        <div role="alert" className="flex gap-2 rounded-md bg-warn-soft p-3 text-warn">
          <TriangleAlert size={16} className="mt-0.5 shrink-0" aria-hidden />
          <div>
            <p>Hardware configuration changed. Direct comparison may not be meaningful.</p>
            <ul className="mt-1 list-disc pl-5">
              {comparison.hardwareChanges.map((c) => (
                <li key={c}>{c}</li>
              ))}
            </ul>
          </div>
        </div>
      )}
      {comparison && !comparison.hardwareChanged && comparison.hardwareChanges.length > 0 && (
        <p className="text-fg-muted">Also changed: {comparison.hardwareChanges.join("; ")}</p>
      )}
      {comparison && (
        <Card className="overflow-x-auto">
          <table className="num w-full text-left">
            <caption className="sr-only">Benchmark comparison</caption>
            <thead className="text-[12px] text-fg-muted">
              <tr className="border-b border-border">
                <th scope="col" className="px-4 py-2 font-medium">
                  Measurement
                </th>
                <th scope="col" className="px-2 py-2 text-right font-medium">
                  Previous
                </th>
                <th scope="col" className="px-2 py-2 text-right font-medium">
                  Current
                </th>
                <th scope="col" className="px-4 py-2 text-right font-medium">
                  Change
                </th>
              </tr>
            </thead>
            <tbody>
              {comparison.rows.map((r) => {
                const better = r.higherIsBetter ? r.changePercent > 0 : r.changePercent < 0;
                const meaningful = comparison.compatibleMethod && !r.withinNoise;
                return (
                  <tr key={r.id} className="border-b border-border last:border-0">
                    <th scope="row" className="px-4 py-2 font-normal">
                      {r.label}
                    </th>
                    <td className="px-2 py-2 text-right">{formatMetricText(r.baseline, r.unit)}</td>
                    <td className="px-2 py-2 text-right">{formatMetricText(r.current, r.unit)}</td>
                    <td
                      className={cn(
                        "px-4 py-2 text-right",
                        meaningful && (better ? "text-ok" : "text-crit"),
                      )}
                    >
                      {comparison.compatibleMethod ? (
                        <>
                          {formatChange(r.changePercent)}
                          <span className="ml-1 text-[12px] text-fg-muted">
                            {r.withinNoise ? "(within variation)" : better ? "better" : "worse"}
                          </span>
                        </>
                      ) : (
                        "Not comparable"
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </Card>
      )}
    </div>
  );
}
