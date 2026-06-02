import { SearchIcon, ShieldOffIcon } from "lucide-react";
import { Card, CardHeader, CardContent } from "@wardnet/forge-web/card";
import { StatTile } from "@wardnet/forge-web/stat-tile";
import { Sparkline } from "@wardnet/forge-web/sparkline";
import type { DashboardDnsStats } from "@wardnet/wardnet-web";

interface Props {
  data: DashboardDnsStats | undefined;
  isLoading: boolean;
}

export function DnsQueriesCard({ data, isLoading }: Props) {
  const spark =
    data && data.totalSeries.length > 0 ? (
      <Sparkline
        values={data.totalSeries}
        color="var(--accent)"
        className="h-9 w-full"
      />
    ) : undefined;

  return (
    <Card>
      <CardHeader className="flex items-center gap-2 px-4 pt-4 pb-0 text-sm font-medium text-ink-2">
        <SearchIcon size={14} strokeWidth={1.8} />
        DNS Queries · 24h
      </CardHeader>
      <CardContent className="px-4 pb-4">
        <StatTile
          label="Total"
          value={isLoading && !data ? "…" : (data?.total.toLocaleString() ?? "0")}
          spark={spark}
        />
      </CardContent>
    </Card>
  );
}

export function BlockedCard({ data, isLoading }: Props) {
  const spark =
    data && data.blockedSeries.length > 0 ? (
      <Sparkline
        values={data.blockedSeries}
        color="var(--danger)"
        className="h-9 w-full"
      />
    ) : undefined;

  const blockedLabel =
    data != null
      ? `${data.blocked.toLocaleString()} blocked`
      : undefined;

  return (
    <Card>
      <CardHeader className="flex items-center gap-2 px-4 pt-4 pb-0 text-sm font-medium text-ink-2">
        <ShieldOffIcon size={14} strokeWidth={1.8} />
        Blocked · 24h
      </CardHeader>
      <CardContent className="px-4 pb-4">
        <StatTile
          label="Block rate"
          value={isLoading && !data ? "…" : `${(data?.blockedPercent ?? 0).toFixed(1)}`}
          unit="%"
          sub={blockedLabel}
          spark={spark}
        />
      </CardContent>
    </Card>
  );
}
