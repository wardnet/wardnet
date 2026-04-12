import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Button } from "@/components/core/ui/button";
import type { DhcpConfig } from "@wardnet/js";

function formatDuration(secs: number): string {
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
  return `${Math.floor(secs / 86400)}d`;
}

interface DhcpConfigCardProps {
  config: DhcpConfig;
  onEdit: () => void;
}

/** Card displaying the DHCP pool configuration in a read-only grid. */
export function DhcpConfigCard({ config, onEdit }: DhcpConfigCardProps) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <CardTitle className="text-sm font-medium text-muted-foreground">Configuration</CardTitle>
        <Button variant="outline" size="sm" onClick={onEdit}>
          Edit
        </Button>
      </CardHeader>
      <CardContent>
        <dl className="grid grid-cols-2 gap-x-8 gap-y-3 text-sm sm:grid-cols-3">
          <div>
            <dt className="text-muted-foreground">Gateway IP</dt>
            <dd className="font-mono text-xs">{config.gateway_ip}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Pool Range</dt>
            <dd className="font-mono text-xs">
              {config.pool_start} &ndash; {config.pool_end}
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Subnet</dt>
            <dd className="font-mono text-xs">{config.subnet_mask}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Lease Duration</dt>
            <dd className="font-medium">{formatDuration(config.lease_duration_secs)}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Fallback Router</dt>
            <dd className="font-mono text-xs">{config.router_ip ?? "\u2014"}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Upstream DNS</dt>
            <dd className="font-mono text-xs">{config.upstream_dns.join(", ") || "\u2014"}</dd>
          </div>
        </dl>
      </CardContent>
    </Card>
  );
}
