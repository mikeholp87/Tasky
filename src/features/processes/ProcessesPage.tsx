import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { useMetricsStore } from "@/stores/metricsStore";
import { useMemoryDetail } from "@/features/memory/useMemoryDetail";
import { requestAction } from "./actions";
import { ProcessContextMenu } from "./ProcessContextMenu";
import { ProcessTable } from "./ProcessTable";
import type { ProcessRow } from "@/types";

const withContextMenu = (
  table: React.ReactElement,
  find: (pid: number) => ProcessRow | undefined,
) => <ProcessContextMenu find={find}>{table}</ProcessContextMenu>;
const requestTerminate = (row: ProcessRow) => void requestAction(row, { type: "terminate" });
import { ProcessToolbar } from "./ProcessToolbar";
import { useProcessView } from "./useProcessView";

export function ProcessesPage() {
  // Proportional memory, so every view shows the figure the Memory page does.
  useMemoryDetail();
  const hasTick = useMetricsStore((s) => s.tick !== null);
  const selfPid = useMetricsStore((s) => s.staticInfo?.selfPid);
  const { rows, items, ctx } = useProcessView();

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader title="Processes" description="Every running process, updated live." />
      {!hasTick ? (
        <ConnectionNotice />
      ) : (
        <>
          <ProcessToolbar shown={items.length} total={rows.length} rows={rows} />
          <ProcessTable
            items={items}
            ctx={ctx}
            selfPid={selfPid}
            tree
            wrapTable={withContextMenu}
            onRequestTerminate={requestTerminate}
          />
        </>
      )}
    </div>
  );
}
