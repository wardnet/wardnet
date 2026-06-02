import {
  useAuth,
  useSystemStatus,
  useDevices,
  useTunnels,
  useDefaultPolicy,
  useDaemonStatus,
  useDashboardDnsStats,
} from "@wardnet/wardnet-web";
import { useBiometric } from "@/hooks/useBiometric";
import { useNavigate } from "react-router";
import { DevicesCard } from "@/features/dashboard/DevicesCard";
import { TunnelsCard } from "@/features/dashboard/TunnelsCard";
import { DnsQueriesCard, BlockedCard } from "@/features/dashboard/DnsCard";
import { DaemonStrip } from "@/features/dashboard/DaemonStrip";

export default function Dashboard() {
  const { logout } = useAuth();
  const biometric = useBiometric();
  const navigate = useNavigate();

  const { data: status } = useSystemStatus();
  const { data: devicesData } = useDevices();
  const { data: tunnelsData } = useTunnels();
  const { data: policyData } = useDefaultPolicy();
  const { data: daemonStatus } = useDaemonStatus();
  const { data: dnsStats, isLoading: dnsLoading } = useDashboardDnsStats();

  function handleLogout() {
    logout();
    biometric.unregister();
    navigate("/login", { replace: true });
  }

  return (
    <div className="flex flex-col gap-3 p-4">
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

      <DaemonStrip
        reachable={daemonStatus?.reachable ?? false}
        version={daemonStatus?.version ?? null}
        uptimeSeconds={daemonStatus?.uptimeSeconds ?? null}
      />

      <button
        onClick={handleLogout}
        className="mt-2 self-start rounded-md border border-danger px-4 py-2 text-sm font-medium text-danger"
      >
        Log out
      </button>
    </div>
  );
}
