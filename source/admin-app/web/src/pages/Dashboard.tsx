import { PageHeader } from "@/components/compound/PageHeader";
import { DashboardStatCard } from "@/components/compound/DashboardStatCard";
import { DhcpSummaryCard } from "@/components/compound/DhcpSummaryCard";
import { RecentErrorsCard } from "@/components/compound/RecentErrorsCard";
import { DashboardLogWidget } from "@/components/features/DashboardLogWidget";
import { useSystemStatus, useRecentErrors } from "@/hooks/useSystemStatus";
import { useDevices } from "@/hooks/useDevices";
import { useTunnels } from "@/hooks/useTunnels";
import { useDhcpStatus } from "@/hooks/useDhcp";
import { useDnsStats } from "@/hooks/useDnsLogs";
import { formatBytes, formatUptime } from "@/lib/utils";

/** Admin dashboard with system overview stats. */
export default function Dashboard() {
  const { data: status } = useSystemStatus();
  const { data: devicesData } = useDevices();
  const { data: tunnelsData } = useTunnels();
  const { data: dhcpStatus } = useDhcpStatus();
  const { data: errorsData } = useRecentErrors();
  const { data: dnsStats } = useDnsStats(24);

  const deviceCount = devicesData?.devices.length ?? status?.device_count ?? 0;
  const tunnelCount = tunnelsData?.tunnels.length ?? status?.tunnel_count ?? 0;
  const activeTunnels = tunnelsData?.tunnels.filter((t) => t.status === "up").length ?? 0;

  const memoryPercent =
    status && status.memory_total_bytes > 0
      ? (status.memory_used_bytes / status.memory_total_bytes) * 100
      : 0;

  return (
    <>
      <PageHeader title="Dashboard" description="Live overview" />

      <div className="col gap-20">
        {/* Stat cards */}
        <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-3">
          <DashboardStatCard
            title="Devices"
            value={deviceCount}
            subtitle="on the network"
            to="/devices"
          />
          <DashboardStatCard
            title="Tunnels"
            value={tunnelCount}
            subtitle={`${activeTunnels} active`}
            to="/tunnels"
          />
          {status && (
            <>
              <DashboardStatCard
                title="Uptime"
                value={formatUptime(status.uptime_seconds)}
                subtitle={`v${status.release_version}`}
              />
              <DashboardStatCard
                title="CPU"
                value={`${status.cpu_usage_percent.toFixed(1)}%`}
                usagePercent={status.cpu_usage_percent}
              />
              <DashboardStatCard
                title="Memory"
                value={formatBytes(status.memory_used_bytes)}
                subtitle={`of ${formatBytes(status.memory_total_bytes)}`}
                usagePercent={memoryPercent}
              />
              <DhcpSummaryCard status={dhcpStatus} to="/dhcp" />
            </>
          )}
          <DashboardStatCard
            title="DNS queries (24h)"
            value={dnsStats?.totals.total_queries.toLocaleString() ?? "—"}
            subtitle={
              dnsStats
                ? `${dnsStats.totals.unique_clients} clients · ${dnsStats.totals.unique_domains} domains`
                : undefined
            }
            to="/dns/logs"
          />
          <DashboardStatCard
            title="Blocked traffic (24hrs)"
            value={dnsStats ? `${dnsStats.totals.blocked_percent.toFixed(1)}%` : "—"}
            subtitle={
              dnsStats
                ? `${dnsStats.totals.blocked_queries.toLocaleString()} of ${dnsStats.totals.total_queries.toLocaleString()}`
                : undefined
            }
            usagePercent={dnsStats?.totals.blocked_percent}
            to="/dns/filter"
          />
        </div>

        {/* Recent errors */}
        <RecentErrorsCard errors={errorsData?.errors ?? []} />

        {/* Live log stream */}
        <DashboardLogWidget />
      </div>
    </>
  );
}
