import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { useMetricsStore } from "@/stores/metricsStore";
import { useProcessStore } from "@/stores/processStore";
import { ProcessTable } from "./ProcessTable";
import { ProcessToolbar } from "./ProcessToolbar";
import { useProcessView } from "./useProcessView";

export function ProcessesPage() {
  const hasTick = useMetricsStore((s) => s.tick !== null);
  const selfPid = useMetricsStore((s) => s.staticInfo?.selfPid);
  const mode = useProcessStore((s) => s.mode);
  const { rows, items, ctx } = useProcessView();

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader title="Processes" description="Every running process, updated live." />
      {!hasTick ? (
        <ConnectionNotice />
      ) : (
        <>
          <ProcessToolbar shown={items.length} total={rows.length} rows={rows} />
          <ProcessTable items={items} ctx={ctx} selfPid={selfPid} tree={mode === "tree"} />
        </>
      )}
    </div>
  );
}
