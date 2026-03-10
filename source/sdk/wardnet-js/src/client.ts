import type { ApiError } from "./types/api.js";

/** Options for creating a WardnetClient. */
export interface WardnetClientOptions {
  /** Base URL for API requests. Defaults to "/api" (browser) or "http://localhost:7411/api" (Node). */
  baseUrl?: string;
}

/** Error thrown when an API request fails. */
export class WardnetApiError extends Error {
  /** Server-generated request ID for correlating with server logs. */
  public readonly requestId: string | undefined;

  constructor(
    public readonly status: number,
    public readonly statusText: string,
    public readonly body: ApiError,
    requestId?: string,
  ) {
    super(body.error);
    this.name = "WardnetApiError";
    this.requestId = requestId ?? body.request_id;
  }
}

/**
 * HTTP client for the Wardnet daemon API.
 *
 * Works in both browser and Node 18+ environments using the native fetch API.
 * All service classes use this client for HTTP requests.
 */
export class WardnetClient {
  readonly baseUrl: string;

  constructor(options?: WardnetClientOptions) {
    this.baseUrl = options?.baseUrl ?? "/api";
  }

  /** Send a typed HTTP request to the daemon API. */
  async request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`, {
      headers: { "Content-Type": "application/json" },
      credentials: "include",
      ...init,
    });

    if (!res.ok) {
      const requestId = res.headers.get("X-Request-Id") ?? undefined;
      const body = (await res.json().catch(() => ({
        error: res.statusText,
      }))) as ApiError;
      throw new WardnetApiError(res.status, res.statusText, body, requestId);
    }

    return (await res.json()) as T;
  }
}
