import { useMemo } from "react";
import { useQuery, useQueries } from "@tanstack/react-query";
import { statsService } from "../lib/sdk";
import type { StatsBucket, StatsTopResponse } from "@wardnet/js";

/** Unified range selector used by all time-series charts. */
export type StatsRange = "1h" | "12h" | "24h" | "7d" | "12mo";

export const RANGES: { value: StatsRange; label: string }[] = [
  { value: "1h", label: "1h" },
  { value: "12h", label: "12h" },
  { value: "24h", label: "24h" },
  { value: "7d", label: "7d" },
  { value: "12mo", label: "12mo" },
];

export const RANGE_HOURS: Record<StatsRange, number> = {
  "1h": 1,
  "12h": 12,
  "24h": 24,
  "7d": 168,
  "12mo": 12 * 30 * 24,
};

function makeWindow(range: StatsRange): {
  from: string;
  to: string;
  bucket: StatsBucket;
} {
  // eslint-disable-next-line security/detect-object-injection -- key is a StatsRange union member indexing Record<StatsRange, number>, not remote input
  const hours = RANGE_HOURS[range];
  const to = new Date();
  const from = new Date(to.getTime() - hours * 3_600_000);
  const bucket: StatsBucket =
    hours <= 24 ? "minute" : hours <= 168 ? "hour" : "day";
  return { from: from.toISOString(), to: to.toISOString(), bucket };
}

export function parseLabels(json: string): Record<string, string> {
  try {
    return JSON.parse(json) as Record<string, string>;
  } catch {
    return {};
  }
}

/** Summary totals for the Dashboard stat cards — 1 query, refetched every 30 s. */
export function useDnsStatSummary(range: StatsRange) {
  const { from, to, bucket } = useMemo(() => makeWindow(range), [range]);

  const { data, isLoading, isError, error } = useQuery({
    queryKey: ["stats", "dns-summary", range],
    queryFn: () =>
      statsService.query({ metric: "dns.queries", from, to, bucket }),
    refetchInterval: 30_000,
  });

  const derived = useMemo(() => {
    if (!data) return undefined;
    let total = 0;
    let blocked = 0;
    for (const point of data.series ?? []) {
      total += point.value;
      if (parseLabels(point.labels).outcome === "blocked")
        blocked += point.value;
    }
    const blockedPercent = total > 0 ? (blocked / total) * 100 : 0;
    return {
      total: Math.round(total),
      blocked: Math.round(blocked),
      blockedPercent,
    };
  }, [data]);

  return { data: derived, isLoading, isError, error };
}

export interface DnsStatsDashboardData {
  series: { ts: string; total: number; blocked: number }[];
  total: number;
  blocked: number;
  blockedPercent: number;
  topDomains: StatsTopResponse;
  topClients: StatsTopResponse;
}

/**
 * Full stats bundle for DnsStatsSection — 3 parallel queries, refetched every 30 s.
 *
 * When `topOverride` is supplied (e.g. from a chart zoom selection) the top-N
 * queries use that window instead of the range-derived window, so all four
 * stat cards reflect the same time slice.  The series query always uses the
 * full range so the chart stays fully populated.
 */
export function useDnsStatsDashboard(
  range: StatsRange,
  topOverride?: { from: string; to: string },
) {
  const { from, to, bucket } = useMemo(() => makeWindow(range), [range]);

  const topFrom = topOverride?.from ?? from;
  const topTo = topOverride?.to ?? to;

  const [queryResult, topDomainsResult, topClientsResult] = useQueries({
    queries: [
      {
        queryKey: ["stats", "dns-series", range],
        queryFn: () =>
          statsService.query({ metric: "dns.queries", from, to, bucket }),
        refetchInterval: 30_000,
      },
      {
        queryKey: ["stats", "dns-top-domains", range, topFrom, topTo],
        queryFn: () =>
          statsService.top({
            metric: "dns.queries.by_domain",
            label_key: "domain",
            // Rank over the tier matching the selected window so a 7d/12mo
            // range ranks the whole period, not just the last ~25h of
            // intraday data.
            bucket,
            from: topFrom,
            to: topTo,
            limit: 10,
          }),
        refetchInterval: 30_000,
      },
      {
        queryKey: ["stats", "dns-top-clients", range, topFrom, topTo],
        queryFn: () =>
          statsService.top({
            metric: "dns.queries.by_client",
            // Rank by write-time device attribution so totals survive DHCP
            // reassigning an IP; unattributed traffic falls back to
            // grouping by its client IP rather than being dropped.
            label_key: "device_id",
            fallback_label_key: "client",
            bucket,
            from: topFrom,
            to: topTo,
            limit: 10,
          }),
        refetchInterval: 30_000,
      },
    ],
  });

  const isLoading =
    queryResult.isLoading ||
    topDomainsResult.isLoading ||
    topClientsResult.isLoading;

  const isError =
    queryResult.isError || topDomainsResult.isError || topClientsResult.isError;

  const error =
    queryResult.error ?? topDomainsResult.error ?? topClientsResult.error;

  const data = useMemo((): DnsStatsDashboardData | undefined => {
    const raw = queryResult.data;
    if (!raw) return undefined;

    const byTs = new Map<string, { total: number; blocked: number }>();
    let total = 0;
    let blocked = 0;

    for (const point of raw.series ?? []) {
      const labels = parseLabels(point.labels);
      const entry = byTs.get(point.ts) ?? { total: 0, blocked: 0 };
      entry.total += point.value;
      total += point.value;
      if (labels.outcome === "blocked") {
        entry.blocked += point.value;
        blocked += point.value;
      }
      byTs.set(point.ts, entry);
    }

    const series = Array.from(byTs.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([ts, v]) => ({ ts, ...v }));

    const blockedPercent = total > 0 ? (blocked / total) * 100 : 0;

    return {
      series,
      total: Math.round(total),
      blocked: Math.round(blocked),
      blockedPercent,
      topDomains: topDomainsResult.data ?? {
        metric: "dns.queries.by_domain",
        entries: [],
      },
      topClients: topClientsResult.data ?? {
        metric: "dns.queries.by_client",
        entries: [],
      },
    };
  }, [queryResult.data, topDomainsResult.data, topClientsResult.data]);

  return { data, isLoading, isError, error };
}

