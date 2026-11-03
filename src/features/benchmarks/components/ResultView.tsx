import { ArrowLeft, Info, Trash2, TriangleAlert } from "lucide-react";
import { Card, CardHeader } from "@/components/ui/Card";
import { EmptyState } from "@/components/EmptyState";
import { deleteResult } from "@/services/benchmarks";
import { useBenchmarkStore } from "@/stores/benchmarkStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { CategoryResult, Metric } from "@/types";
import { useBenchmarkRecord } from "../hooks";
import { formatDuration, formatTimestamp } from "@/utils/format";
import {
  CONFIDENCE_LABELS,
  KIND_LABELS,
  PROFILE_LABELS,
  STATUS_LABELS,
  formatMetric,
  formatMetricText,
  formatScore,
} from "../format";
import { ResultActions } from "./ResultActions";
import { secondaryButton } from "./styles";

export function MetricRow({ metric, showRaw }: { metric: Metric; showRaw?: boolean }) {
  const f = formatMetric(metric.value, metric.unit);
  const s = metric.summary;
  return (
    <div className="py-2">
      <div className="flex items-baseline justify-between gap-3">
        <span>{metric.label}</span>
        <span className="num text-right">
          <span className="text-base font-semibold">{f.value}</span>{" "}
          <span className="text-fg-muted">{f.unit}</span>
        </span>
      </div>
      {s && s.n > 1 && (
        <p className="num mt-0.5 text-right text-[12px] text-fg-muted">
          Median of {s.n} · p95 {formatMetric(s.p95, metric.unit).value} · variation{" "}
          {s.cvPercent.toFixed(1)}%
          {metric.outliers.length > 0 && ` · ${metric.outliers.length} outlier(s) flagged`}
        </p>
      )}
      {metric.note && <p className="mt-0.5 text-[12px] text-fg-muted">{metric.note}</p>}
      {showRaw && metric.samples.length > 1 && (
        <p className="num mt-0.5 break-words text-[12px] text-fg-subtle">
          Raw: {metric.samples.map((v) => formatMetric(v, metric.unit).value).join(", ")}
        </p>
      )}
    </div>
  );
}

function CategoryBlock({ category, showRaw }: { category: CategoryResult; showRaw: boolean }) {
  return (
    <Card>
      <CardHeader
        title={KIND_LABELS[category.kind]}
        action={
          <span className="num text-[12px] text-fg-muted">
            {category.status === "completed"
              ? `Score ${formatScore(category.score)}`
              : STATUS_LABELS[category.status]}
          </span>
        }
      />
      <div className="divide-y divide-border px-4 pb-3">
        {category.metrics.map((m) => (
          <MetricRow key={m.id} metric={m} showRaw={showRaw} />
        ))}
        {category.error && <p className="py-2 text-crit">{category.error.message}</p>}
        {category.thermal?.peakCelsius != null && (
          <dl className="num grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 py-2">
            <dt className="text-fg-muted">Temperature before</dt>
            <dd>{formatMetricText(category.thermal.beforeCelsius, "celsius")}</dd>
            <dt className="text-fg-muted">Peak temperature</dt>
            <dd>{formatMetricText(category.thermal.peakCelsius, "celsius")}</dd>
            <dt className="text-fg-muted">Temperature after</dt>
            <dd>{formatMetricText(category.thermal.afterCelsius, "celsius")}</dd>
          </dl>
        )}
        {category.confidence && (
          <p className="py-2 text-fg-muted">
            Result stability: <strong>{CONFIDENCE_LABELS[category.confidence]}</strong>
            <span className="sr-only"> (consistency of repeated measurements)</span>
          </p>
        )}
        {category.details.length > 0 && (
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 py-2 text-[12px]">
            {category.details.map((d) => (
              <div key={d.key} className="contents">
                <dt className="text-fg-muted">{d.label}</dt>
                <dd>{d.value}</dd>
              </div>
            ))}
          </dl>
        )}
        {category.warnings.map((w, i) => (
          <p key={`${w.code}-${i}`} className="flex gap-2 py-2 text-warn">
            <TriangleAlert size={14} className="mt-0.5 shrink-0" aria-hidden />
            <span>{w.message}</span>
          </p>
        ))}
      </div>
    </Card>
  );
}

export function ResultView({ id }: { id: string }) {
  const { record, error } = useBenchmarkRecord(id);
  const setView = useBenchmarkStore((s) => s.setView);
  const showRaw = useSettingsStore((s) => s.benchmark.showRawMeasurements);

  const back = (
    <button type="button" className={secondaryButton} onClick={() => setView({ name: "home" })}>
      <ArrowLeft size={14} aria-hidden />
      Benchmarks
    </button>
  );
  if (error) return <EmptyState title="Result unavailable">{error}</EmptyState>;
  if (!record) return <p className="text-fg-muted">Loading result…</p>;

  const score = record.systemScore;
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        {back}
        <div className="flex flex-wrap gap-2">
          <ResultActions id={record.id} />
          <button
            type="button"
            className={secondaryButton}
            onClick={() => {
              if (window.confirm("Delete this benchmark result?")) void deleteResult(record.id);
            }}
          >
            <Trash2 size={14} aria-hidden />
            Delete
          </button>
        </div>
      </div>
      <Card className="p-4">
        <h2 className="text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
          {record.simulated ? "Simulated benchmark" : "System benchmark"}
        </h2>
        <div className="mt-2 flex flex-wrap items-end gap-x-8 gap-y-2">
          <div>
            <p className="text-fg-muted">
              {score?.partial ? "Overall score (partial benchmark)" : "Overall score"}
            </p>
            <p className="num text-3xl font-semibold">{formatScore(score?.value)}</p>
          </div>
          <dl className="num grid grid-cols-[auto_auto] gap-x-4 gap-y-0.5">
            <dt className="text-fg-muted">Date</dt>
            <dd>{formatTimestamp(record.startedAtMs)}</dd>
            <dt className="text-fg-muted">Profile</dt>
            <dd>{PROFILE_LABELS[record.profile]}</dd>
            <dt className="text-fg-muted">Duration</dt>
            <dd>{formatDuration(record.durationMs / 1000)}</dd>
            <dt className="text-fg-muted">Method</dt>
            <dd>
              v{record.methodVersion} · score v{record.scoreVersion}
            </dd>
          </dl>
        </div>
        {!score && (
          <p className="mt-2 flex gap-2 text-fg-muted">
            <Info size={14} className="mt-0.5 shrink-0" aria-hidden />
            An overall score needs at least two of CPU, GPU and Storage. Internet results are
            reported separately.
          </p>
        )}
        {record.warnings.map((w, i) => (
          <p key={`${w.code}-${i}`} className="mt-2 flex gap-2 text-warn">
            <TriangleAlert size={14} className="mt-0.5 shrink-0" aria-hidden />
            <span>{w.message}</span>
          </p>
        ))}
      </Card>
      <div className="grid gap-4 lg:grid-cols-2">
        {record.categories.map((c) => (
          <CategoryBlock key={c.kind} category={c} showRaw={showRaw} />
        ))}
      </div>
    </div>
  );
}
