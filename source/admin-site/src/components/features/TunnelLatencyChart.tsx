import { useMemo } from "react";
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
import { useChartZoom, type ZoomRange } from "@/hooks/useChartZoom";
import type { TunnelStatsData, StatsRange } from "@wardnet/web";

const chartConfig: ChartConfig = {
  rttMs: { label: "RTT", color: "var(--chart-3)" },
};

function formatTickTime(ts: number, range: StatsRange): string {
  const d = new Date(ts);
  if (range === "12mo") {
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }
  return d.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

interface ChartPoint {
  ts: number;
  rttMs: number | null;
}

interface Props {
  data: TunnelStatsData | undefined;
  isLoading: boolean;
  isError: boolean;
  range: StatsRange;
  /** Externally controlled zoom — shared with sibling throughput chart. */
  zoom: ZoomRange | null;
  onZoomChange: (zoom: ZoomRange | null) => void;
}

export function TunnelLatencyChart({
  data,
  isLoading,
  isError,
  range,
  zoom,
  onZoomChange,
}: Props) {
  const points = useMemo<ChartPoint[]>(() => {
    if (!data) return [];
    return data.points.map((p) => ({ ts: p.ts, rttMs: p.rttMs }));
  }, [data]);

  const datasetKey = `${range}|${points.length}`;
  const { chartProps, previewRange, isZoomed, reset } = useChartZoom({
    datasetKey,
    zoom,
    onZoomChange,
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>Latency</CardTitle>
      </CardHeader>
      <CardContent>
        {isError ? (
          <div className="flex h-64 items-center justify-center text-sm text-ink-3">
            Failed to load latency history.
          </div>
        ) : isLoading ? (
          <div className="flex h-64 items-center justify-center text-sm text-ink-3">
            Loading…
          </div>
        ) : points.length === 0 ? (
          <div className="flex h-64 items-center justify-center text-sm text-ink-3">
            No latency samples yet — bring this tunnel up to start collecting
            probes.
          </div>
        ) : (
          <ZoomableChartContainer
            config={chartConfig}
            className="h-64 w-full"
            isZoomed={isZoomed}
            onResetZoom={reset}
          >
            <LineChart
              data={points}
              margin={{ top: 8, right: 8, left: 8, bottom: 0 }}
              {...chartProps}
            >
              <CartesianGrid />
              <XAxis
                dataKey="ts"
                type="number"
                domain={zoom ? [zoom.start, zoom.end] : ["dataMin", "dataMax"]}
                allowDataOverflow
                tickFormatter={(ts: number) => formatTickTime(ts, range)}
                minTickGap={48}
                tickMargin={10}
                height={36}
              />
              <YAxis
                width={48}
                tick={{ fontSize: 10 }}
                tickFormatter={(v: number) => `${v} ms`}
              />
              <Tooltip
                labelFormatter={(ts) => new Date(ts as number).toLocaleString()}
                formatter={(value) => [
                  value == null
                    ? "—"
                    : `${typeof value === "number" ? value.toFixed(1) : value} ms`,
                  "RTT",
                ]}
              />
              <Legend formatter={() => "RTT (ms)"} />
              <Line
                type="monotone"
                dataKey="rttMs"
                stroke="var(--color-rttMs)"
                strokeWidth={1.75}
                dot={false}
                connectNulls={false}
                isAnimationActive={false}
                name="rttMs"
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
  );
}
