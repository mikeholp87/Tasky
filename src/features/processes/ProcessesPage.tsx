import { PageHeader } from "@/components/PageHeader";
import { EmptyState } from "@/components/EmptyState";

export function ProcessesPage() {
  return (
    <>
      <PageHeader title="Processes" description="Every running process, live." />
      <EmptyState title="Coming soon">This section is under construction.</EmptyState>
    </>
  );
}
