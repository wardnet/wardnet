export type StatsBucket = "minute" | "hour" | "day";

export interface StatsQuery {
  metric: string;
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

export interface StatsQueryResponse {
  metric: string;
  series: StatsSeriesPoint[];
}

export interface StatsTopQuery {
  metric: string;
  label_key: string;
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
