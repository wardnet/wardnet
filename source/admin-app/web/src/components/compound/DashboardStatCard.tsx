import { Link } from "react-router";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { DashboardUsageBar } from "./DashboardUsageBar";

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
  const card = (
    <Card className={to ? "transition-colors hover:bg-accent/50" : undefined}>
      <CardHeader>
        <CardTitle className="text-sm font-semibold">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-3xl font-bold">{value}</p>
        {subtitle && <p className="mt-1 text-xs text-muted-foreground">{subtitle}</p>}
        {usagePercent != null && <DashboardUsageBar value={usagePercent} />}
      </CardContent>
    </Card>
  );

  if (to) {
    return (
      <Link
        to={to}
        className="block focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg"
      >
        {card}
      </Link>
    );
  }
  return card;
}
