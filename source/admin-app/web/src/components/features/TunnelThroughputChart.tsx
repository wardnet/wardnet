import { useMemo, useState } from "react";
import { Brush, CartesianGrid, Legend, Line, LineChart, Tooltip, XAxis, YAxis } from "recharts";

import { ChartContainer, type ChartConfig } from "@/components/core/ui/chart";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Tabs, TabsList, TabsTrigger } from "@wardnet/forge-web/tabs";
import { useTunnelMetrics } from "@/hooks/useTunnels";
import { formatBytes } from "@/lib/utils";
import type { TunnelMetricsRange } from "@wardnet/js";

const RANGES: { value: TunnelMetricsRange; label: string }[] = [
  { value: "1h", label: "1h" },
  { value: "6h", label: "6h" },
  { value: "24h", label: "24h" },
  { value: "48h", label: "48h" },
  { value: "12mo", label: "12mo" },
];

const chartConfig: ChartConfig = {
  rx: { label: "Download", color: "var(--chart-1)" },
  tx: { label: "Upload", color: "var(--chart-2)" },
};

interface ChartPoint {
  ts: number;
  /** Bytes per second (for intraday) or per day (for the 12mo range). */
  txRate: number;
  rxRate: number;
}

function formatTickTime(ts: number, range: TunnelMetricsRange): string {
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

function formatRate(rate: number, range: TunnelMetricsRange): string {
  if (range === "12mo") {
    // Daily totals — show per-day total in human-readable bytes.
    return `${formatBytes(rate * 86_400)}/day`;
  }
  return `${formatBytes(rate)}/s`;
}

/**
 * Split a rate string into (number, unit) so the Y-axis tick can render
 * them on two lines — keeps tick labels narrow on mobile.
 *
 * `formatBytes` returns shapes like `"1.2 MB"`; the rate suffix is
 * appended so we get `"1.2 MB/s"` or `"1.2 MB/day"`. Splitting on the
 * first space gives us the numeric magnitude and the rest as the unit.
 */
function splitRate(
  rate: number,
  range: TunnelMetricsRange,
): {
  num: string;
  unit: string;
} {
  const formatted = formatRate(rate, range);
  const ix = formatted.indexOf(" ");
  if (ix === -1) return { num: formatted, unit: "" };
  return { num: formatted.slice(0, ix), unit: formatted.slice(ix + 1) };
}

interface YAxisTickProps {
  x?: number | string;
  y?: number | string;
  payload?: { value: number };
  range: TunnelMetricsRange;
}

/**
 * Two-line Y-axis tick: number on top, unit below. Forge §10 owns
 * the typography (mono numerics, --ink-3) via `.chart .recharts-yAxis
 * .recharts-cartesian-axis-tick text` — Recharts wraps custom-rendered
 * ticks in that selector path, so we don't restate fill/font here.
 */
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
  tunnelId: string;
  range: TunnelMetricsRange;
  onRangeChange: (next: TunnelMetricsRange) => void;
}

