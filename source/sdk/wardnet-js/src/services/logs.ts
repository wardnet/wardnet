import type { WardnetClient } from "../client.js";
import { ReconnectingStream } from "./reconnect.js";

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
 * with auto-reconnect (exponential backoff + jitter, via
 * {@link ReconnectingStream}) and per-client filter commands.
 */
export class LogService {
  private readonly stream: ReconnectingStream<LogFilter, LogEntry>;

  /**
   * @param client - The Wardnet HTTP client.
   * @param origin - Page origin for relative base URLs (e.g. "http://localhost:7411").
   *                 Only needed when `client.baseUrl` is relative (browser).
   */
  constructor(client: WardnetClient, origin = "http://localhost:7411") {
    this.stream = new ReconnectingStream(client, "/system/logs/stream", { level: "info" }, origin);
  }

  /** Start streaming log entries. */
  connect(callbacks: LogStreamCallbacks, initialFilter?: LogFilter): void {
    this.stream.start(
      {
        onItem: callbacks.onEntry,
        onLagged: callbacks.onLagged,
        onConnected: callbacks.onConnected,
        onDisconnected: callbacks.onDisconnected,
      },
      initialFilter,
    );
  }

  /** Send a filter change to the server. */
  setFilter(filter: LogFilter): void {
    this.stream.setFilter(filter);
  }

  /** Pause the stream (closes WebSocket, keeps state). */
  pause(): void {
    this.stream.pause();
  }

  /** Resume the stream (reconnects WebSocket). */
  resume(): void {
    this.stream.resume();
  }

  /** Disconnect and clean up. */
  disconnect(): void {
    this.stream.disconnect();
  }
}