export interface DashboardDnsStats {
  total: number;
  blocked: number;
  blockedPercent: number;
  /** Per-hour total query counts for the last 24h, oldest-first. Feeds the DNS sparkline. */
  totalSeries: number[];
  /** Per-hour blocked counts for the last 24h, oldest-first. Feeds the blocked sparkline. */
  blockedSeries: number[];
}

/**
 * Lightweight 24h DNS summary for the Dashboard cards.
 *
 * Uses hour buckets (24 points) rather than minute buckets so the
 * sparkline series is a manageable size. Distinct query key from
 * useDnsStatSummary("24h") which uses minute buckets.
 */
export function useDashboardDnsStats() {
  const { data, isLoading, isError, error } = useQuery({
    queryKey: ["stats", "dns-dashboard-24h"],
    queryFn: () => {
      // Compute the window inside queryFn so each 30-second refetch uses the
      // current time rather than the time the component first mounted.
      const now = new Date();
      const from = new Date(now.getTime() - 24 * 3_600_000).toISOString();
      const to = now.toISOString();
      return statsService.query({
        metric: "dns.queries",
        from,
        to,
        bucket: "hour",
      });
    },
    refetchInterval: 30_000,
  });

  const derived = useMemo((): DashboardDnsStats | undefined => {
    if (!data) return undefined;

    const byTs = new Map<string, { total: number; blocked: number }>();
    let total = 0;
    let blocked = 0;

    for (const point of data.series ?? []) {
      const entry = byTs.get(point.ts) ?? { total: 0, blocked: 0 };
      entry.total += point.value;
      total += point.value;
      if (parseLabels(point.labels).outcome === "blocked") {
        entry.blocked += point.value;
        blocked += point.value;
      }
      byTs.set(point.ts, entry);
    }

    const sorted = Array.from(byTs.entries()).sort(([a], [b]) =>
      a.localeCompare(b),
    );

    return {
      total: Math.round(total),
      blocked: Math.round(blocked),
      blockedPercent: total > 0 ? (blocked / total) * 100 : 0,
      totalSeries: sorted.map(([, v]) => v.total),
      blockedSeries: sorted.map(([, v]) => v.blocked),
    };
  }, [data]);

  return { data: derived, isLoading, isError, error };
}

export function useDnsTopBlockedDomains(limit = 10) {
  return useQuery({
    queryKey: ["stats", "dns-top-blocked-domains", limit],
    queryFn: () => {
      const now = new Date();
      const from = new Date(now.getTime() - 24 * 3_600_000).toISOString();
      return statsService.top({
        metric: "dns.queries.by_domain",
        label_key: "domain",
        from,
        to: now.toISOString(),
        limit,
      });
    },
    refetchInterval: 30_000,
  });
}

/**
 * Top trackers blocked over `range`, ranked by operating company.
 *
 * Backed by `dns.blocked.by_tracker` — a bounded counter the daemon records
 * only for blocked queries whose domain matches its curated tracker
 * catalogue. Ranks over the tier matching the window (like the top-domains
 * query) so a 7d/12mo range covers the whole period. Honours the same
 * `topOverride` (chart zoom) contract as `useDnsStatsDashboard`.
 */
export function useDnsTopTrackers(
  range: StatsRange,
  topOverride?: { from: string; to: string },
  limit = 10,
) {
  const { from, to, bucket } = useMemo(() => makeWindow(range), [range]);
  const topFrom = topOverride?.from ?? from;
  const topTo = topOverride?.to ?? to;

  return useQuery({
    queryKey: ["stats", "dns-top-trackers", range, topFrom, topTo, limit],
    queryFn: () =>
      statsService.top({
        metric: "dns.blocked.by_tracker",
        label_key: "company",
        bucket,
        from: topFrom,
        to: topTo,
        limit,
      }),
    refetchInterval: 30_000,
  });
}

