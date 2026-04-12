import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { DashboardUsageBar } from "./DashboardUsageBar";

interface DashboardStatCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  /** If provided, renders a usage bar below the value. */
  usagePercent?: number;
}

/** Single stat card for the admin dashboard. */
export function DashboardStatCard({
  title,
  value,
  subtitle,
  usagePercent,
}: DashboardStatCardProps) {
  return (
    <Card>
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
}
