import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { Switch } from "@/components/core/ui/switch";
import { Label } from "@/components/core/ui/label";
import { DashboardUsageBar } from "./DashboardUsageBar";
import type { DhcpStatusResponse } from "@wardnet/js";

interface DhcpStatusCardProps {
  status: DhcpStatusResponse;
  onToggle: (enabled: boolean) => void;
  isPending: boolean;
}

/** Card showing DHCP server status with toggle, lease count, and pool usage. */
export function DhcpStatusCard({ status, onToggle, isPending }: DhcpStatusCardProps) {
  const poolUsagePercent = status.pool_total > 0 ? (status.pool_used / status.pool_total) * 100 : 0;

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <CardTitle className="text-sm font-medium text-muted-foreground">DHCP Server</CardTitle>
        <Badge variant={status.running ? "default" : "secondary"}>
          {status.running ? "Running" : "Stopped"}
        </Badge>
      </CardHeader>
      <CardContent>
        <div className="flex flex-col gap-4">
          <div className="flex items-center justify-between">
            <Label htmlFor="dhcp-toggle">Enable DHCP</Label>
            <Switch
              id="dhcp-toggle"
              checked={status.enabled}
              onCheckedChange={onToggle}
              disabled={isPending}
            />
          </div>
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <p className="text-muted-foreground">Active Leases</p>
              <p className="text-2xl font-bold">{status.active_lease_count}</p>
            </div>
            <div>
              <p className="text-muted-foreground">Pool Size</p>
              <p className="text-2xl font-bold">{status.pool_total}</p>
            </div>
          </div>
          <div>
            <p className="mb-1 text-xs text-muted-foreground">
              Pool Usage ({status.pool_used} / {status.pool_total})
            </p>
            <DashboardUsageBar value={poolUsagePercent} />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