export function TunnelThroughputChart({ tunnelId, range, onRangeChange }: Props) {
  const { data, isLoading, isError } = useTunnelMetrics(tunnelId, range);

  const points = useMemo<ChartPoint[]>(() => {
    if (!data) return [];
    const interval = data.interval_secs || 300;
    return data.points.map((p) => ({
      ts: new Date(p.ts).getTime(),
      txRate: p.bytes_tx / interval,
      rxRate: p.bytes_rx / interval,
    }));
  }, [data]);

  // Indexes into `data.points` for the brushed window. We tag the
  // stored selection with the dataset key (tunnel + range + length)
  // and discard it in render whenever the key changes — that way a
  // toggle to a different range or tunnel resets the window without
  // an effect (which the lint rule rightly flags).
  const datasetKey = `${tunnelId}|${range}|${data?.points.length ?? 0}`;
  const [stored, setStored] = useState<{
    key: string;
    start: number;
    end: number;
  } | null>(null);
  const brush = stored?.key === datasetKey ? stored : null;

  const totals = useMemo(() => {
    if (!data) return { tx: 0, rx: 0 };
    const start = brush ? Math.max(0, brush.start) : 0;
    const end = brush ? Math.min(data.points.length - 1, brush.end) : data.points.length - 1;
    let tx = 0;
    let rx = 0;
    for (let i = start; i <= end; i++) {
      tx += data.points[i].bytes_tx;
      rx += data.points[i].bytes_rx;
    }
    return { tx, rx };
  }, [data, brush]);

  return (
    <Card>
      <CardHeader className="flex flex-col gap-3 @md/card-header:flex-row @md/card-header:items-center @md/card-header:justify-between">
        <div>
          <CardTitle>Throughput</CardTitle>
          <p className="mt-1 text-[10px] text-ink-3">
            Window total: ↓ {formatBytes(totals.rx)} · ↑ {formatBytes(totals.tx)}
          </p>
        </div>
        <Tabs value={range} onValueChange={(v) => onRangeChange(v as TunnelMetricsRange)}>
          <TabsList>
            {RANGES.map((r) => (
              <TabsTrigger key={r.value} value={r.value}>
                {r.label}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      </CardHeader>
      <CardContent>
        {isError ? (
          <div className="flex h-64 items-center justify-center text-sm text-ink-3">
            Failed to load throughput history.
          </div>
        ) : isLoading ? (
          <div className="flex h-64 items-center justify-center text-sm text-ink-3">Loading…</div>
        ) : points.length === 0 ? (
          <div className="flex h-64 items-center justify-center text-sm text-ink-3">
            No throughput history yet — bring this tunnel up to start collecting samples.
          </div>
        ) : (
          <ChartContainer config={chartConfig} className="h-64 w-full">
            <LineChart data={points} margin={{ top: 8, right: 8, left: 8, bottom: 0 }}>
              {/* Forge §10 owns the grid contract: horizontal hairlines
                  only (2 4 dash, 1px / --line) and vertical lines hidden.
                  We still render <CartesianGrid> so Recharts emits the
                  selector targets — no props needed; CSS wins. */}
              <CartesianGrid />
              <XAxis
                dataKey="ts"
                type="number"
                domain={["dataMin", "dataMax"]}
                tickFormatter={(ts: number) => formatTickTime(ts, range)}
                minTickGap={48}
                tickMargin={10}
                height={36}
              />
              <YAxis width={48} tick={(props) => <YAxisTick {...props} range={range} />} />
              <Tooltip
                labelFormatter={(ts) => new Date(ts as number).toLocaleString()}
                formatter={(value, name) => [
                  formatRate(typeof value === "number" ? value : 0, range),
                  name === "rxRate" ? "Download" : "Upload",
                ]}
              />
              <Legend formatter={(name) => (name === "rxRate" ? "Download" : "Upload")} />
              {/* Forge §10 stroke weight is 1.6–1.8 px — heavier than a
                  Sparkline (1.5) so the primary chart reads as a real
                  series, lighter than the default Recharts 2 px. */}
              <Line
                type="monotone"
                dataKey="rxRate"
                stroke="var(--color-rx)"
                strokeWidth={1.75}
                dot={false}
                isAnimationActive={false}
              />
              <Line
                type="monotone"
                dataKey="txRate"
                stroke="var(--color-tx)"
                strokeWidth={1.75}
                dot={false}
                isAnimationActive={false}
              />
              <Brush
                dataKey="ts"
                height={24}
                travellerWidth={8}
                stroke="var(--color-rx)"
                // Suppress the built-in start/end labels — they
                // duplicate the X-axis ticks (which already update to
                // reflect the brushed window) and the right-edge label
                // gets clipped by the SVG bounds.
                tickFormatter={() => ""}
                onChange={(r) => {
                  // Recharts emits `{ startIndex, endIndex }` while the
                  // user drags the brush handles. We mirror that into
                  // local state — tagged with the current dataset key
                  // so the selection auto-discards on range/tunnel
                  // change instead of bleeding indices into a fresh
                  // dataset of a different length.
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
        )}
      </CardContent>
    </Card>
  );
}
