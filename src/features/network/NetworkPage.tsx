import { Activity, ArrowDown, ArrowUp, Globe, Wifi } from "lucide-react";
import { useMemo } from "react";
import { ConnectionNotice } from "@/components/ConnectionNotice";
import { PageHeader } from "@/components/PageHeader";
import { DeviceChart, type DeviceChartSeries } from "@/components/charts/DeviceChart";
import { rateAxis, RATE_MIN_Y } from "@/components/charts/series";
import { Card, CardHeader } from "@/components/ui/Card";
import { KeyValueList } from "@/components/ui/KeyValueList";
import { networkHistory } from "@/stores/deviceHistory";
import { useMetricsStore } from "@/stores/metricsStore";
import type { NetworkInterfaceSnapshot } from "@/types";
import { formatBytes, formatBytesPerSecond } from "@/utils/format";

const SERIES: readonly DeviceChartSeries[] = [
  { label: "Download", key: "rx", colorVar: "--chart-1" },
  { label: "Upload", key: "tx", colorVar: "--chart-2", fill: false },
];

export function NetworkPage() {
  const networks = useMetricsStore((s) => s.tick?.networks);

  const overview = useMemo(() => {
    if (!networks || networks.length === 0) return null;
    let rxBytesPerSec = 0;
    let txBytesPerSec = 0;
    let totalRxBytes = 0;
    let totalTxBytes = 0;

    for (const n of networks) {
      rxBytesPerSec += n.rxBytesPerSec;
      txBytesPerSec += n.txBytesPerSec;
      totalRxBytes += n.totalRxBytes;
      totalTxBytes += n.totalTxBytes;
    }

    const primary = networks.find((n) => n.isPrimary) ?? networks[0] ?? null;

    return {
      rxBytesPerSec,
      txBytesPerSec,
      totalRxBytes,
      totalTxBytes,
      primary,
      count: networks.length,
    };
  }, [networks]);

  return (
    <>
      <PageHeader title="Network" description="Interfaces and throughput." />
      {!networks ? (
        <ConnectionNotice />
      ) : networks.length === 0 ? (
        <p className="text-fg-muted">No active network interfaces.</p>
      ) : (
        <div className="space-y-4">
          {overview && (
            <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
              <Card>
                <CardHeader
                  title="Aggregate Download Rate"
                  icon={<ArrowDown size={14} aria-hidden />}
                />
                <div className="space-y-1.5 px-4 pb-4 pt-3">
                  <div className="num text-xl font-semibold">
                    ↓ {formatBytesPerSecond(overview.rxBytesPerSec)}
                  </div>
                  <div className="text-[12px] text-fg-muted">
                    Across {overview.count}{" "}
                    {overview.count === 1 ? "interface" : "interfaces"}
                  </div>
                </div>
              </Card>

              <Card>
                <CardHeader
                  title="Aggregate Upload Rate"
                  icon={<ArrowUp size={14} aria-hidden />}
                />
                <div className="space-y-1.5 px-4 pb-4 pt-3">
                  <div className="num text-xl font-semibold">
                    ↑ {formatBytesPerSecond(overview.txBytesPerSec)}
                  </div>
                  <div className="text-[12px] text-fg-muted">
                    Outbound throughput across active links
                  </div>
                </div>
              </Card>

              <Card>
                <CardHeader
                  title="Total Session Traffic"
                  icon={<Activity size={14} aria-hidden />}
                />
                <div className="space-y-1.5 px-4 pb-4 pt-3">
                  <div className="num text-xl font-semibold">
                    {formatBytes(overview.totalRxBytes + overview.totalTxBytes)}
                  </div>
                  <div className="num text-[12px] text-fg-muted">
                    ↓ {formatBytes(overview.totalRxBytes)} · ↑{" "}
                    {formatBytes(overview.totalTxBytes)}
                  </div>
                </div>
              </Card>

              <Card>
                <CardHeader
                  title="Primary Interface & IPv4"
                  icon={<Globe size={14} aria-hidden />}
                />
                <div className="space-y-1.5 px-4 pb-4 pt-3">
                  <div className="truncate text-xl font-semibold">
                    {overview.primary?.name ?? "—"}
                    {overview.primary?.ipv4[0] ? (
                      <span className="num ml-2 text-sm font-normal text-fg-muted">
                        {overview.primary.ipv4[0]}
                      </span>
                    ) : null}
                  </div>
                  <div className="num truncate text-[12px] text-fg-muted">
                    MAC {overview.primary?.macAddress ?? "—"} · {overview.count} active
                  </div>
                </div>
              </Card>
            </div>
          )}

          {networks.map((n) => (
            <InterfaceCard key={n.name} iface={n} />
          ))}
        </div>
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
              <ArrowDown size={16} className="text-[var(--chart-1)]" aria-label="Download" />
              {formatBytesPerSecond(n.rxBytesPerSec)}
            </span>
            <span className="num flex items-center gap-1 text-xl font-semibold">
              <ArrowUp size={16} className="text-[var(--chart-2)]" aria-label="Upload" />
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
