import { useMemo, useState } from "react";
import { Brush, CartesianGrid, Legend, Line, LineChart, Tooltip, XAxis, YAxis } from "recharts";

import { ChartContainer, type ChartConfig } from "@/components/core/ui/chart";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Tabs, TabsList, TabsTrigger } from "@/components/core/ui/tabs";
import { DashboardStatCard } from "@/components/compound/DashboardStatCard";
import { useDnsStats } from "@/hooks/useDnsLogs";

const RANGES: { value: number; label: string }[] = [
  { value: 1, label: "1h" },
  { value: 24, label: "24h" },
  { value: 168, label: "7d" },
];

const chartConfig: ChartConfig = {
  total: { label: "Total", color: "var(--chart-1, #3b82f6)" },
  blocked: { label: "Blocked", color: "var(--chart-2, #ef4444)" },
};

interface ChartPoint {
  bucket: string;
  total: number;
  blocked: number;
}

/** Stats panel for the Ad blocking page: top cards, time series, top tables. */
export function DnsStatsSection() {
  const [hours, setHours] = useState<number>(24);
  const { data: stats } = useDnsStats(hours);

  const series: ChartPoint[] = useMemo(
    () =>
      stats?.series.map((p) => ({
        bucket: p.bucket,
        total: p.total,
        blocked: p.blocked,
      })) ?? [],
    [stats],
  );

  // Brush state — tagged with a dataset key so the selection auto-resets
  // when the range changes (a 1h-window selection means nothing on a 7d
  // dataset). Same pattern as TunnelThroughputChart.
  const datasetKey = `${hours}|${series.length}`;
  const [stored, setStored] = useState<{
    key: string;
    start: number;
    end: number;
  } | null>(null);
  const brush = stored?.key === datasetKey ? stored : null;

  const windowTotals = useMemo(() => {
    if (series.length === 0) return { total: 0, blocked: 0 };
    const start = brush ? Math.max(0, brush.start) : 0;
    const end = brush ? Math.min(series.length - 1, brush.end) : series.length - 1;
    let total = 0;
    let blocked = 0;
    for (let i = start; i <= end; i++) {
      total += series[i].total;
      blocked += series[i].blocked;
    }
    return { total, blocked };
  }, [series, brush]);

  const topBlockedDomain = stats?.top_blocked[0];
  const rangeLabel = RANGES.find((r) => r.value === hours)?.label ?? `${hours}h`;

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <DashboardStatCard
          title="Queries"
          value={stats?.totals.total_queries.toLocaleString() ?? "—"}
          subtitle={`last ${rangeLabel}`}
        />
        <DashboardStatCard
          title="Blocked"
          value={stats ? `${stats.totals.blocked_percent.toFixed(1)}%` : "—"}
          subtitle={
            stats
              ? `${stats.totals.blocked_queries.toLocaleString()} of ${stats.totals.total_queries.toLocaleString()}`
              : undefined
          }
          usagePercent={stats?.totals.blocked_percent}
        />
        <DashboardStatCard
          title="Top blocked"
          value={topBlockedDomain?.domain ?? "—"}
          subtitle={
            topBlockedDomain ? `${topBlockedDomain.count.toLocaleString()} queries` : undefined
          }
        />
        <DashboardStatCard
          title="Active clients"
          value={stats?.totals.unique_clients.toString() ?? "—"}
          subtitle={
            stats ? `${stats.totals.unique_domains.toLocaleString()} unique domains` : undefined
          }
        />
      </div>

      <Card>
        <CardHeader className="flex flex-col gap-3 @md/card-header:flex-row @md/card-header:items-center @md/card-header:justify-between">
          <div>
            <CardTitle>Queries over time</CardTitle>
            <p className="mt-1 text-[10px] text-muted-foreground">
              Window total: {windowTotals.total.toLocaleString()} queries ·{" "}
              {windowTotals.blocked.toLocaleString()} blocked
            </p>
          </div>
          <Tabs value={String(hours)} onValueChange={(v) => setHours(Number(v))}>
            <TabsList>
              {RANGES.map((r) => (
                <TabsTrigger key={r.value} value={String(r.value)}>
                  {r.label}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
        </CardHeader>
        <CardContent>
          <ChartContainer config={chartConfig} className="h-64 w-full">
            <LineChart data={series} margin={{ top: 8, right: 8, left: 8, bottom: 0 }}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis dataKey="bucket" tick={{ fontSize: 10 }} minTickGap={48} tickMargin={10} />
              <YAxis tick={{ fontSize: 10 }} width={42} />
              <Tooltip />
              <Legend />
              <Line
                type="monotone"
                dataKey="total"
                stroke={chartConfig.total.color}
                strokeWidth={2}
                dot={false}
                name="Total"
              />
              <Line
                type="monotone"
                dataKey="blocked"
                stroke={chartConfig.blocked.color}
                strokeWidth={2}
                dot={false}
                name="Blocked"
              />
              <Brush
                dataKey="bucket"
                height={24}
                travellerWidth={8}
                stroke={chartConfig.total.color}
                tickFormatter={() => ""}
                onChange={(r) => {
                  if (typeof r?.startIndex === "number" && typeof r?.endIndex === "number") {
                    setStored({
                      key: datasetKey,
                      start: r.startIndex,
                      end: r.endIndex,
                    });
                  }
                }}
              />
            </LineChart>
          </ChartContainer>
        </CardContent>
      </Card>

      <div className="grid gap-4 lg:grid-cols-3">
        <TopList title="Top domains" items={stats?.top_domains} valueLabel="queries" />
        <TopList title="Top blocked" items={stats?.top_blocked} valueLabel="blocks" />
        <TopClientsList stats={stats} />
      </div>
    </div>
  );
}

function TopList({
  title,
  items,
  valueLabel,
}: {
  title: string;
  items: { domain: string; count: number }[] | undefined;
  valueLabel: string;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        {items && items.length > 0 ? (
          <ul className="flex flex-col gap-2 text-sm">
            {items.map((item) => (
              <li key={item.domain} className="flex items-center justify-between gap-2">
                <span className="truncate font-mono text-xs">{item.domain}</span>
                <span className="tabular-nums text-muted-foreground">
                  {item.count.toLocaleString()} {valueLabel}
                </span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-muted-foreground">No data yet.</p>
        )}
      </CardContent>
    </Card>
  );
}

function TopClientsList({
  stats,
}: {
  stats:
    | { top_clients: { client_ip: string; count: number; device_label?: string | null }[] }
    | undefined;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Top clients</CardTitle>
      </CardHeader>
      <CardContent>
        {stats && stats.top_clients.length > 0 ? (
          <ul className="flex flex-col gap-2 text-sm">
            {stats.top_clients.map((c) => {
              const primary = c.device_label || c.client_ip;
              const secondary = c.device_label ? c.client_ip : null;
              return (
                <li key={c.client_ip} className="flex items-center justify-between gap-2">
                  <div className="flex flex-col">
                    <span className="font-medium">{primary}</span>
                    {secondary && (
                      <span className="text-xs text-muted-foreground">{secondary}</span>
                    )}
                  </div>
                  <span className="tabular-nums text-muted-foreground">
                    {c.count.toLocaleString()} queries
                  </span>
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="text-sm text-muted-foreground">No data yet.</p>
        )}
      </CardContent>
    </Card>
  );
}
