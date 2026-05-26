import type { WardnetClient } from "../client.js";
import type {
  StatsQuery,
  StatsQueryResponse,
  StatsTopQuery,
  StatsTopResponse,
} from "../types/stats.js";

export class StatsService {
  constructor(private readonly client: WardnetClient) {}

  async query(q: StatsQuery): Promise<StatsQueryResponse> {
    const enc = encodeURIComponent;
    const parts = [
      `metric=${enc(q.metric)}`,
      `from=${enc(q.from)}`,
      `to=${enc(q.to)}`,
      `bucket=${enc(q.bucket)}`,
    ];
    if (q.label_filter != null) parts.push(`label_filter=${enc(q.label_filter)}`);
    return this.client.request<StatsQueryResponse>(`/stats?${parts.join("&")}`);
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
    return this.client.request<StatsTopResponse>(`/stats/top?${parts.join("&")}`);
  }
}
