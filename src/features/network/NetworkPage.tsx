import { ArrowDown, ArrowUp, Wifi } from "lucide-react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { DeviceChart, type DeviceChartSeries } from "@/components/charts/DeviceChart";
import { rateAxis, RATE_MIN_Y } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList } from "@/components/ui/KeyValueList";
import { networkHistory } from "@/stores/deviceHistory";
import { useMetricsStore } from "@/stores/metricsStore";
import { useSettingsStore } from "@/stores/settingsStore";
import type { NetworkInterfaceSnapshot } from "@/types";
import { formatBytes, formatBytesPerSecond } from "@/utils/format";
import { INTERFACE_KIND_LABEL, visibleInterfaces } from "@/utils/system";

const SERIES: readonly DeviceChartSeries[] = [
  { label: "Download", key: "rx", colorVar: "--res-net" },
  { label: "Upload", key: "tx", colorVar: "--res-net-2", fill: false },
];

export function NetworkPage() {
  const all = useMetricsStore((s) => s.tick?.networks);
  const showVirtual = useSettingsStore((s) => s.showVirtualAdapters);
  const networks = all && visibleInterfaces(all, showVirtual);
  const hidden = all && networks ? all.length - networks.length : 0;
  return (
    <>
      <PageHeader title="Network" description="Interfaces and throughput." />
      {!networks ? (
        <ConnectionNotice />
      ) : networks.length === 0 ? (
        <p className="text-fg-muted">No active network interfaces.</p>
      ) : (
        <div className="space-y-4">
          {networks.map((n) => (
            <InterfaceCard key={n.name} iface={n} />
          ))}
        </div>
      )}
      {hidden > 0 && (
        <p className="mt-4 text-[12px] text-fg-muted">
          {hidden} virtual {hidden === 1 ? "adapter" : "adapters"} (bridges, container networks)
          hidden. Show them in Settings → Monitoring.
        </p>
      )}
    </>
  );
}

function InterfaceCard({ iface: n }: { iface: NetworkInterfaceSnapshot }) {
  return (
    <Card>
      <CardHeader
        title={
          <span className="normal-case tracking-normal text-fg">
            {n.name}
            {INTERFACE_KIND_LABEL[n.kind] && (
              <span className="ml-2 rounded bg-surface-2 px-1.5 py-0.5 text-[11px] font-medium text-fg-muted">
                {INTERFACE_KIND_LABEL[n.kind]}
              </span>
            )}
            {n.isPrimary && (
              <span className="ml-2 rounded bg-accent-soft px-1.5 py-0.5 text-[11px] font-medium text-fg">
                Primary
              </span>
            )}
          </span>
        }
        icon={<Wifi size={14} aria-hidden />}
      />
      <div className="grid gap-6 px-4 pb-4 pt-3 lg:grid-cols-[320px_1fr]">
        <div className="space-y-3">
          <div className="flex gap-6">
            <span className="num flex items-center gap-1 text-xl font-semibold">
              <ArrowDown size={16} className="text-[var(--res-net)]" aria-label="Download" />
              {formatBytesPerSecond(n.rxBytesPerSec)}
            </span>
            <span className="num flex items-center gap-1 text-xl font-semibold">
              <ArrowUp size={16} className="text-[var(--res-net-2)]" aria-label="Upload" />
              {formatBytesPerSecond(n.txBytesPerSec)}
            </span>
          </div>
          <KeyValueList
            items={[
              { label: "Total received", value: formatBytes(n.totalRxBytes) },
              { label: "Total sent", value: formatBytes(n.totalTxBytes) },
              { label: "IPv4", value: n.ipv4.join(", ") || "—" },
              {
                label: "IPv6",
                value: n.ipv6.length ? (
                  <span className="flex flex-col items-end">
                    {n.ipv6.map((ip) => (
                      <span key={ip}>{ip}</span>
                    ))}
                  </span>
                ) : (
                  "—"
                ),
              },
              { label: "MAC address", value: n.macAddress ?? "—" },
            ]}
          />
        </div>
        <DeviceChart
          source={networkHistory}
          device={n.name}
          series={SERIES}
          height={190}
          formatValue={rateAxis}
          minYMax={RATE_MIN_Y}
          ariaLabel={`Download and upload rate of ${n.name}`}
        />
      </div>
    </Card>
  );
}
