import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { PageHeader } from "@/components/compound/PageHeader";
import { useSystemStatus } from "@/hooks/useSystemStatus";
import { useDevices } from "@/hooks/useDevices";
import { useTunnels } from "@/hooks/useTunnels";
import { formatBytes, formatUptime } from "@/lib/utils";

/** Admin dashboard with system overview stats. */
export default function Dashboard() {
  const { data: status } = useSystemStatus();
  const { data: devicesData } = useDevices();
  const { data: tunnelsData } = useTunnels();

  const deviceCount = devicesData?.devices.length ?? status?.device_count ?? 0;
  const tunnelCount = tunnelsData?.tunnels.length ?? status?.tunnel_count ?? 0;
  const activeTunnels = tunnelsData?.tunnels.filter((t) => t.status === "up").length ?? 0;

  return (
    <>
      <PageHeader title="Dashboard" />
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">Devices</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-3xl font-bold">{deviceCount}</p>
            <p className="mt-1 text-xs text-muted-foreground">on the network</p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm font-medium text-muted-foreground">Tunnels</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-3xl font-bold">{tunnelCount}</p>
            <p className="mt-1 text-xs text-muted-foreground">{activeTunnels} active</p>
          </CardContent>
        </Card>

        {status && (
          <>
            <Card>
              <CardHeader>
                <CardTitle className="text-sm font-medium text-muted-foreground">Uptime</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="text-3xl font-bold">{formatUptime(status.uptime_seconds)}</p>
                <p className="mt-1 text-xs text-muted-foreground">v{status.version}</p>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="text-sm font-medium text-muted-foreground">CPU</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="text-3xl font-bold">{status.cpu_usage_percent.toFixed(1)}%</p>
                <CpuBar value={status.cpu_usage_percent} />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="text-sm font-medium text-muted-foreground">Memory</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="text-3xl font-bold">{formatBytes(status.memory_used_bytes)}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  of {formatBytes(status.memory_total_bytes)}
                </p>
                <CpuBar
                  value={
                    status.memory_total_bytes > 0
                      ? (status.memory_used_bytes / status.memory_total_bytes) * 100
                      : 0
                  }
                />
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="text-sm font-medium text-muted-foreground">
                  Database
                </CardTitle>
              </CardHeader>
              <CardContent>
                <p className="text-3xl font-bold">{formatBytes(status.db_size_bytes)}</p>
                <p className="mt-1 text-xs text-muted-foreground">SQLite</p>
              </CardContent>
            </Card>
          </>
        )}
      </div>
    </>
  );
}

/** Small usage bar for CPU/memory. */
function CpuBar({ value }: { value: number }) {
  const clamped = Math.min(100, Math.max(0, value));
  const color = clamped > 80 ? "bg-destructive" : clamped > 50 ? "bg-yellow-500" : "bg-primary";

  return (
    <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-muted">
      <div className={`h-full rounded-full ${color}`} style={{ width: `${clamped}%` }} />
    </div>
  );
}
