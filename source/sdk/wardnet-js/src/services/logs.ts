import type { WardnetClient } from "../client.js";

/** A single structured log entry from the WebSocket stream. */
export interface LogEntry {
  timestamp: string;
  level: string;
  target: string;
  message: string;
  fields?: Record<string, string>;
  span?: Record<string, string>;
}

/** Filter settings sent to the server via WebSocket command. */
export interface LogFilter {
  level?: string;
  target?: string;
}

/** Callbacks for the log stream consumer. */
export interface LogStreamCallbacks {
  onEntry: (entry: LogEntry) => void;
  onLagged: (skipped: number) => void;
  onConnected: () => void;
  onDisconnected: () => void;
}

/**
 * Log streaming service.
 *
 * Manages a WebSocket connection to the daemon's log stream endpoint
 * with auto-reconnect and per-client filter commands.
 */
export class LogService {
  private ws: WebSocket | null = null;
  private callbacks: LogStreamCallbacks | null = null;
  private filter: LogFilter = { level: "info" };
  private paused = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly origin: string;

  /**
   * @param client - The Wardnet HTTP client.
   * @param origin - Page origin for relative base URLs (e.g. "http://localhost:7411").
   *                 Only needed when `client.baseUrl` is relative (browser).
   */
  constructor(
    private readonly client: WardnetClient,
    origin = "http://localhost:7411",
  ) {
    this.origin = origin;
  }

  /**
   * Build the WebSocket URL from the client's base URL.
   *
   * For absolute URLs (Node: "http://host:port/api"), converts http→ws.
   * For relative URLs (browser: "/api"), requires `origin` to be set via
   * constructor option or defaults to "ws://localhost:7411".
   */
  private wsUrl(): string {
    const base = this.client.baseUrl;
    if (base.startsWith("http")) {
      const httpUrl = new URL(base);
      const protocol = httpUrl.protocol === "https:" ? "wss:" : "ws:";
      return `${protocol}//${httpUrl.host}${httpUrl.pathname}/system/logs/stream`;
    }
    // Relative path — use the origin provided at construction.
    const origin = this.origin;
    const protocol = origin.startsWith("https") ? "wss:" : "ws:";
    const url = new URL(origin);
    return `${protocol}//${url.host}${base}/system/logs/stream`;
  }

  /** Start streaming log entries. */
  connect(callbacks: LogStreamCallbacks, initialFilter?: LogFilter): void {
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
        this.callbacks?.onEntry(data as LogEntry);
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

  /** Send a filter change to the server. */
  setFilter(filter: LogFilter): void {
    this.filter = filter;
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "set_filter", ...filter }));
    }
  }

  /** Pause the stream (closes WebSocket, keeps state). */
  pause(): void {
    this.paused = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
  }

  /** Resume the stream (reconnects WebSocket). */
  resume(): void {
    this.paused = false;
    this.doConnect();
  }

  /** Disconnect and clean up. */
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
