/**
 * Minimal fetch API type declarations for universal (browser + Node 18+) support.
 *
 * We intentionally exclude DOM/DOM.Iterable from tsconfig to prevent accidental
 * use of browser-only APIs (window, document, etc.) in the SDK. These declarations
 * provide only the fetch-related types we actually use.
 */

declare function fetch(input: string | URL, init?: RequestInit): Promise<Response>;

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
