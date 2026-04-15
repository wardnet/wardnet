import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { Switch } from "@/components/core/ui/switch";
import { Label } from "@/components/core/ui/label";
import { Button } from "@/components/core/ui/button";
import { PageHeader } from "@/components/compound/PageHeader";
import { DashboardUsageBar } from "@/components/compound/DashboardUsageBar";
import { useDnsStatus, useDnsConfig, useToggleDns, useFlushDnsCache } from "@/hooks/useDns";

/** DNS server configuration page (admin only). */
export default function Dns() {
  const { data: statusData, isLoading: statusLoading } = useDnsStatus();
  const { data: configData } = useDnsConfig();

  const toggleDns = useToggleDns();
  const flushCache = useFlushDnsCache();

  const status = statusData;
  const config = configData?.config;

  const cacheUsagePercent =
    status && status.cache_capacity > 0 ? (status.cache_size / status.cache_capacity) * 100 : 0;

  return (
    <>
      <PageHeader title="DNS" />

      {statusLoading && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Loading DNS status...
          </CardContent>
        </Card>
      )}

      {status && config && (
        <div className="flex min-h-0 flex-1 flex-col gap-4">
          {/* Status & Cache cards */}
          <div className="grid gap-4 sm:grid-cols-2">
            {/* Status card */}
            <Card>
              <CardHeader className="flex flex-row items-center justify-between">
                <CardTitle className="text-sm font-medium text-muted-foreground">
                  DNS Server
                </CardTitle>
                <Badge variant={status.running ? "default" : "secondary"}>
                  {status.running ? "Running" : "Stopped"}
                </Badge>
              </CardHeader>
              <CardContent>
                <div className="flex flex-col gap-4">
                  <div className="flex items-center justify-between">
                    <Label htmlFor="dns-toggle">Enable DNS</Label>
                    <Switch
                      id="dns-toggle"
                      checked={status.enabled}
                      onCheckedChange={(enabled) => toggleDns.mutate(enabled)}
                      disabled={toggleDns.isPending}
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <p className="text-muted-foreground">Resolution Mode</p>
                      <p className="text-lg font-semibold capitalize">{config.resolution_mode}</p>
                    </div>
                    <div>
                      <p className="text-muted-foreground">DNSSEC</p>
                      <p className="text-lg font-semibold">
                        {config.dnssec_enabled ? "Enabled" : "Disabled"}
                      </p>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>

            {/* Cache card */}
            <Card>
              <CardHeader className="flex flex-row items-center justify-between">
                <CardTitle className="text-sm font-medium text-muted-foreground">Cache</CardTitle>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => flushCache.mutate()}
                  disabled={flushCache.isPending}
                >
                  {flushCache.isPending ? "Flushing..." : "Flush Cache"}
                </Button>
              </CardHeader>
              <CardContent>
                <div className="flex flex-col gap-4">
                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <p className="text-muted-foreground">Entries</p>
                      <p className="text-2xl font-bold">{status.cache_size}</p>
                    </div>
                    <div>
                      <p className="text-muted-foreground">Hit Rate</p>
                      <p className="text-2xl font-bold">{status.cache_hit_rate.toFixed(1)}%</p>
                    </div>
                  </div>
                  <div>
                    <p className="mb-1 text-xs text-muted-foreground">
                      Cache Usage ({status.cache_size} / {status.cache_capacity})
                    </p>
                    <DashboardUsageBar value={cacheUsagePercent} />
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Upstream servers card */}
          <Card>
            <CardHeader>
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Upstream Servers
              </CardTitle>
            </CardHeader>
            <CardContent>
              {config.upstream_servers.length === 0 ? (
                <p className="text-sm text-muted-foreground">No upstream servers configured.</p>
              ) : (
                <div className="divide-y">
                  {config.upstream_servers.map((server, i) => (
                    <div
                      key={i}
                      className="flex items-center justify-between py-3 first:pt-0 last:pb-0"
                    >
                      <div className="flex flex-col gap-0.5">
                        <p className="text-sm font-medium">{server.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {server.address}
                          {server.port ? `:${server.port}` : ""}
                        </p>
                      </div>
                      <Badge variant="outline">{server.protocol.toUpperCase()}</Badge>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      )}
    </>
  );
}
