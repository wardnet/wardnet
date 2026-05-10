import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Toggle } from "@wardnet/forge-web/toggle";
import { StatusBadge } from "./StatusBadge";
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
        <CardTitle className="text-sm font-medium text-ink-3">DHCP server</CardTitle>
        <StatusBadge tone={status.running ? "success" : "neutral"} withIcon={status.running}>
          {status.running ? "Running" : "Stopped"}
        </StatusBadge>
      </CardHeader>
      <CardContent>
        <div className="flex flex-col gap-4">
          <Field direction="row" label="Enable DHCP" htmlFor="dhcp-toggle">
            <Toggle
              id="dhcp-toggle"
              checked={status.enabled}
              onCheckedChange={onToggle}
              disabled={isPending}
            />
          </Field>
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <p className="text-ink-3">Active leases</p>
              <p className="text-2xl font-bold">{status.active_lease_count}</p>
            </div>
            <div>
              <p className="text-ink-3">Pool size</p>
              <p className="text-2xl font-bold">{status.pool_total}</p>
            </div>
          </div>
          <div>
            <p className="mb-1 text-xs text-ink-3">
              Pool usage ({status.pool_used} / {status.pool_total})
            </p>
            <DashboardUsageBar value={poolUsagePercent} />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
