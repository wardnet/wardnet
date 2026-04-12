import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import type { DhcpStatusResponse } from "@wardnet/js";

interface DhcpSummaryCardProps {
  status: DhcpStatusResponse | undefined;
}

/** Compact DHCP summary card for the dashboard. */
export function DhcpSummaryCard({ status }: DhcpSummaryCardProps) {
  if (!status) return null;

  const poolPercent =
    status.pool_total > 0 ? Math.round((status.pool_used / status.pool_total) * 100) : 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between text-sm font-semibold">
          DHCP
          <Badge variant={status.running ? "default" : "secondary"}>
            {status.running ? "Running" : "Stopped"}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-3xl font-bold">{status.active_lease_count}</p>
        <p className="mt-1 text-xs text-muted-foreground">
          active leases &middot; {poolPercent}% pool used
        </p>
        {status.pool_total > 0 && (
          <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-muted">
            <div
              className={`h-full rounded-full ${poolPercent > 80 ? "bg-destructive" : poolPercent > 50 ? "bg-yellow-500" : "bg-primary"}`}
              style={{ width: `${Math.min(100, poolPercent)}%` }}
            />
          </div>
        )}
      </CardContent>
    </Card>
  );
}
