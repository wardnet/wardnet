import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { statsService } from "../lib/sdk";

const BUCKET_SECS = 60;

export interface CombinedTunnelStatsData {
  /** Combined tx+rx values per minute bucket, sorted ascending by timestamp. */
  sparkValues: number[];
  /** Current download rate in bytes/s (last bucket ÷ bucket seconds). */
  rxRate: number;
  /** Current upload rate in bytes/s (last bucket ÷ bucket seconds). */
  txRate: number;
}

/**
 * Fetches aggregate `tunnel.bytes.tx` and `tunnel.bytes.rx` across all
 * tunnels for the trailing 1-hour window (1-minute buckets). The window
 * advances on each 60-second refetch. Use for the summary header sparkline
 * and combined throughput rate.
 */
export function useCombinedTunnelStats(): {
  data: CombinedTunnelStatsData | undefined;
  isLoading: boolean;
} {
  const { data: rawStats, isLoading } = useQuery({
    queryKey: ["stats", "tunnels-combined", "1h"],
    queryFn: () => {
      const now = new Date();
      return statsService.queryMulti({
        metrics: ["tunnel.bytes.tx", "tunnel.bytes.rx"],
        to: now.toISOString(),
        from: new Date(now.getTime() - 3_600_000).toISOString(),
        bucket: "minute",
      });
    },
    refetchInterval: 60_000,
  });

  const data = useMemo<CombinedTunnelStatsData | undefined>(() => {
    if (!rawStats) return undefined;

    const byTs = new Map<number, { tx: number; rx: number }>();
    for (const p of rawStats["tunnel.bytes.tx"] ?? []) {
      const ts = new Date(p.ts).getTime();
      const e = byTs.get(ts) ?? { tx: 0, rx: 0 };
      e.tx += p.value;
      byTs.set(ts, e);
    }
    for (const p of rawStats["tunnel.bytes.rx"] ?? []) {
      const ts = new Date(p.ts).getTime();
      const e = byTs.get(ts) ?? { tx: 0, rx: 0 };
      e.rx += p.value;
      byTs.set(ts, e);
    }

    const sortedEntries = Array.from(byTs.entries()).sort(([a], [b]) => a - b);
    const sparkValues = sortedEntries.map(([, v]) => v.tx + v.rx);
    const last = sortedEntries[sortedEntries.length - 1]?.[1] ?? { tx: 0, rx: 0 };

    return {
      sparkValues,
      rxRate: last.rx / BUCKET_SECS,
      txRate: last.tx / BUCKET_SECS,
    };
  }, [rawStats]);

  return { data, isLoading };
}
