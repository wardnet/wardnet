import { PageHeader } from "@/components/compound/PageHeader";
import { DashboardStatCard } from "@/components/compound/DashboardStatCard";
import { DhcpSummaryCard } from "@/components/compound/DhcpSummaryCard";
import { RecentErrorsCard } from "@/components/compound/RecentErrorsCard";
import { DashboardLogWidget } from "@/components/features/DashboardLogWidget";
import { useSystemStatus, useRecentErrors } from "@wardnet/wardnet-web";
import { useDevices } from "@wardnet/wardnet-web";
import { useTunnels } from "@wardnet/wardnet-web";
import { useDhcpStatus } from "@wardnet/wardnet-web";
import { useDnsStatSummary } from "@wardnet/wardnet-web";
import { formatBytes, formatUptime } from "@wardnet/wardnet-web";

/** Admin dashboard with system overview stats. */
export default function Dashboard() {
  const { data: status } = useSystemStatus();
  const { data: devicesData } = useDevices();
  const { data: tunnelsData } = useTunnels();
  const { data: dhcpStatus } = useDhcpStatus();
  const { data: errorsData } = useRecentErrors();
  const {
    data: dnsStats,
    isError: dnsStatsError,
    error: dnsStatsErrorObj,
  } = useDnsStatSummary("24h");
  const dnsStatsErrorMsg = dnsStatsError
    ? dnsStatsErrorObj instanceof Error
      ? dnsStatsErrorObj.message
      : "Failed to load DNS stats"
    : null;

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
              <DashboardStatCard
                title="Disk"
                value={formatBytes(status.disk_free_bytes)}
                subtitle={`free of ${formatBytes(status.disk_total_bytes)}`}
                usagePercent={
                  status.disk_total_bytes > 0
                    ? ((status.disk_total_bytes - status.disk_free_bytes) /
                        status.disk_total_bytes) *
                      100
                    : 0
                }
              />
              <DhcpSummaryCard status={dhcpStatus} to="/dhcp" />
            </>
          )}
          <DashboardStatCard
            title="DNS queries (24h)"
            value={dnsStats?.total.toLocaleString() ?? "—"}
            to="/dns/logs"
            error={dnsStatsErrorMsg}
          />
          <DashboardStatCard
            title="Blocked traffic (24h)"
            value={dnsStats ? `${dnsStats.blockedPercent.toFixed(1)}%` : "—"}
            subtitle={
              dnsStats
                ? `${dnsStats.blocked.toLocaleString()} of ${dnsStats.total.toLocaleString()}`
                : undefined
            }
            usagePercent={dnsStats?.blockedPercent}
            to="/dns/filter"
            error={dnsStatsErrorMsg}
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
