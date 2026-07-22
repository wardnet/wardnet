import type { WardnetClient } from "../client.js";

/** Reconnect backoff bounds. The delay starts at BASE and doubles every
 *  consecutive failed attempt, capped at MAX; a successful open resets the
 *  attempt counter. Without this a stream hammered the daemon every 3 s
 *  forever while it was down — a tight loop that floods the console and the
 *  network the moment the daemon is unreachable. */
const RECONNECT_BASE_MS = 1_000;
const RECONNECT_MAX_MS = 30_000;

/** Normalised consumer callbacks shared by every WebSocket stream service.
 *  `onItem` receives an already-parsed payload; `onLagged` fires when the
 *  server reports dropped events. */
export interface StreamHandlers<TItem> {
  onItem: (item: TItem) => void;
  onLagged: (skipped: number) => void;
  onConnected: () => void;
  onDisconnected: () => void;
}

/**
 * Auto-reconnecting WebSocket stream client.
 *
 * Owns the socket lifecycle for the daemon's push endpoints: URL resolution,
 * JSON message dispatch (including the `lagged` control frame), server-side
 * filter commands, and — critically — reconnect scheduling with exponential
 * backoff and jitter so a fleet of clients doesn't fall into lockstep the
 * instant the daemon disappears. Stream services compose this rather than
 * re-implementing the state machine per endpoint.
 */
export class ReconnectingStream<TFilter extends object, TItem> {
  private ws: WebSocket | null = null;
  private handlers: StreamHandlers<TItem> | null = null;
  private filter: TFilter;
  private paused = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  /** Consecutive failed reconnect attempts; drives the backoff delay and is
   *  reset to 0 on a successful open. */
  private reconnectAttempts = 0;
  private readonly origin: string;

  /**
   * @param client - The Wardnet HTTP client.
   * @param path - Endpoint path appended to the API base (e.g. "/system/logs/stream").
   * @param defaultFilter - Filter applied until the consumer supplies its own.
   * @param origin - Page origin for relative base URLs (e.g. "http://localhost:7411").
   *                 Only needed when `client.baseUrl` is relative (browser).
   */
  constructor(
    private readonly client: WardnetClient,
    private readonly path: string,
    defaultFilter: TFilter,
    origin = "http://localhost:7411",
  ) {
    this.filter = defaultFilter;
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
      return `${protocol}//${httpUrl.host}${httpUrl.pathname}${this.path}`;
    }
    // Relative path — use the origin provided at construction.
    const protocol = this.origin.startsWith("https") ? "wss:" : "ws:";
    const url = new URL(this.origin);
    return `${protocol}//${url.host}${base}${this.path}`;
  }

  /** Start streaming. Registers the consumer callbacks and opens the socket. */
  start(handlers: StreamHandlers<TItem>, initialFilter?: TFilter): void {
    this.handlers = handlers;
    if (initialFilter) this.filter = initialFilter;
    this.paused = false;
    this.reconnectAttempts = 0;
    this.doConnect();
  }

  /** Schedule the next reconnect with exponential backoff + jitter. */
  private scheduleReconnect(): void {
    const exp = Math.min(RECONNECT_MAX_MS, RECONNECT_BASE_MS * 2 ** this.reconnectAttempts);
    this.reconnectAttempts += 1;
    // ±25% jitter so a fleet of clients doesn't reconnect in lockstep the
    // instant the daemon comes back.
    const delay = exp * (0.75 + Math.random() * 0.5);
    this.reconnectTimer = setTimeout(() => this.doConnect(), delay);
  }

  private doConnect(): void {
    if (this.paused || !this.handlers) return;

    // Replacing any prior connection: detach and close the old socket and
    // cancel a pending reconnect first, so a repeated connect()/resume() call
    // can't leave an orphaned WebSocket (or timer) firing in the background.
    this.teardownSocket();
    this.clearReconnect();

    const ws = new WebSocket(this.wsUrl());
    this.ws = ws;

    ws.onopen = () => {
      this.reconnectAttempts = 0;
      this.handlers?.onConnected();
      ws.send(JSON.stringify({ type: "set_filter", ...this.filter }));
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.type === "lagged") {
          this.handlers?.onLagged(data.skipped ?? 0);
          return;
        }
        this.handlers?.onItem(data as TItem);
      } catch {
        // Ignore unparseable messages.
      }
    };

    ws.onclose = () => {
      this.handlers?.onDisconnected();
      this.ws = null;
      if (!this.paused) {
        this.scheduleReconnect();
      }
    };

    ws.onerror = () => ws.close();
  }

  /** Send a filter change to the server. */
  setFilter(filter: TFilter): void {
    this.filter = filter;
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "set_filter", ...filter }));
    }
  }

  /** Pause the stream (closes WebSocket, keeps state). */
  pause(): void {
    this.paused = true;
    this.clearReconnect();
    this.ws?.close();
  }

  /** Resume the stream (reconnects WebSocket). */
  resume(): void {
    this.paused = false;
    this.reconnectAttempts = 0;
    this.doConnect();
  }

  /** Disconnect and clean up. */
  disconnect(): void {
    this.paused = true;
    this.clearReconnect();
    this.ws?.close();
    this.ws = null;
    this.handlers = null;
  }

  private clearReconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  /** Detach handlers from the current socket and close it, so its `onclose`
   *  can't fire a stray reconnect or disconnected-callback after we've moved
   *  on. Used when a fresh connection replaces the existing one. */
  private teardownSocket(): void {
    if (!this.ws) return;
    this.ws.onopen = null;
    this.ws.onmessage = null;
    this.ws.onclose = null;
    this.ws.onerror = null;
    this.ws.close();
    this.ws = null;
  }
}
