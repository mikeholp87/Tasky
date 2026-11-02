import { ConnectionNotice } from "@/components/ConnectionNotice";
import { EmptyState } from "@/components/EmptyState";
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
  const { rows, view, ctx } = useProcessView();

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader title="Processes" description="Every running process, updated live." />
      {!hasTick ? (
        <ConnectionNotice />
      ) : (
        <>
          <ProcessToolbar shown={view.length} total={rows.length} />
          {mode === "list" ? (
            <ProcessTable rows={view} ctx={ctx} selfPid={selfPid} />
          ) : (
            <EmptyState title="Tree view">Coming in the next update.</EmptyState>
          )}
        </>
      )}
    </div>
  );
}
