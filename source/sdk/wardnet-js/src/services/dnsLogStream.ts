import type { WardnetClient } from "../client.js";
import type { QueryLogEvent } from "../types/dns.js";

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
 * Manages a WebSocket connection to `/api/dns/log/stream`. Auto-reconnects
 * with a 3-second backoff. Filter commands are sent via the same socket.
 */
export class DnsLogStreamService {
  private ws: WebSocket | null = null;
  private callbacks: DnsLogStreamCallbacks | null = null;
  private filter: DnsLogStreamFilter = {};
  private paused = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly origin: string;

  constructor(
    private readonly client: WardnetClient,
    origin = "http://localhost:7411",
  ) {
    this.origin = origin;
  }

  private wsUrl(): string {
    const base = this.client.baseUrl;
    if (base.startsWith("http")) {
      const httpUrl = new URL(base);
      const protocol = httpUrl.protocol === "https:" ? "wss:" : "ws:";
      return `${protocol}//${httpUrl.host}${httpUrl.pathname}/dns/log/stream`;
    }
    const origin = this.origin;
    const protocol = origin.startsWith("https") ? "wss:" : "ws:";
    const url = new URL(origin);
    return `${protocol}//${url.host}${base}/dns/log/stream`;
  }

  connect(callbacks: DnsLogStreamCallbacks, initialFilter?: DnsLogStreamFilter): void {
    this.callbacks = callbacks;
    if (initialFilter) this.filter = initialFilter;
    this.paused = false;
    this.doConnect();
  }

  private doConnect(): void {
    if (this.paused || !this.callbacks) return;

    const ws = new WebSocket(this.wsUrl());
    this.ws = ws;

    ws.onopen = () => {
      this.callbacks?.onConnected();
      ws.send(JSON.stringify({ type: "set_filter", ...this.filter }));
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.type === "lagged") {
          this.callbacks?.onLagged(data.skipped ?? 0);
          return;
        }
        this.callbacks?.onEvent(data as QueryLogEvent);
      } catch {
        // Ignore unparseable messages.
      }
    };

    ws.onclose = () => {
      this.callbacks?.onDisconnected();
      this.ws = null;
      if (!this.paused) {
        this.reconnectTimer = setTimeout(() => this.doConnect(), 3000);
      }
    };

    ws.onerror = () => ws.close();
  }

  setFilter(filter: DnsLogStreamFilter): void {
    this.filter = filter;
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "set_filter", ...filter }));
    }
  }

  pause(): void {
    this.paused = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
  }

  resume(): void {
    this.paused = false;
    this.doConnect();
  }

  disconnect(): void {
    this.paused = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
    this.callbacks = null;
  }
}