export interface DnsPeriodTotals {
  total: number;
  blocked: number;
  blockedPercent: number;
}

export interface DnsPeriodComparison {
  current: DnsPeriodTotals;
  previous: DnsPeriodTotals;
  /** Signed percentage change in total queries vs the previous period. */
  totalChangePercent: number;
  /** Signed percentage change in blocked queries vs the previous period. */
  blockedChangePercent: number;
}

function sumTotals(
  series: { value: number; labels: string }[] | undefined,
): DnsPeriodTotals {
  let total = 0;
  let blocked = 0;
  for (const point of series ?? []) {
    total += point.value;
    if (parseLabels(point.labels).outcome === "blocked") blocked += point.value;
  }
  return {
    total: Math.round(total),
    blocked: Math.round(blocked),
    blockedPercent: total > 0 ? (blocked / total) * 100 : 0,
  };
}

function percentChange(current: number, previous: number): number {
  if (previous === 0) return current > 0 ? 100 : 0;
  return ((current - previous) / previous) * 100;
}

/**
 * Period-over-period comparison of DNS volume for `range` (this window vs the
 * immediately-preceding window of equal length) — e.g. week-over-week when
 * `range` is `"7d"`. Two `dns.queries` series queries, summed client-side.
 */
export function useDnsPeriodComparison(range: StatsRange): {
  data: DnsPeriodComparison | undefined;
  isLoading: boolean;
  isError: boolean;
  error: unknown;
} {
  // eslint-disable-next-line security/detect-object-injection -- key is a StatsRange union member indexing Record<StatsRange, number>, not remote input
  const hours = RANGE_HOURS[range];

  const [currentResult, previousResult] = useQueries({
    queries: [
      {
        queryKey: ["stats", "dns-period-current", range],
        queryFn: () => {
          const now = new Date();
          const from = new Date(now.getTime() - hours * 3_600_000);
          const bucket: StatsBucket =
            hours <= 24 ? "minute" : hours <= 168 ? "hour" : "day";
          return statsService.query({
            metric: "dns.queries",
            from: from.toISOString(),
            to: now.toISOString(),
            bucket,
          });
        },
        refetchInterval: 30_000,
      },
      {
        queryKey: ["stats", "dns-period-previous", range],
        queryFn: () => {
          const now = new Date();
          const to = new Date(now.getTime() - hours * 3_600_000);
          const from = new Date(to.getTime() - hours * 3_600_000);
          const bucket: StatsBucket =
            hours <= 24 ? "minute" : hours <= 168 ? "hour" : "day";
          return statsService.query({
            metric: "dns.queries",
            from: from.toISOString(),
            to: to.toISOString(),
            bucket,
          });
        },
        refetchInterval: 30_000,
      },
    ],
  });

  const data = useMemo((): DnsPeriodComparison | undefined => {
    if (!currentResult.data || !previousResult.data) return undefined;
    const current = sumTotals(currentResult.data.series);
    const previous = sumTotals(previousResult.data.series);
    return {
      current,
      previous,
      totalChangePercent: percentChange(current.total, previous.total),
      blockedChangePercent: percentChange(current.blocked, previous.blocked),
    };
  }, [currentResult.data, previousResult.data]);

  return {
    data,
    isLoading: currentResult.isLoading || previousResult.isLoading,
    isError: currentResult.isError || previousResult.isError,
    error: currentResult.error ?? previousResult.error,
  };
}

export interface DevicePoint {
  ts: string;
  total: number;
}

/**
 * Per-device query timeseries over `range`, backed by `dns.queries.by_device`
 * (a device-keyed counter, so the series does not fragment when the device's
 * IP changes). Disabled until a `deviceId` is selected.
 */
export function useDnsPerDeviceStats(
  deviceId: string | null,
  range: StatsRange,
): {
  data: DevicePoint[] | undefined;
  isLoading: boolean;
  isError: boolean;
  error: unknown;
} {
  const { from, to, bucket } = useMemo(() => makeWindow(range), [range]);

  const { data, isLoading, isError, error } = useQuery({
    queryKey: ["stats", "dns-per-device", deviceId, range],
    enabled: deviceId != null,
    queryFn: () =>
      statsService.query({
        metric: "dns.queries.by_device",
        label_filter: JSON.stringify({ device_id: deviceId }),
        from,
        to,
        bucket,
      }),
    refetchInterval: 30_000,
  });

  const points = useMemo((): DevicePoint[] | undefined => {
    if (!data) return undefined;
    const byTs = new Map<string, number>();
    for (const point of data.series ?? []) {
      byTs.set(point.ts, (byTs.get(point.ts) ?? 0) + point.value);
    }
    return Array.from(byTs.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([ts, total]) => ({ ts, total: Math.round(total) }));
  }, [data]);

  return { data: points, isLoading, isError, error };
}
