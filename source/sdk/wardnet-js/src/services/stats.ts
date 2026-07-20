import type { WardnetClient } from "../client.js";
import type {
  StatsBucket,
  StatsQuery,
  StatsQueryResponse,
  StatsSeriesPoint,
  StatsTopQuery,
  StatsTopResponse,
} from "../types/stats.js";

export class StatsService {
  constructor(private readonly client: WardnetClient) {}

  /**
   * Run a stats query. Pass either `metric` (single series under
   * `series`) or `metrics` (map of series under `results`) — exactly
   * one of the two. The server rejects both-or-neither with 400.
   */
  async query(q: StatsQuery): Promise<StatsQueryResponse> {
    const enc = encodeURIComponent;
    const parts: string[] = [`from=${enc(q.from)}`, `to=${enc(q.to)}`, `bucket=${enc(q.bucket)}`];
    if (q.metric != null) parts.push(`metric=${enc(q.metric)}`);
    if (q.metrics != null) {
      // Repeated `metrics=` keys — server uses serde_html_form which
      // parses repeated query params into Vec<String>. Comma-joining
      // would not work.
      for (const m of q.metrics) parts.push(`metrics=${enc(m)}`);
    }
    if (q.label_filter != null) parts.push(`label_filter=${enc(q.label_filter)}`);
    return this.client.request<StatsQueryResponse>(`/stats?${parts.join("&")}`);
  }

  /**
   * Convenience wrapper around the multi-metric form of `query`. Returns
   * the `results` map directly (each metric has its own series), with
   * any missing metric defaulting to an empty array so callers can rely
   * on every requested key being present.
   */
  async queryMulti(args: {
    metrics: string[];
    from: string;
    to: string;
    bucket: StatsBucket;
    label_filter?: string | null;
  }): Promise<Record<string, StatsSeriesPoint[]>> {
    const resp = await this.query({
      metrics: args.metrics,
      from: args.from,
      to: args.to,
      bucket: args.bucket,
      label_filter: args.label_filter,
    });
    const out: Record<string, StatsSeriesPoint[]> = {};
    // eslint-disable-next-line security/detect-object-injection -- key is the caller-supplied metric name from the local request list, not from the API response
    for (const m of args.metrics) out[m] = resp.results?.[m] ?? [];
    return out;
  }

  async top(q: StatsTopQuery): Promise<StatsTopResponse> {
    const enc = encodeURIComponent;
    const parts = [
      `metric=${enc(q.metric)}`,
      `label_key=${enc(q.label_key)}`,
      `from=${enc(q.from)}`,
      `to=${enc(q.to)}`,
      `limit=${q.limit}`,
    ];
    if (q.fallback_label_key) {
      parts.push(`fallback_label_key=${enc(q.fallback_label_key)}`);
    }
    if (q.bucket) {
      parts.push(`bucket=${enc(q.bucket)}`);
    }
    return this.client.request<StatsTopResponse>(`/stats/top?${parts.join("&")}`);
  }
}
