import { Link } from "react-router";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { StatusBadge } from "./StatusBadge";
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
          <StatusBadge tone={status.running ? "success" : "neutral"} withIcon={status.running}>
            {status.running ? "Running" : "Stopped"}
          </StatusBadge>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-3xl font-bold">{status.active_lease_count}</p>
        <p className="mt-1 text-xs text-muted-foreground">
          active leases &middot; {poolPercent}% pool used
        </p>
        {status.pool_total > 0 && (
          <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-sunken">
            <div
              className={`h-full rounded-full ${poolPercent > 80 ? "bg-danger" : poolPercent > 50 ? "bg-yellow-500" : "bg-primary"}`}
              style={{ width: `${Math.min(100, poolPercent)}%` }}
            />
          </div>
        )}
      </CardContent>
    </Card>
  );

  if (to) {
    return (
      <Link
        to={to}
        className="block focus:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-lg"
      >
        {card}
      </Link>
    );
  }
  return card;
}
