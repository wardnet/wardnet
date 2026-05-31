import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { statsService } from "../lib/sdk";
import { RANGE_HOURS, type StatsRange } from "./useStats";
import type { StatsBucket } from "@wardnet/js";

export { RANGES } from "./useStats";
export type { StatsRange } from "./useStats";

interface Window {
  from: string;
  to: string;
  bucket: StatsBucket;
  bucketSecs: number;
}

function makeWindow(range: StatsRange): Window {
  const hours = RANGE_HOURS[range];
  const to = new Date();
  const from = new Date(to.getTime() - hours * 3_600_000);
  const bucket: StatsBucket = hours <= 24 ? "minute" : hours <= 168 ? "hour" : "day";
  const bucketSecs = bucket === "minute" ? 60 : bucket === "hour" ? 3_600 : 86_400;
  return { from: from.toISOString(), to: to.toISOString(), bucket, bucketSecs };
}

export interface TunnelStatsPoint {
  /** Milliseconds since epoch — bucket end. */
  ts: number;
  /** Raw counter value (bytes) in this bucket. */
  bytesTx: number;
  /** Raw counter value (bytes) in this bucket. */
  bytesRx: number;
  /** Gauge value for this bucket, or `null` if no probe landed in it. */
  rttMs: number | null;
}

export interface TunnelStatsData {
  points: TunnelStatsPoint[];
  bucketSecs: number;
  range: StatsRange;
}

/**
 * Fetches tunnel throughput (tunnel.bytes.tx / rx) and latency
 * (tunnel.latency.rtt_ms) in one round-trip, filtered to a single
 * tunnel id. Merges the three series into one timeline indexed by
 * bucket end. Refresh cadence matches the bucket granularity (1 min
 * for intraday, 5 min for hour, 1 h for daily).
 */
export function useTunnelStats(tunnelId: string, range: StatsRange) {
  const { from, to, bucket, bucketSecs } = useMemo(() => makeWindow(range), [range]);

  // `label_filter` is matched against the canonical sorted-JSON label
  // string the daemon writes — single-key filters are unambiguous.
  const labelFilter = useMemo(() => JSON.stringify({ tunnel_id: tunnelId }), [tunnelId]);

  const refetchInterval = bucket === "minute" ? 60_000 : bucket === "hour" ? 5 * 60_000 : 3_600_000;

  const { data, isLoading, isError, error } = useQuery({
    queryKey: ["stats", "tunnel", tunnelId, range],
    queryFn: () =>
      statsService.queryMulti({
        metrics: ["tunnel.bytes.tx", "tunnel.bytes.rx", "tunnel.latency.rtt_ms"],
        from,
        to,
        bucket,
        label_filter: labelFilter,
      }),
    enabled: !!tunnelId,
    refetchInterval,
  });

  const shaped = useMemo<TunnelStatsData | undefined>(() => {
    if (!data) return undefined;

    const byTs = new Map<number, TunnelStatsPoint>();
    const upsert = (ts: number) => {
      let p = byTs.get(ts);
      if (!p) {
        p = { ts, bytesTx: 0, bytesRx: 0, rttMs: null };
        byTs.set(ts, p);
      }
      return p;
    };

    for (const point of data["tunnel.bytes.tx"] ?? []) {
      upsert(new Date(point.ts).getTime()).bytesTx += point.value;
    }
    for (const point of data["tunnel.bytes.rx"] ?? []) {
      upsert(new Date(point.ts).getTime()).bytesRx += point.value;
    }
    for (const point of data["tunnel.latency.rtt_ms"] ?? []) {
      // Gauges are last-write-wins per bucket; if multiple labels collapse
      // to the same bucket (shouldn't happen with a single tunnel_id
      // filter) the later value wins.
      upsert(new Date(point.ts).getTime()).rttMs = point.value;
    }

    const points = Array.from(byTs.values()).sort((a, b) => a.ts - b.ts);
    return { points, bucketSecs, range };
  }, [data, bucketSecs, range]);

  return { data: shaped, isLoading, isError, error };
}
