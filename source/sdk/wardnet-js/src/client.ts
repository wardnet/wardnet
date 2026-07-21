import type { ApiError } from "./types/api.js";
import { logger } from "./logger.js";

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

  /**
   * Build the header set for an outgoing request.
   *
   * This is the single seam a subclass overrides to attach cross-cutting
   * headers — most commonly `Authorization`. Every network call in the SDK
   * routes its header construction through here: the JSON path in `request`,
   * and the raw octet-stream / multipart calls in `BackupService` (which use
   * {@link authorizedFetch} directly because they don't fit the JSON-in /
   * JSON-out shape). Because the seam is shared, an override applies to all
   * of them uniformly instead of silently missing the endpoints that bypass
   * `request`.
   */
  protected buildHeaders(init?: RequestInit): Record<string, string> {
    return { ...init?.headers };
  }

  /**
   * Perform a `fetch` against the daemon API with the client's shared
   * transport policy applied: base-URL prefixing, `credentials: "include"`
   * (so the browser session cookie rides along), and headers routed through
   * {@link buildHeaders} so subclass auth overrides take effect.
   *
   * Callers own the request and response body shapes. `request` uses this
   * for the JSON path; `BackupService` calls it directly for its
   * octet-stream (`export`) and multipart (`previewImport`) endpoints.
   */
  async authorizedFetch(path: string, init?: RequestInit): Promise<Response> {
    return fetch(`${this.baseUrl}${path}`, {
      credentials: "include",
      ...init,
      headers: this.buildHeaders(init),
    });
  }

  /** Send a typed HTTP request to the daemon API.
   *
   * Returns `undefined` (cast to `T`) on `204 No Content` so callers
   * with `Promise<void>` can route through the same path as JSON-
   * returning calls — important because subclasses (e.g. `AuthedClient`
   * in the e2e harness) override `buildHeaders` to attach an
   * `Authorization` header. Bypassing the client means bypassing
   * those overrides; routing through it keeps the auth path uniform.
   */
  async request<T>(path: string, init?: RequestInit): Promise<T> {
    if (init?.body != null) {
      const method = (init.method ?? "GET").toUpperCase();
      if (method === "GET" || method === "HEAD") {
        const err = new Error(
          `WardnetClient.request: ${method} ${path} was called with a body. ` +
            `GET/HEAD requests cannot have a body. Set method: "POST"/"PUT"/"PATCH"/"DELETE" or drop the body.`,
        );
        // Log explicitly — React Query (and other consumers) often swallow
        // the rejection, so the throw alone may not surface in DevTools.
        logger.error(err);
        throw err;
      }
    }

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...init?.headers,
    };
    const res = await this.authorizedFetch(path, { ...init, headers });

    if (!res.ok) {
      const requestId = res.headers.get("X-Request-Id") ?? undefined;
      const body = (await res.json().catch(() => ({
        error: res.statusText,
      }))) as ApiError;
      throw new WardnetApiError(res.status, res.statusText, body, requestId);
    }

    if (res.status === 204) {
      return undefined as T;
    }

    return (await res.json()) as T;
  }
}
