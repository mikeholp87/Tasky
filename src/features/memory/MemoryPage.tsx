import { PageHeader } from "@/components/PageHeader";
import { EmptyState } from "@/components/EmptyState";

export function MemoryPage() {
  return (
    <>
      <PageHeader title="Memory" description="Physical memory and swap." />
      <EmptyState title="Coming soon">This section is under construction.</EmptyState>
    </>
  );
}
