import type { WardnetClient } from "../client.js";
import type { QueryLogEvent } from "../types/dns.js";
import { ReconnectingStream } from "./reconnect.js";

/** Filter settings sent to the server via WebSocket command. */
export interface DnsLogStreamFilter {
  domain?: string;
  client_ip?: string;
  results?: string[];
}

/** Callbacks for the DNS query stream consumer. */
export interface DnsLogStreamCallbacks {
  onEvent: (event: QueryLogEvent) => void;
  onLagged: (skipped: number) => void;
  onConnected: () => void;
  onDisconnected: () => void;
}

/**
 * DNS query log streaming service.
 *
 * Manages a WebSocket connection to `/api/dns/log/stream` with auto-reconnect
 * (exponential backoff + jitter, via {@link ReconnectingStream}). Filter
 * commands are sent via the same socket.
 */
export class DnsLogStreamService {
  private readonly stream: ReconnectingStream<DnsLogStreamFilter, QueryLogEvent>;

  constructor(client: WardnetClient, origin = "http://localhost:7411") {
    this.stream = new ReconnectingStream(client, "/dns/log/stream", {}, origin);
  }

  connect(callbacks: DnsLogStreamCallbacks, initialFilter?: DnsLogStreamFilter): void {
    this.stream.start(
      {
        onItem: callbacks.onEvent,
        onLagged: callbacks.onLagged,
        onConnected: callbacks.onConnected,
        onDisconnected: callbacks.onDisconnected,
      },
      initialFilter,
    );
  }

  setFilter(filter: DnsLogStreamFilter): void {
    this.stream.setFilter(filter);
  }

  pause(): void {
    this.stream.pause();
  }

  resume(): void {
    this.stream.resume();
  }

  disconnect(): void {
    this.stream.disconnect();
  }
}
