import { CircleAlertIcon } from "lucide-react";
import { Link } from "react-router";
import { StatTile, Text } from "@wardnet/web";

interface DashboardStatCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  /** If provided, renders a usage bar below the value. */
  usagePercent?: number;
  /** If provided, wraps the card in a router Link to this path. */
  to?: string;
  /** If provided, renders the card in error mode instead of the stat tile. */
  error?: string | null;
}

/** Single stat card for the admin dashboard. */
export function DashboardStatCard({
  title,
  value,
  subtitle,
  usagePercent,
  to,
  error,
}: DashboardStatCardProps) {
  if (error) {
    return (
      <div
        className="card flex items-start gap-2 p-3.5 text-danger-soft-ink"
        style={{ background: "var(--danger-soft)" }}
      >
        <CircleAlertIcon className="size-4 shrink-0 mt-0.5" />
        <Text as="p" size="sm" weight="medium" className="leading-snug m-0">
          {error}
        </Text>
      </div>
    );
  }

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
