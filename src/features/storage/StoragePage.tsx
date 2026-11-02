import { PageHeader } from "@/components/PageHeader";
import { EmptyState } from "@/components/EmptyState";

export function StoragePage() {
  return (
    <>
      <PageHeader title="Storage" description="Drives, capacity and activity." />
      <EmptyState title="Coming soon">This section is under construction.</EmptyState>
    </>
  );
}
