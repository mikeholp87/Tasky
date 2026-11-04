import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList } from "@/components/ui/KeyValueList";
import { useMetricsStore } from "@/stores/metricsStore";
import { usePerfStore } from "@/stores/perfStore";
import { formatBytes, formatCount } from "@/utils/format";

const MOCK_COUNTS = Array.of(0, 100, 500, 1000, 5000);
const ms = (v: number) => `${v.toFixed(1)} ms`;

/** Live cost of monitoring, to keep performance claims measurable. */
export function DiagnosticsCard() {
  const engine = useMetricsStore((s) => s.tick?.engine);
  const ingest = usePerfStore((s) => s.ingestMs);
  const frame = usePerfStore((s) => s.frameMs);
  const mock = usePerfStore((s) => s.mockProcessCount);
  const setMock = usePerfStore((s) => s.setMockProcessCount);
  if (!engine) return null;

  return (
    <Card>
      <CardHeader title="Monitoring diagnostics" />
      <div className="space-y-3 px-4 pb-4 pt-3">
        <KeyValueList
          items={[
            {
              label: "Backend poll time",
              value: ms(engine.pollDurationMs),
              hint: "Time to sample the OS for one tick.",
            },
            { label: "Refresh interval", value: `${engine.intervalMs} ms` },
            { label: "Processes", value: formatCount(engine.processCount) },
            {
              label: "Tick payload",
              value: formatBytes(engine.lastPayloadBytes),
              hint: "Serialized size of the previous tick sent to this window.",
            },
            {
              label: "Frontend ingest",
              value: ms(ingest),
              hint: "Store and history updates for one tick.",
            },
            {
              label: "Tick to next frame",
              value: ms(frame),
              hint: "Ingest plus React rendering and chart drawing.",
            },
          ]}
        />
        {import.meta.env.DEV && (
          <label className="flex items-center justify-between gap-3 border-t border-border pt-3">
            <span>
              <span className="font-medium">Mock processes</span>
              <span className="block text-fg-muted">
                Development builds only. Replaces the real list.
              </span>
            </span>
            <select
              value={mock}
              onChange={(e) => setMock(Number(e.target.value))}
              className="h-8 rounded-md border border-border bg-surface px-2"
            >
              {MOCK_COUNTS.map((n) => (
                <option key={n} value={n}>
                  {n === 0 ? "Off" : formatCount(n)}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
    </Card>
  );
}
