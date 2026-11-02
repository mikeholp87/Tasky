import { Cpu, HardDrive, MemoryStick, Network } from "lucide-react";
import type { ReactNode } from "react";
import { Card, CardHeader } from "@/components/ui/Card";
import { PageHeader } from "@/components/PageHeader";

export function OverviewPage() {
  return (
    <>
      <PageHeader title="Overview" description="What your computer is doing right now." />
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <MetricCard title="CPU" icon={<Cpu size={14} />} />
        <MetricCard title="Memory" icon={<MemoryStick size={14} />} />
        <MetricCard title="Storage" icon={<HardDrive size={14} />} />
        <MetricCard title="Network" icon={<Network size={14} />} />
      </div>
    </>
  );
}

function MetricCard({ title, icon }: { title: string; icon: ReactNode }) {
  return (
    <Card>
      <CardHeader title={title} icon={icon} />
      <div className="px-4 pb-4 pt-2">
        <div className="num text-3xl font-semibold text-fg-subtle">—</div>
        <div className="mt-3 h-20 rounded-md bg-surface-2" aria-hidden />
      </div>
    </Card>
  );
}
