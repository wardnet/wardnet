import { useMemo } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  Legend,
  ReferenceArea,
  ReferenceLine,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { ZoomableChartContainer } from "@/components/compound/ZoomableChartContainer";
import { type ChartConfig } from "@/components/core/ui/chart";
import { Card, CardContent, CardHeader, CardTitle, Text } from "@wardnet/web";
import { useChartZoom, type ZoomRange } from "@/hooks/useChartZoom";
import type { TunnelStatsData, StatsRange } from "@wardnet/web";
import { formatBytes } from "@wardnet/web";

const chartConfig: ChartConfig = {
  rx: { label: "Download", color: "var(--chart-1)" },
  tx: { label: "Upload", color: "var(--chart-2)" },
};

interface ChartPoint {
  ts: number;
  /** Upload, stored as a NEGATIVE rate so the area renders below the
   *  zero line in the mirrored throughput layout (pfSense/MRTG
   *  convention). Y-axis ticks + tooltips apply `Math.abs` so the
   *  user sees positive values. */
  txRate: number;
  /** Download, positive — renders above the zero line. */
  rxRate: number;
}

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

function formatRate(rate: number, range: StatsRange): string {
  if (range === "12mo") {
    return `${formatBytes(rate * 86_400)}/day`;
  }
  return `${formatBytes(rate)}/s`;
}

function splitRate(
  rate: number,
  range: StatsRange,
): { num: string; unit: string } {
  const formatted = formatRate(Math.abs(rate), range);
  const ix = formatted.indexOf(" ");
  if (ix === -1) return { num: formatted, unit: "" };
  return { num: formatted.slice(0, ix), unit: formatted.slice(ix + 1) };
}

interface YAxisTickProps {
  x?: number | string;
  y?: number | string;
  payload?: { value: number };
  range: StatsRange;
}

function YAxisTick({ x = 0, y = 0, payload, range }: YAxisTickProps) {
  if (!payload) return null;
  const { num, unit } = splitRate(payload.value, range);
  return (
    <g transform={`translate(${x},${y})`}>
      <text textAnchor="end">
        <tspan x={-4} dy={0}>
          {num}
        </tspan>
        <tspan x={-4} dy={11}>
          {unit}
        </tspan>
      </text>
    </g>
  );
}

interface Props {
  data: TunnelStatsData | undefined;
  isLoading: boolean;
  isError: boolean;
  range: StatsRange;
  /** Externally controlled zoom — shared with sibling latency chart. */
  zoom: ZoomRange | null;
  onZoomChange: (zoom: ZoomRange | null) => void;
}

export function TunnelThroughputChart({
  data,
  isLoading,
  isError,
  range,
  zoom,
  onZoomChange,
}: Props) {
  const points = useMemo<ChartPoint[]>(() => {
    if (!data) return [];
    const interval = data.bucketSecs;
    return data.points.map((p) => ({
      ts: p.ts,
      // Upload is negated so the area renders below the zero line.
      txRate: -(p.bytesTx / interval),
      rxRate: p.bytesRx / interval,
    }));
  }, [data]);

  const datasetKey = `${range}|${points.length}`;
  const { chartProps, previewRange, isZoomed, reset } = useChartZoom({
    datasetKey,
    zoom,
    onZoomChange,
  });

  const totals = useMemo(() => {
    if (!data) return { tx: 0, rx: 0 };
    const startMs = zoom ? Number(zoom.start) : -Infinity;
    const endMs = zoom ? Number(zoom.end) : Infinity;
    let tx = 0;
    let rx = 0;
    for (const p of data.points) {
      if (p.ts >= startMs && p.ts <= endMs) {
        tx += p.bytesTx;
        rx += p.bytesRx;
      }
    }
    return { tx, rx };
  }, [data, zoom]);

  return (
    <Card>
      <CardHeader>
        <div>
          <CardTitle>Throughput</CardTitle>
          <Text as="p" size="2xs" className="mt-1 text-ink-3">
            Window total: ↓ {formatBytes(totals.rx)} · ↑{" "}
            {formatBytes(totals.tx)}
          </Text>
        </div>
      </CardHeader>
      <CardContent>
        {isError ? (
          <Text
            as="div"
            size="sm"
            className="flex h-64 items-center justify-center text-ink-3"
          >
            Failed to load throughput history.
          </Text>
        ) : isLoading ? (
          <Text
            as="div"
            size="sm"
            className="flex h-64 items-center justify-center text-ink-3"
          >
            Loading…
          </Text>
        ) : points.length === 0 ? (
          <Text
            as="div"
            size="sm"
            className="flex h-64 items-center justify-center text-ink-3"
          >
            No throughput history yet — bring this tunnel up to start collecting
            samples.
          </Text>
        ) : (
          <ZoomableChartContainer
            config={chartConfig}
            className="h-64 w-full"
            isZoomed={isZoomed}
            onResetZoom={reset}
          >
            <AreaChart
              data={points}
              margin={{ top: 8, right: 8, left: 8, bottom: 0 }}
              {...chartProps}
            >
              <CartesianGrid />
              <XAxis
                dataKey="ts"
                type="number"
                // When zoomed, constrain the visible domain to the
                // selected window's timestamps and let recharts clip
                // points outside it (`allowDataOverflow`). This keeps
                // `activeTooltipIndex` referring to absolute positions
                // in `points`, so drag-zoom commits are in the same
                // absolute coordinate space as the incoming `zoom`.
                domain={zoom ? [zoom.start, zoom.end] : ["dataMin", "dataMax"]}
                allowDataOverflow
                tickFormatter={(ts: number) => formatTickTime(ts, range)}
                minTickGap={48}
                tickMargin={10}
                height={36}
              />
              <YAxis
                width={48}
                tick={(props) => <YAxisTick {...props} range={range} />}
              />
              <ReferenceLine
                y={0}
                stroke="var(--line-strong)"
                strokeWidth={1}
              />
              <Tooltip
                labelFormatter={(ts) => new Date(ts as number).toLocaleString()}
                formatter={(value, name) => [
                  formatRate(
                    Math.abs(typeof value === "number" ? value : 0),
                    range,
                  ),
                  name === "rxRate" ? "Download" : "Upload",
                ]}
              />
              <Legend
                formatter={(name) =>
                  name === "rxRate" ? "Download" : "Upload"
                }
              />
              <Area
                type="monotone"
                dataKey="rxRate"
                stroke="var(--color-rx)"
                strokeWidth={1.75}
                fill="var(--color-rx)"
                fillOpacity={0.18}
                dot={false}
                isAnimationActive={false}
              />
              <Area
                type="monotone"
                dataKey="txRate"
                stroke="var(--color-tx)"
                strokeWidth={1.75}
                fill="var(--color-tx)"
                fillOpacity={0.18}
                dot={false}
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
            </AreaChart>
          </ZoomableChartContainer>
        )}
      </CardContent>
    </Card>
  );
}
