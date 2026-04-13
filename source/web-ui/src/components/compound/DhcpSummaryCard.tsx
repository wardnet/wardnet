import { Link } from "react-router";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import type { DhcpStatusResponse } from "@wardnet/js";

interface DhcpSummaryCardProps {
  status: DhcpStatusResponse | undefined;
  /** If provided, wraps the card in a router Link to this path. */
  to?: string;
}

/** Compact DHCP summary card for the dashboard. */
export function DhcpSummaryCard({ status, to }: DhcpSummaryCardProps) {
  if (!status) return null;

  const poolPercent =
    status.pool_total > 0 ? Math.round((status.pool_used / status.pool_total) * 100) : 0;

  const card = (
    <Card className={to ? "transition-colors hover:bg-accent/50" : undefined}>
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

  if (to) {
    return (
      <Link to={to} className="block focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg">
        {card}
      </Link>
    );
  }
  return card;
}
