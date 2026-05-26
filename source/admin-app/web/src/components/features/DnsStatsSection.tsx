import { useMemo, useState } from "react";
import { Brush, CartesianGrid, Legend, Line, LineChart, Tooltip, XAxis, YAxis } from "recharts";

import { ChartContainer, type ChartConfig } from "@/components/core/ui/chart";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Tabs, TabsList, TabsTrigger } from "@wardnet/forge-web/tabs";
import { DashboardStatCard } from "@/components/compound/DashboardStatCard";
import { useDnsStatsDashboard } from "@/hooks/useStats";
import type { StatsTopEntry } from "@wardnet/js";

const RANGES: { value: number; label: string }[] = [
  { value: 1, label: "1h" },
  { value: 24, label: "24h" },
  { value: 168, label: "7d" },
  { value: 8760, label: "12m" },
];

const chartConfig: ChartConfig = {
  total: { label: "Total", color: "var(--chart-1)" },
  blocked: { label: "Blocked", color: "var(--chart-2)" },
};

function formatXAxis(ts: string, hours: number): string {
  const d = new Date(ts);
  if (hours <= 24) return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (hours <= 168)
    return d.toLocaleString([], { month: "short", day: "numeric", hour: "2-digit" });
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

/** Stats panel for the DNS page: top cards, time series, top tables. */
export function DnsStatsSection() {
  const [hours, setHours] = useState<number>(24);
  const { data, isLoading, isError, error } = useDnsStatsDashboard(hours);

  const series = data?.series ?? [];

  // Brush state — tagged with a dataset key so the selection auto-resets
  // when the range changes. Same pattern as TunnelThroughputChart.
  const datasetKey = `${hours}|${series.length}`;
  const [stored, setStored] = useState<{ key: string; start: number; end: number } | null>(null);
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

  const topBlockedDomain = data?.topDomains.entries[0];
  const topBlockedLabel = topBlockedDomain
    ? (parseLabels(topBlockedDomain.labels).domain ?? "—")
    : "—";
  const rangeLabel = RANGES.find((r) => r.value === hours)?.label ?? `${hours}h`;

  if (isError) {
    return (
      <Card error={error instanceof Error ? error.message : "Failed to load DNS stats"} />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <DashboardStatCard
          title={`Queries (${rangeLabel})`}
          value={data ? data.total.toLocaleString() : "—"}
        />
        <DashboardStatCard
          title={`Blocked (${rangeLabel})`}
          value={data ? `${data.blockedPercent.toFixed(1)}%` : "—"}
          subtitle={
            data
              ? `${data.blocked.toLocaleString()} of ${data.total.toLocaleString()}`
              : undefined
          }
          usagePercent={data?.blockedPercent}
        />
        <DashboardStatCard
          title={`Top blocked (${rangeLabel})`}
          value={topBlockedLabel}
          subtitle={
            topBlockedDomain
              ? `${Math.round(topBlockedDomain.total).toLocaleString()} queries`
              : undefined
          }
        />
        <DashboardStatCard
          title={`Active clients (${rangeLabel})`}
          value={data ? data.topClients.entries.length.toString() : "—"}
        />
      </div>

      <Card>
        <CardHeader className="flex flex-col gap-3 @md/card-header:flex-row @md/card-header:items-center @md/card-header:justify-between">
          <div>
            <CardTitle>Queries over time</CardTitle>
            <p className="mt-1 text-[10px] text-ink-3">
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
          {isLoading ? (
            <div className="flex h-64 items-center justify-center text-sm text-ink-3">
              Loading…
            </div>
          ) : series.length === 0 ? (
            <div className="flex h-64 items-center justify-center text-sm text-ink-3">
              No data yet.
            </div>
          ) : (
          <ChartContainer config={chartConfig} className="h-64 w-full">
            <LineChart data={series} margin={{ top: 8, right: 8, left: 8, bottom: 0 }}>
              <CartesianGrid strokeDasharray="3 3" />
              <XAxis
                dataKey="ts"
                tick={{ fontSize: 10 }}
                minTickGap={48}
                tickMargin={10}
                tickFormatter={(ts: string) => formatXAxis(ts, hours)}
              />
              <YAxis tick={{ fontSize: 10 }} width={42} />
              <Tooltip
                labelFormatter={(label) =>
                  typeof label === "string" ? formatXAxis(label, hours) : String(label)
                }
              />
              <Legend />
              <Line
                type="monotone"
                dataKey="total"
                stroke="var(--color-total)"
                strokeWidth={2}
                dot={false}
                name="Total"
              />
              <Line
                type="monotone"
                dataKey="blocked"
                stroke="var(--color-blocked)"
                strokeWidth={2}
                dot={false}
                name="Blocked"
              />
              <Brush
                dataKey="ts"
                height={24}
                travellerWidth={8}
                stroke="var(--color-total)"
                tickFormatter={() => ""}
                onChange={(r) => {
                  if (typeof r?.startIndex === "number" && typeof r?.endIndex === "number") {
                    setStored({ key: datasetKey, start: r.startIndex, end: r.endIndex });
                  }
                }}
              />
            </LineChart>
          </ChartContainer>
          )}
        </CardContent>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <TopList
          title="Top blocked domains"
          entries={data?.topDomains.entries}
          labelKey="domain"
          valueLabel="blocks"
        />
        <TopList
          title="Top clients"
          entries={data?.topClients.entries}
          labelKey="client"
          valueLabel="queries"
        />
      </div>
    </div>
  );
}

function parseLabels(json: string): Record<string, string> {
  try {
    return JSON.parse(json) as Record<string, string>;
  } catch {
    return {};
  }
}

function TopList({
  title,
  entries,
  labelKey,
  valueLabel,
}: {
  title: string;
  entries: StatsTopEntry[] | undefined;
  labelKey: string;
  valueLabel: string;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        {entries && entries.length > 0 ? (
          <ul className="flex flex-col gap-2 text-sm">
            {entries.map((entry, i) => {
              const label = parseLabels(entry.labels)[labelKey] ?? entry.labels;
              return (
                <li key={i} className="flex items-center justify-between gap-2">
                  <span className="truncate font-mono text-xs">{label}</span>
                  <span className="tabular-nums text-ink-3">
                    {Math.round(entry.total).toLocaleString()} {valueLabel}
                  </span>
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="text-sm text-ink-3">No data yet.</p>
        )}
      </CardContent>
    </Card>
  );
}
