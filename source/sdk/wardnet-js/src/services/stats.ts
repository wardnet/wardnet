import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  StatsBucket,
  StatsQuery,
  StatsQueryResponse,
  StatsSeriesPoint,
  StatsTopQuery,
  StatsTopResponse,
} from "../types/stats.js";

export class StatsService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /**
   * Run a stats query. Pass either `metric` (single series under
   * `series`) or `metrics` (map of series under `results`) — exactly
   * one of the two. The server rejects both-or-neither with 400.
   *
   * Repeated `metrics=` keys are serialized by the client's query
   * encoder — the daemon's `serde_html_form` parses them into a
   * `Vec<String>`, so comma-joining would not work.
   */
  async query(q: StatsQuery): Promise<StatsQueryResponse> {
    return this.api.get("/stats", { query: q });
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
    return this.api.get("/stats/top", { query: q });
  }
}
