import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { PageHeader } from "@/components/compound/PageHeader";
import { useSystemStatus } from "@/hooks/useSystemStatus";
import { formatBytes, formatUptime } from "@/lib/utils";

/** Settings page for system configuration (admin only). */
export default function Settings() {
  const { data: status, isLoading } = useSystemStatus();

  return (
    <>
      <PageHeader title="Settings" />
      <div className="flex flex-col gap-4">
        <Card>
          <CardHeader>
            <CardTitle>System Information</CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <p className="text-sm text-muted-foreground">Loading...</p>
            ) : status ? (
              <dl className="grid grid-cols-2 gap-x-8 gap-y-3 text-sm sm:grid-cols-3">
                <div>
                  <dt className="text-muted-foreground">Version</dt>
                  <dd className="font-medium">{status.version}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Uptime</dt>
                  <dd className="font-medium">{formatUptime(status.uptime_seconds)}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Devices</dt>
                  <dd className="font-medium">{status.device_count}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Tunnels</dt>
                  <dd className="font-medium">{status.tunnel_count}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Database Size</dt>
                  <dd className="font-medium">{formatBytes(status.db_size_bytes)}</dd>
                </div>
              </dl>
            ) : (
              <p className="text-sm text-muted-foreground">Unable to connect to daemon.</p>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Account</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              Account management will be available in a future release.
            </p>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
