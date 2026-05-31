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

function makeWindow(range: StatsRange): { from: string; to: string; bucket: StatsBucket } {
  const hours = RANGE_HOURS[range];
  const to = new Date();
  const from = new Date(to.getTime() - hours * 3_600_000);
  const bucket: StatsBucket = hours <= 24 ? "minute" : hours <= 168 ? "hour" : "day";
  return { from: from.toISOString(), to: to.toISOString(), bucket };
}

function parseLabels(json: string): Record<string, string> {
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
    queryFn: () => statsService.query({ metric: "dns.queries", from, to, bucket }),
    refetchInterval: 30_000,
  });

  const derived = useMemo(() => {
    if (!data) return undefined;
    let total = 0;
    let blocked = 0;
    for (const point of data.series ?? []) {
      total += point.value;
      if (parseLabels(point.labels).outcome === "blocked") blocked += point.value;
    }
    const blockedPercent = total > 0 ? (blocked / total) * 100 : 0;
    return { total: Math.round(total), blocked: Math.round(blocked), blockedPercent };
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
        queryFn: () => statsService.query({ metric: "dns.queries", from, to, bucket }),
        refetchInterval: 30_000,
      },
      {
        queryKey: ["stats", "dns-top-domains", range, topFrom, topTo],
        queryFn: () =>
          statsService.top({
            metric: "dns.queries.by_domain",
            label_key: "domain",
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
            label_key: "client",
            from: topFrom,
            to: topTo,
            limit: 10,
          }),
        refetchInterval: 30_000,
      },
    ],
  });

  const isLoading =
    queryResult.isLoading || topDomainsResult.isLoading || topClientsResult.isLoading;

  const isError = queryResult.isError || topDomainsResult.isError || topClientsResult.isError;

  const error = queryResult.error ?? topDomainsResult.error ?? topClientsResult.error;

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
      topDomains: topDomainsResult.data ?? { metric: "dns.queries.by_domain", entries: [] },
      topClients: topClientsResult.data ?? { metric: "dns.queries.by_client", entries: [] },
    };
  }, [queryResult.data, topDomainsResult.data, topClientsResult.data]);

  return { data, isLoading, isError, error };
}
