import { PageHeader } from "@/components/PageHeader";
import { EmptyState } from "@/components/EmptyState";

export function SystemInfoPage() {
  return (
    <>
      <PageHeader title="System Information" description="Operating system and hardware." />
      <EmptyState title="Coming soon">This section is under construction.</EmptyState>
    </>
  );
}
