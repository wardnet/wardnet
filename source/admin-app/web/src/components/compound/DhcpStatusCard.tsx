import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Pill } from "@wardnet/forge-web/pill";
import { Toggle } from "@wardnet/forge-web/toggle";
import type { DhcpStatusResponse } from "@wardnet/js";

interface DhcpStatusCardProps {
  status: DhcpStatusResponse;
  onToggle: (enabled: boolean) => void;
  isPending: boolean;
}

/** Card showing DHCP server status with toggle, lease count, and pool usage.
 *  Follows the studio "first-card" pattern: status pill in the header, two
 *  headline numbers, and a usage bar. */
export function DhcpStatusCard({ status, onToggle, isPending }: DhcpStatusCardProps) {
  const poolUsagePercent = status.pool_total > 0 ? (status.pool_used / status.pool_total) * 100 : 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle>DHCP server</CardTitle>
        <Pill variant={status.running ? "ok" : "ghost"}>
          <span className="dot" />
          {status.running ? "Running" : "Stopped"}
        </Pill>
        <CardAction>
          <Toggle
            id="dhcp-toggle"
            aria-label="Enable DHCP"
            checked={status.enabled}
            onCheckedChange={onToggle}
            disabled={isPending}
          />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 p-[var(--pad)]">
        <div className="grid grid-cols-2 gap-4">
          <div>
            <div className="stat__label">Active leases</div>
            <div className="stat__value">{status.active_lease_count}</div>
          </div>
          <div>
            <div className="stat__label">Pool size</div>
            <div className="stat__value">{status.pool_total}</div>
          </div>
        </div>
        <div>
          <div className="flex items-center justify-between">
            <div className="stat__label">Pool usage</div>
            <div className="font-mono text-[11px] text-ink-3">
              {status.pool_used} / {status.pool_total}
            </div>
          </div>
          <div className="bar mt-1.5">
            <span style={{ width: `${Math.min(100, Math.max(0, poolUsagePercent))}%` }} />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
