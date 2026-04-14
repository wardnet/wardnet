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

interface RequestInit {
  method?: string;
  headers?: Record<string, string>;
  body?: string | null;
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
}

interface Headers {
  get(name: string): string | null;
  has(name: string): boolean;
}

interface AbortSignal {
  aborted: boolean;
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
