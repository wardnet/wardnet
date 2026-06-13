import { useCallback, useEffect, useState } from "react";

import {
  type DnsEventItem,
  recentDates,
  subscribeStats,
} from "@/lib/dnsDb";
import {
  type DayHeadline,
  type DomainCount,
  type TrendDay,
  getDayHeadline,
  getRecentActivity,
  getTopBlocked,
  getTopQueried,
  getTrend,
  hasAnyData,
} from "@/lib/dnsStats";

export const TREND_DAYS = 7;

export interface DnsStatsData {
  headline: DayHeadline;
  topBlocked: DomainCount[];
  topQueried: DomainCount[];
  recent: DnsEventItem[];
  trend: TrendDay[];
  hasData: boolean;
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

  const load = useCallback(async () => {
    const [headline, topBlocked, topQueried, recent, trend, dataPresent] =
      await Promise.all([
        getDayHeadline(date),
        getTopBlocked(date),
        getTopQueried(date),
        getRecentActivity(20),
        getTrend(recentDates(TREND_DAYS)),
        hasAnyData(),
      ]);
    setData({
      headline,
      topBlocked,
      topQueried,
      recent,
      trend,
      hasData: dataPresent,
      loading: false,
    });
  }, [date]);

  useEffect(() => {
    let active = true;
    const run = () => {
      load().catch(() => {
        if (active) setData((d) => ({ ...d, loading: false }));
      });
    };
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
