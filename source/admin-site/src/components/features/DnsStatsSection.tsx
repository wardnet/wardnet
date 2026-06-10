import { useMemo, useState } from "react";
import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ReferenceArea,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { ZoomableChartContainer } from "@/components/compound/ZoomableChartContainer";
import { type ChartConfig } from "@/components/core/ui/chart";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { DashboardStatCard } from "@/components/compound/DashboardStatCard";
import { useChartZoom, type ZoomRange } from "@/hooks/useChartZoom";
import {
  useDnsStatsDashboard,
  RANGE_HOURS,
  type StatsRange,
} from "@wardnet/web";
import type { StatsTopEntry } from "@wardnet/js";

const chartConfig: ChartConfig = {
  total: { label: "Total", color: "var(--chart-1)" },
  blocked: { label: "Blocked", color: "var(--chart-2)" },
};

function formatXAxis(tsMs: number, range: StatsRange): string {
  const d = new Date(tsMs);
  const hours = RANGE_HOURS[range];
  if (hours <= 24)
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (hours <= 168)
    return d.toLocaleString([], {
      month: "short",
      day: "numeric",
      hour: "2-digit",
    });
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

interface Props {
  range: StatsRange;
}

/** Stats panel for the DNS page: top cards, time series, top tables. */
export function DnsStatsSection({ range }: Props) {
  // Tag zoom with the range it was committed for so it auto-invalidates
  // when the parent changes range without needing a useEffect reset.
  const [storedZoom, setStoredZoom] = useState<{
    range: StatsRange;
    zoom: ZoomRange;
  } | null>(null);
  const zoom = storedZoom?.range === range ? storedZoom.zoom : null;
  const setZoom = (z: ZoomRange | null) =>
    setStoredZoom(z ? { range, zoom: z } : null);
  const topOverride = zoom
    ? {
        from: new Date(Number(zoom.start)).toISOString(),
        to: new Date(Number(zoom.end)).toISOString(),
      }
    : undefined;
  const { data, isLoading, isError, error } = useDnsStatsDashboard(
    range,
    topOverride,
  );

  // Numeric-timestamp chart data lets Recharts XAxis use type="number"
  // so the domain can be constrained to the zoom window, same pattern as
  // TunnelThroughputChart.
  const chartSeries = useMemo(
    () =>
      (data?.series ?? []).map((p) => ({
        ...p,
        tsMs: new Date(p.ts).getTime(),
      })),
    [data?.series],
  );

  const datasetKey = `${range}|${chartSeries.length}`;
  const { chartProps, previewRange, isZoomed, reset } = useChartZoom({
    datasetKey,
    zoom,
    onZoomChange: setZoom,
  });

  const windowTotals = useMemo(() => {
    if (chartSeries.length === 0) return { total: 0, blocked: 0 };
    const startMs = zoom ? Number(zoom.start) : -Infinity;
    const endMs = zoom ? Number(zoom.end) : Infinity;
    let total = 0;
    let blocked = 0;
    for (const p of chartSeries) {
      if (p.tsMs >= startMs && p.tsMs <= endMs) {
        total += p.total;
        blocked += p.blocked;
      }
    }
    return { total, blocked };
  }, [chartSeries, zoom]);

  // When zoomed, show the window duration instead of the range label and
  // replace query/blocked counts with the zoom-filtered totals. Top blocked
  // domain and active clients can't be filtered client-side so they keep the
  // full-range label and values.
  const zoomLabel = zoom ? formatZoomDuration(zoom) : null;
  const cardTotal = zoom ? windowTotals.total : data?.total;
  const cardBlocked = zoom ? windowTotals.blocked : data?.blocked;
  const cardBlockedPct = zoom
    ? windowTotals.total > 0
      ? (windowTotals.blocked / windowTotals.total) * 100
      : 0
    : data?.blockedPercent;

  const topBlockedDomain = data?.topDomains.entries[0];
  const topBlockedLabel = topBlockedDomain
    ? (parseLabels(topBlockedDomain.labels).domain ?? "—")
    : "—";

  if (isError) {
    return (
      <Card
        error={
          error instanceof Error ? error.message : "Failed to load DNS stats"
        }
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <DashboardStatCard
          title={`Queries (${zoomLabel ?? range})`}
          value={cardTotal != null ? cardTotal.toLocaleString() : "—"}
        />
        <DashboardStatCard
          title={`Blocked (${zoomLabel ?? range})`}
          value={cardBlockedPct != null ? `${cardBlockedPct.toFixed(1)}%` : "—"}
          subtitle={
            cardTotal != null && cardBlocked != null
              ? `${cardBlocked.toLocaleString()} of ${cardTotal.toLocaleString()}`
              : undefined
          }
          usagePercent={cardBlockedPct}
        />
        <DashboardStatCard
          title={`Top blocked (${zoomLabel ?? range})`}
          value={topBlockedLabel}
          subtitle={
            topBlockedDomain
              ? `${Math.round(topBlockedDomain.total).toLocaleString()} queries`
              : undefined
          }
        />
        <DashboardStatCard
          title={`Active clients (${zoomLabel ?? range})`}
          value={data ? data.topClients.entries.length.toString() : "—"}
        />
      </div>

      <Card>
        <CardHeader>
          <div>
            <CardTitle>Queries over time</CardTitle>
            <p className="mt-1 text-[10px] text-ink-3">
              Window total: {windowTotals.total.toLocaleString()} queries ·{" "}
              {windowTotals.blocked.toLocaleString()} blocked
            </p>
          </div>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="flex h-64 items-center justify-center text-sm text-ink-3">
              Loading…
            </div>
          ) : chartSeries.length === 0 ? (
            <div className="flex h-64 items-center justify-center text-sm text-ink-3">
              No data yet.
            </div>
          ) : (
            <ZoomableChartContainer
              config={chartConfig}
              className="h-64 w-full"
              isZoomed={isZoomed}
              onResetZoom={reset}
            >
              <LineChart
                data={chartSeries}
                margin={{ top: 8, right: 8, left: 8, bottom: 0 }}
                {...chartProps}
              >
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis
                  dataKey="tsMs"
                  type="number"
                  domain={
                    zoom ? [zoom.start, zoom.end] : ["dataMin", "dataMax"]
                  }
                  allowDataOverflow
                  tick={{ fontSize: 10 }}
                  minTickGap={48}
                  tickMargin={10}
                  tickFormatter={(tsMs: number) => formatXAxis(tsMs, range)}
                />
                <YAxis tick={{ fontSize: 10 }} width={42} />
                <Tooltip
                  labelFormatter={(tsMs) =>
                    typeof tsMs === "number"
                      ? new Date(tsMs).toLocaleString()
                      : String(tsMs)
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
                  isAnimationActive={false}
                />
                <Line
                  type="monotone"
                  dataKey="blocked"
                  stroke="var(--color-blocked)"
                  strokeWidth={2}
                  dot={false}
                  name="Blocked"
                  isAnimationActive={false}
                />
                {previewRange && (
                  <ReferenceArea
                    x1={previewRange.start}
                    x2={previewRange.end}
                    strokeOpacity={0}
                    fill="var(--ink-3)"
                    fillOpacity={0.12}
                  />
                )}
              </LineChart>
            </ZoomableChartContainer>
          )}
        </CardContent>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <TopList
          title={`Top blocked domains (${zoomLabel ?? range})`}
          entries={data?.topDomains.entries}
          labelKey="domain"
          valueLabel="blocks"
        />
        <TopList
          title={`Top clients (${zoomLabel ?? range})`}
          entries={data?.topClients.entries}
          labelKey="client"
          valueLabel="queries"
        />
      </div>
    </div>
  );
}

function formatZoomDuration(zoom: ZoomRange): string {
  const totalMins = Math.round(
    (Number(zoom.end) - Number(zoom.start)) / 60_000,
  );
  if (totalMins < 60) return `${totalMins}m`;
  const h = Math.floor(totalMins / 60);
  const m = totalMins % 60;
  if (h < 24) return m > 0 ? `${h}h ${m}m` : `${h}h`;
  const d = Math.floor(h / 24);
  const rem = h % 24;
  return rem > 0 ? `${d}d ${rem}h` : `${d}d`;
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
