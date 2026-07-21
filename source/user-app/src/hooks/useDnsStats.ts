import { useCallback, useEffect, useState } from "react";

import { recentDates, subscribeStats, TREND_DAYS } from "@/lib/dnsDb";
import { type DnsStatsSnapshot, loadDnsStats } from "@/lib/dnsStats";

// TREND_DAYS is defined in dnsDb (the layer pruneDaily also depends on) so the
// read window and the retention window share one source; re-exported here for
// the historical import path.
export { TREND_DAYS };

export interface DnsStatsData extends DnsStatsSnapshot {
  loading: boolean;
}

const EMPTY: DnsStatsData = {
  headline: { total: 0, blocked: 0, allowed: 0 },
  topBlocked: [],
  topQueried: [],
  recent: [],
  trend: [],
  hasData: false,
  loading: true,
};

/**
 * Reads device-local DNS stats for a given day from IndexedDB. Re-queries when
 * the sync hook reports new events (debounced) and whenever `date` changes.
 * Never touches the daemon — data is always instantly available (or empty).
 */
export function useDnsStats(date: string): DnsStatsData {
  const [data, setData] = useState<DnsStatsData>(EMPTY);

  const load = useCallback(async (): Promise<DnsStatsData> => {
    const snapshot = await loadDnsStats(date, recentDates(TREND_DAYS));
    return { ...snapshot, loading: false };
  }, [date]);

  useEffect(() => {
    let active = true;
    const run = () => {
      load()
        .then((next) => {
          // Guard the commit: a slow read for a previously-selected date must
          // not overwrite the current one after the effect has been torn down.
          if (active) setData(next);
        })
        .catch(() => {
          if (active) setData((d) => ({ ...d, loading: false }));
        });
    };

    // A newly selected date must not keep showing the previous day's counts
    // while its read is in flight. Blank the day-specific fields and mark
    // loading; the window-scoped trend/recent/hasData stay put so the chart
    // and empty-state don't flash on every switch.
    setData((d) => ({
      ...d,
      headline: EMPTY.headline,
      topBlocked: [],
      topQueried: [],
      loading: true,
    }));
    run();

    // Coalesce bursts of new events into one reload.
    let timer: ReturnType<typeof setTimeout> | null = null;
    const unsubscribe = subscribeStats(() => {
      if (timer !== null) return;
      timer = setTimeout(() => {
        timer = null;
        run();
      }, 500);
    });

    return () => {
      active = false;
      if (timer !== null) clearTimeout(timer);
      unsubscribe();
    };
  }, [load]);

  return data;
}
