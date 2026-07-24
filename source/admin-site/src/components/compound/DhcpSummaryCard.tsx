import { Link } from "react-router";
import { StatTile } from "@wardnet/web";
import { poolUsagePercent } from "@wardnet/web";
import { StatusBadge } from "./StatusBadge";
import type { DhcpStatusResponse } from "@wardnet/js";

interface DhcpSummaryCardProps {
  status: DhcpStatusResponse | undefined;
  /** If provided, wraps the card in a router Link to this path. */
  to?: string;
  /** e2e locator, forwarded to the rendered tile as `data-testid`. */
  testId?: string;
}

/** Compact DHCP summary card for the dashboard. */
export function DhcpSummaryCard({ status, to, testId }: DhcpSummaryCardProps) {
  if (!status) return null;

  const poolPercent = poolUsagePercent(status.pool_used, status.pool_total);

  const tile = (
    <StatTile
      data-testid={testId}
      label="DHCP"
      value={status.active_lease_count}
      sub={`active leases · ${poolPercent}% pool used`}
      bar={status.pool_total > 0 ? poolPercent : undefined}
      pill={
        <StatusBadge
          tone={status.running ? "success" : "neutral"}
          withIcon={status.running}
        >
          {status.running ? "Running" : "Stopped"}
        </StatusBadge>
      }
      className={to ? "transition-colors hover:bg-accent/50" : undefined}
    />
  );

  if (to) {
    return (
      <Link
        to={to}
        className="block focus:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-lg"
      >
        {tile}
      </Link>
    );
  }
  return tile;
}
