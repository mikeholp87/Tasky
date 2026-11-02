import { PageHeader } from "@/components/PageHeader";
import { EmptyState } from "@/components/EmptyState";

export function CpuPage() {
  return (
    <>
      <PageHeader title="CPU" description="Total and per-core utilization." />
      <EmptyState title="Coming soon">This section is under construction.</EmptyState>
    </>
  );
}
