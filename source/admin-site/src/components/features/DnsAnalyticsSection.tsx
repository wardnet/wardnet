import { useMemo, useState } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import {
  ChartContainer,
  type ChartConfig,
} from "@/components/core/ui/chart";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Text,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  useDevices,
  useDnsPeriodComparison,
  useDnsPerDeviceStats,
  useDnsTopTrackers,
  deviceDisplayName,
  parseLabels,
  RANGE_HOURS,
  type StatsRange,
} from "@wardnet/web";
import type { StatsTopEntry } from "@wardnet/js";

const perDeviceConfig: ChartConfig = {
  total: { label: "Queries", color: "var(--chart-1)" },
};

/**
 * Human phrase for the window a comparison is measured over, so the
 * period-over-period copy reads naturally ("vs previous week").
 */
function periodNoun(range: StatsRange): string {
  switch (range) {
    case "1h":
      return "hour";
    case "12h":
      return "12 hours";
    case "24h":
      return "day";
    case "7d":
      return "week";
    case "12mo":
      return "year";
  }
}

function formatXAxis(tsMs: number, range: StatsRange): string {
  const d = new Date(tsMs);
  // eslint-disable-next-line security/detect-object-injection -- range is a closed StatsRange union key into the RANGE_HOURS const map
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

/**
 * Deeper DNS analytics for the DNS page, rendered under the same range
 * selector as {@link DnsStatsSection}: period-over-period comparison, top
 * trackers blocked (categorised by operating company), and a per-device
 * query timeseries.
 */
export function DnsAnalyticsSection({ range }: Props) {
  const { data: comparison } = useDnsPeriodComparison(range);
  const { data: trackers } = useDnsTopTrackers(range);
  const { data: devicesData } = useDevices();

  const devices = devicesData?.devices ?? [];
  const [selectedDevice, setSelectedDevice] = useState<string>("");
  // Default to the first device once the list loads, so the chart shows
  // something without the user having to pick.
  const effectiveDevice =
    selectedDevice || (devices.length > 0 ? devices[0].id : "");
  const { data: deviceSeries, isLoading: deviceLoading } = useDnsPerDeviceStats(
    effectiveDevice || null,
    range,
  );

  const chartSeries = useMemo(
    () =>
      (deviceSeries ?? []).map((p) => ({
        ...p,
        tsMs: new Date(p.ts).getTime(),
      })),
    [deviceSeries],
  );

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-4 lg:grid-cols-2">
        <Card data-testid="dns-period-comparison">
          <CardHeader>
            <CardTitle className="text-sm">
              Compared to previous {periodNoun(range)}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {comparison ? (
              <div className="grid grid-cols-2 gap-4">
                <ComparisonTile
                  label="Queries"
                  current={comparison.current.total}
                  changePercent={comparison.totalChangePercent}
                  testId="dns-comparison-queries"
                />
                <ComparisonTile
                  label="Blocked"
                  current={comparison.current.blocked}
                  changePercent={comparison.blockedChangePercent}
                  // More blocking is not "worse" — keep the sign neutral so a
                  // rise in blocks does not read as a red regression.
                  neutral
                  testId="dns-comparison-blocked"
                />
              </div>
            ) : (
              <Text as="p" size="sm" className="text-ink-3">
                No data yet.
              </Text>
            )}
          </CardContent>
        </Card>

        <TrackersList
          title={`Top trackers blocked (${range})`}
          entries={trackers?.entries}
        />
      </div>

      <Card data-testid="dns-per-device-chart">
        <CardHeader>
          <div className="flex items-center justify-between gap-3">
            <CardTitle>Per-device queries</CardTitle>
            {devices.length > 0 && (
              <Select
                value={effectiveDevice}
                onValueChange={setSelectedDevice}
              >
                <SelectTrigger
                  data-testid="dns-per-device-select"
                  className="w-48"
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {devices.map((d) => (
                    <SelectItem key={d.id} value={d.id}>
                      {deviceDisplayName(d)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
        </CardHeader>
        <CardContent>
          {devices.length === 0 ? (
            <Text
              as="div"
              size="sm"
              className="flex h-56 items-center justify-center text-ink-3"
            >
              No devices yet.
            </Text>
          ) : deviceLoading ? (
            <Text
              as="div"
              size="sm"
              className="flex h-56 items-center justify-center text-ink-3"
            >
              Loading…
            </Text>
          ) : chartSeries.length === 0 ? (
            <Text
              as="div"
              size="sm"
              className="flex h-56 items-center justify-center text-ink-3"
            >
              No queries recorded for this device yet.
            </Text>
          ) : (
            <ChartContainer config={perDeviceConfig} className="h-56 w-full">
              <LineChart
                data={chartSeries}
                margin={{ top: 8, right: 8, left: 8, bottom: 0 }}
              >
                <CartesianGrid strokeDasharray="3 3" />
                <XAxis
                  dataKey="tsMs"
                  type="number"
                  domain={["dataMin", "dataMax"]}
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
                <Line
                  type="monotone"
                  dataKey="total"
                  stroke="var(--color-total)"
                  strokeWidth={2}
                  dot={false}
                  name="Queries"
                  isAnimationActive={false}
                />
              </LineChart>
            </ChartContainer>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function ComparisonTile({
  label,
  current,
  changePercent,
  neutral = false,
  testId,
}: {
  label: string;
  current: number;
  changePercent: number;
  neutral?: boolean;
  testId?: string;
}) {
  const rounded = Math.round(changePercent);
  const arrow = rounded > 0 ? "▲" : rounded < 0 ? "▼" : "–";
  // Colour only when direction carries meaning: fewer queries is greener,
  // more is amber. Blocked passes `neutral` so it stays ink-coloured.
  const tone = neutral
    ? "text-ink-3"
    : rounded > 0
      ? "text-amber-600"
      : rounded < 0
        ? "text-emerald-600"
        : "text-ink-3";
  return (
    <div className="rounded-md bg-surface-2 p-3" data-testid={testId}>
      <Text as="p" size="2xs" className="text-ink-3">
        {label}
      </Text>
      <Text as="p" size="lg" className="mt-1 tabular-nums">
        {current.toLocaleString()}
      </Text>
      <Text as="p" size="xs" className={`mt-1 tabular-nums ${tone}`}>
        {arrow} {Math.abs(rounded)}%
      </Text>
    </div>
  );
}

function TrackersList({
  title,
  entries,
}: {
  title: string;
  entries: StatsTopEntry[] | undefined;
}) {
  return (
    <Card data-testid="dns-top-trackers">
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        {entries && entries.length > 0 ? (
          <Text as="ul" size="sm" className="flex flex-col gap-2">
            {entries.map((entry, i) => {
              const company = parseLabels(entry.labels).company ?? entry.labels;
              return (
                <li key={i} className="flex items-center justify-between gap-2">
                  <Text size="xs" className="truncate">
                    {company}
                  </Text>
                  <span className="tabular-nums text-ink-3">
                    {Math.round(entry.total).toLocaleString()} blocks
                  </span>
                </li>
              );
            })}
          </Text>
        ) : (
          <Text as="p" size="sm" className="text-ink-3">
            No recognised trackers blocked yet.
          </Text>
        )}
      </CardContent>
    </Card>
  );
}
