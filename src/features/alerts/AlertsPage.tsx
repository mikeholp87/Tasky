import { PageHeader } from "@/components/PageHeader";
import { EmptyState } from "@/components/EmptyState";

export function AlertsPage() {
  return (
    <>
      <PageHeader title="Alerts" description="Notify me when resources cross thresholds." />
      <EmptyState title="Coming soon">This section is under construction.</EmptyState>
    </>
  );
}
