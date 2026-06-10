import {
  useSystemStatus,
  useDevices,
  useTunnels,
  useDefaultPolicy,
  useDaemonStatus,
  useDashboardDnsStats,
} from "@wardnet/web";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import { StatusCard } from "@/features/dashboard/StatusCard";
import { DevicesCard } from "@/features/dashboard/DevicesCard";
import { TunnelsCard } from "@/features/dashboard/TunnelsCard";
import { DnsQueriesCard, BlockedCard } from "@/features/dashboard/DnsCard";

export default function Dashboard() {
  const { data: status } = useSystemStatus();
  const { data: devicesData } = useDevices();
  const { data: tunnelsData } = useTunnels();
  const { data: policyData } = useDefaultPolicy();
  const { data: daemonStatus } = useDaemonStatus();
  const { data: dnsStats, isLoading: dnsLoading } = useDashboardDnsStats();
  const { showingLastKnownState } = useOnlineStatusContext();

  return (
    <div className="flex flex-col gap-3 p-4">
      <StatusCard
        reachable={daemonStatus?.reachable ?? false}
        uptimeSeconds={daemonStatus?.uptimeSeconds ?? null}
      />

      <div className={showingLastKnownState ? "opacity-40 pointer-events-none transition-opacity" : "transition-opacity"}>
        <div className="flex flex-col gap-3">
          <DevicesCard
            deviceCount={status?.device_count ?? 0}
            devices={devicesData?.devices}
            defaultPolicy={policyData?.policy}
          />

          <TunnelsCard
            tunnelCount={status?.tunnel_count ?? 0}
            tunnelActiveCount={status?.tunnel_active_count ?? 0}
            tunnels={tunnelsData?.tunnels}
          />

          <DnsQueriesCard data={dnsStats} isLoading={dnsLoading} />

          <BlockedCard data={dnsStats} isLoading={dnsLoading} />
        </div>
      </div>
    </div>
  );
}
