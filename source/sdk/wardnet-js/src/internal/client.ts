// Internal typed HTTP client — an implementation detail of @wardnet/js.
//
// This is the thin fetch layer the issue calls for: a client whose request
// bodies, path/query parameters, and response payloads are all pinned to the
// daemon's OpenAPI contract via the generated `paths` map. The hand-authored
// services (auth, devices, tunnels, …) route their calls through it and map
// between their public types and the generated wire types.
//
// Because every call is keyed by a generated path, a change to the daemon's
// DTOs flows straight into these types when `docs/openapi.json` is regenerated
// — so the mapping code in the services stops type-checking until it's brought
// back in line. Hand-crafted DX on the outside, a compile-time drift tripwire
// underneath.
//
// Nothing here is re-exported from the package entrypoint.

import type { WardnetClient } from "../client.js";
import type { paths } from "./openapi-schema.js";

/**
 * Generated paths, re-keyed without the `/api` transport prefix so call sites
 * read the same relative paths the services have always used (`/auth/login`,
 * `/devices/{id}`). {@link WardnetClient} owns the `/api` base URL.
 */
type RelPaths = {
  [P in keyof paths as P extends `/api${infer Rest}` ? Rest : never]: paths[P];
};

type HttpMethod = "get" | "put" | "post" | "delete" | "patch";

/** Relative paths that define an operation for method `M`. */
type PathsFor<M extends HttpMethod> = {
  [P in keyof RelPaths as RelPaths[P] extends { [K in M]: object } ? P : never]: RelPaths[P];
};

/** The operation object for `path`/`method`, or `never` if undefined. */
type OperationOf<P extends keyof RelPaths, M extends HttpMethod> = M extends keyof RelPaths[P]
  ? RelPaths[P][M]
  : never;

type PathParamsOf<Op> = Op extends { parameters: { path: infer T } } ? T : undefined;

type QueryOf<Op> = Op extends { parameters: { query?: infer T } }
  ? [T] extends [never]
    ? undefined
    : T
  : undefined;

type BodyOf<Op> = Op extends { requestBody: { content: { "application/json": infer T } } }
  ? T
  : Op extends { requestBody?: { content: { "application/json": infer T } } }
    ? T | undefined
    : undefined;

/**
 * The success payload for an operation: the `application/json` body of its
 * 2xx response, or `void` for a bodyless (204) success.
 */
type ResultOf<Op> = Op extends { responses: infer R }
  ? {
      [S in keyof R & number]: S extends 200 | 201 | 202 | 204
        ? R[S] extends { content: { "application/json": infer T } }
          ? T
          : void
        : never;
    }[keyof R & number]
  : void;

type NonNullish<T> = Exclude<T, null | undefined>;

/**
 * Compile-time normalization of a generated response type. openapi-typescript
 * emits `| null`, `| undefined`, and `?` liberally for every optional or
 * nullable wire field; the hand-authored public types clean that up. Stripping
 * that looseness recursively here lets a service return a fetched payload
 * directly as its public type without writing a mapper for every nullable
 * field — while still surfacing *structural* drift: a renamed, removed, or
 * retyped field no longer lines up, and the service's `return` fails to
 * compile until the public type and mapping are brought back in sync.
 */
type ResponseShape<T> = T extends (infer U)[]
  ? ResponseShape<U>[]
  : T extends object
    ? { [K in keyof T]-?: ResponseShape<NonNullish<T[K]>> }
    : T;

/**
 * Per-call options. Every field is optional at the type level to keep call
 * sites terse; the drift tripwire lives in the *types* of `body`, `path`, and
 * the returned payload, not in which fields are required. `query` and `path`
 * carry the operation's parameter shapes; `body` its request DTO.
 */
type RequestOptions<Op> = {
  path?: PathParamsOf<Op>;
  query?: QueryOf<Op>;
  body?: BodyOf<Op>;
  signal?: AbortSignal;
  headers?: Record<string, string>;
};

