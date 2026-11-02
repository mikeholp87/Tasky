import { PageHeader } from "@/components/PageHeader";
import { EmptyState } from "@/components/EmptyState";

export function PerformancePage() {
  return (
    <>
      <PageHeader title="Performance" description="Live resource graphs." />
      <EmptyState title="Coming soon">This section is under construction.</EmptyState>
    </>
  );
}
