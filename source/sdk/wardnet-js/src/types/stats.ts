export type StatsBucket = "minute" | "hour" | "day";

/**
 * Parameters for a time-series stats query.
 *
 * Exactly one of `metric` or `metrics` must be set:
 *  - `metric` returns a flat series under `series` in the response.
 *  - `metrics` runs one query per name (sharing the same `from`/`to`/
 *    `bucket`/`label_filter`) and returns a `metric → series` map
 *    under `results` — used to fetch multiple related metrics (e.g.
 *    tunnel tx, rx, and `rtt_ms`) in a single round-trip.
 */
export interface StatsQuery {
  metric?: string;
  metrics?: string[];
  label_filter?: string | null;
  from: string;
  to: string;
  bucket: StatsBucket;
}

export interface StatsSeriesPoint {
  ts: string;
  value: number;
  labels: string;
}

/**
 * Response shape mirrors the request: a single-metric query returns
 * `metric` + `series`; a multi-metric query returns `results` (metric
 * name → series). Exactly one of `series` / `results` is populated.
 */
export interface StatsQueryResponse {
  metric?: string;
  series?: StatsSeriesPoint[];
  results?: Record<string, StatsSeriesPoint[]>;
}

export interface StatsTopQuery {
  metric: string;
  label_key: string;
  /**
   * Label dimension to group by when `label_key` is absent from an
   * entry's labels (e.g. rank by `device_id` falling back to `client`
   * for queries from sources with no known device).
   */
  fallback_label_key?: string;
  /**
   * Storage tier to rank over. Omitted defaults to `"minute"` (intraday,
   * ~25h) server-side; pass `"hour"` or `"day"` to rank over the longer
   * tiers so the ranking covers the same window as a matching time-series
   * query (e.g. top trackers over the last 7 days).
   */
  bucket?: StatsBucket;
  from: string;
  to: string;
  limit: number;
}

export interface StatsTopEntry {
  labels: string;
  total: number;
}

export interface StatsTopResponse {
  metric: string;
  entries: StatsTopEntry[];
}
