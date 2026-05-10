import { Link } from "react-router";
import { StatTile } from "@wardnet/forge-web/stat-tile";

interface DashboardStatCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  /** If provided, renders a usage bar below the value. */
  usagePercent?: number;
  /** If provided, wraps the card in a router Link to this path. */
  to?: string;
}

/** Single stat card for the admin dashboard. */
export function DashboardStatCard({
  title,
  value,
  subtitle,
  usagePercent,
  to,
}: DashboardStatCardProps) {
  const tile = (
    <StatTile
      label={title}
      value={value}
      sub={subtitle}
      bar={usagePercent}
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
