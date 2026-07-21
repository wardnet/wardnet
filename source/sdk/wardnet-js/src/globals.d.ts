/**
 * Minimal type declarations for APIs available in both browser and Node 22+.
 *
 * We intentionally exclude DOM/DOM.Iterable from tsconfig to prevent accidental
 * use of browser-only APIs (window, document, etc.) in the SDK. These declarations
 * provide only the types we actually use.
 */

declare function fetch(input: string | URL, init?: RequestInit): Promise<Response>;
declare function setTimeout(callback: () => void, ms: number): ReturnType<typeof setTimeout>;
declare function clearTimeout(id: ReturnType<typeof setTimeout>): void;

/**
 * Console — available in both browsers and Node. Only the methods the
 * default log adapter writes through are declared; consumers wanting richer
 * output swap the sink via `setAdapter` (e.g. the `@wardnet/js/consola`
 * adapter) rather than relying on more of this surface.
 */
declare const console: {
  error(...args: unknown[]): void;
  warn(...args: unknown[]): void;
  info(...args: unknown[]): void;
  debug(...args: unknown[]): void;
};

interface RequestInit {
  method?: string;
  // Mirrors what the platform `fetch` accepts. The SDK's own call sites pass a
  // plain object; the wider union keeps consumers who hand in a `Headers`
  // instance or entries array (valid in the browser) from being silently
  // dropped — `WardnetClient` normalizes all three forms.
  headers?: Record<string, string> | Headers | string[][];
  body?: string | Blob | FormData | null;
  credentials?: "include" | "omit" | "same-origin";
  signal?: AbortSignal;
}

interface Response {
  ok: boolean;
  status: number;
  statusText: string;
  headers: Headers;
  json(): Promise<unknown>;
  text(): Promise<string>;
  blob(): Promise<Blob>;
  arrayBuffer(): Promise<ArrayBuffer>;
}

/**
 * Binary file object — available natively in browsers and Node 18+.
 * The SDK surfaces it as the return type of `BackupService.export`
 * and as the input to `BackupService.previewImport`.
 */
declare class Blob {
  readonly size: number;
  readonly type: string;
  constructor(parts?: Array<ArrayBuffer | Uint8Array | Blob | string>, options?: { type?: string });
  arrayBuffer(): Promise<ArrayBuffer>;
}

/**
 * Multipart form body — available natively in browsers and Node 18+.
 * Required by `BackupService.previewImport` to submit the bundle and
 * passphrase as a `multipart/form-data` request.
 */
declare class FormData {
  constructor();
  append(name: string, value: string | Blob, filename?: string): void;
}

interface Headers {
  get(name: string): string | null;
  has(name: string): boolean;
  forEach(callback: (value: string, name: string) => void): void;
  entries(): IterableIterator<[string, string]>;
}

/**
 * `Headers` constructor — available natively in browsers and Node 18+. The
 * SDK uses it only to normalize a caller-supplied `Headers`/entries value into
 * a plain record; it accepts the same inputs as the platform constructor.
 */
declare const Headers: {
  new (init?: Record<string, string> | Headers | string[][]): Headers;
};

interface AbortSignal {
  aborted: boolean;
}

/** AbortController — available natively in browsers and Node 15+. */
declare class AbortController {
  readonly signal: AbortSignal;
  constructor();
  abort(): void;
}

/** WebSocket — available natively in browsers and Node 22+. */
declare class WebSocket {
  static readonly OPEN: number;
  readonly readyState: number;
  onopen: ((event: unknown) => void) | null;
  onmessage: ((event: { data: string }) => void) | null;
  onclose: ((event: unknown) => void) | null;
  onerror: ((event: unknown) => void) | null;
  constructor(url: string);
  send(data: string): void;
  close(): void;
}

/** URL — available natively in browsers and Node 10+. */
declare class URL {
  readonly protocol: string;
  readonly host: string;
  readonly pathname: string;
  constructor(input: string, base?: string);
}