/** Substitute `{name}` path segments with URL-encoded parameter values. */
function fillPath(path: string, params: Record<string, unknown> | undefined): string {
  if (!params) return path;
  // A Map, not the record itself: lookups are keyed by a placeholder name taken
  // from the path template, and going through `get` keeps that to the object's
  // own entries rather than anything reachable up the prototype chain.
  const lookup = new Map(Object.entries(params));
  return path.replace(/\{([^}]+)\}/g, (_, key: string) => {
    const value = lookup.get(key);
    if (value == null) {
      throw new Error(`Missing path parameter "${key}" for ${path}`);
    }
    return encodeURIComponent(String(value));
  });
}

/**
 * Serialize a query object. Arrays become repeated keys (`?m=a&m=b`), matching
 * the daemon's `serde_html_form` parsing of `Vec<_>` params; `null`/`undefined`
 * values are dropped so optional params stay absent rather than sending the
 * literal string "undefined".
 */
function buildQuery(query: Record<string, unknown> | undefined): string {
  if (!query) return "";
  const parts: string[] = [];
  const enc = encodeURIComponent;
  for (const [key, value] of Object.entries(query)) {
    if (value == null) continue;
    if (Array.isArray(value)) {
      for (const item of value) {
        if (item != null) parts.push(`${enc(key)}=${enc(String(item))}`);
      }
    } else {
      parts.push(`${enc(key)}=${enc(String(value))}`);
    }
  }
  return parts.length > 0 ? `?${parts.join("&")}` : "";
}

/**
 * Typed wrapper over {@link WardnetClient}'s transport. Keyed by the generated
 * relative paths, so `body`/`path` inputs and the returned payload are checked
 * against the wire contract for each endpoint.
 */
export class ApiClient {
  constructor(private readonly client: WardnetClient) {}

  get<P extends keyof PathsFor<"get">>(
    path: P,
    opts?: RequestOptions<OperationOf<P & keyof RelPaths, "get">>,
  ): Promise<ResponseShape<ResultOf<OperationOf<P & keyof RelPaths, "get">>>> {
    return this.send(path, "GET", opts);
  }

  post<P extends keyof PathsFor<"post">>(
    path: P,
    opts?: RequestOptions<OperationOf<P & keyof RelPaths, "post">>,
  ): Promise<ResponseShape<ResultOf<OperationOf<P & keyof RelPaths, "post">>>> {
    return this.send(path, "POST", opts);
  }

  put<P extends keyof PathsFor<"put">>(
    path: P,
    opts?: RequestOptions<OperationOf<P & keyof RelPaths, "put">>,
  ): Promise<ResponseShape<ResultOf<OperationOf<P & keyof RelPaths, "put">>>> {
    return this.send(path, "PUT", opts);
  }

  patch<P extends keyof PathsFor<"patch">>(
    path: P,
    opts?: RequestOptions<OperationOf<P & keyof RelPaths, "patch">>,
  ): Promise<ResponseShape<ResultOf<OperationOf<P & keyof RelPaths, "patch">>>> {
    return this.send(path, "PATCH", opts);
  }

  del<P extends keyof PathsFor<"delete">>(
    path: P,
    opts?: RequestOptions<OperationOf<P & keyof RelPaths, "delete">>,
  ): Promise<ResponseShape<ResultOf<OperationOf<P & keyof RelPaths, "delete">>>> {
    return this.send(path, "DELETE", opts);
  }

  private async send<T>(
    path: string,
    method: string,
    opts?: {
      path?: unknown;
      query?: unknown;
      body?: unknown;
      signal?: AbortSignal;
      headers?: Record<string, string>;
    },
  ): Promise<T> {
    const filled = fillPath(path, opts?.path as Record<string, unknown> | undefined);
    const url = filled + buildQuery(opts?.query as Record<string, unknown> | undefined);
    const init: RequestInit = { method };
    if (opts?.body !== undefined) init.body = JSON.stringify(opts.body);
    if (opts?.signal) init.signal = opts.signal;
    if (opts?.headers) init.headers = opts.headers;
    return this.client.request<T>(url, init);
  }
}

/** Wrap a {@link WardnetClient} in the internal typed client. */
export function apiClient(client: WardnetClient): ApiClient {
  return new ApiClient(client);
}
